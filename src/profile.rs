use anyhow::Result;
use std::fs;

use crate::auth::{self, AuthDotJson, IdToken};
use crate::config::{get_auth_file, get_current_profile_file, get_profiles_dir};

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileSummary {
    pub name: String,
    pub email: Option<String>,
    pub is_current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveProfileOutcome {
    Created { name: String },
    Updated { name: String },
    AlreadyExists { name: String },
}

pub fn list_profiles_data() -> Result<Vec<ProfileSummary>> {
    let profiles_dir = get_profiles_dir()?;

    if !profiles_dir.exists() {
        fs::create_dir_all(&profiles_dir)?;
        return Ok(Vec::new());
    }

    let current_profile = get_current_profile()?;
    let mut profiles = Vec::new();

    for entry in fs::read_dir(&profiles_dir)? {
        let entry = entry?;
        if !entry.path().is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        let profile_auth_file = entry.path().join("auth.json");
        let email = fs::read_to_string(&profile_auth_file)
            .ok()
            .and_then(|auth_json| serde_json::from_str::<AuthDotJson>(&auth_json).ok())
            .and_then(|auth| auth::get_email(&auth));

        let is_current = current_profile.as_deref() == Some(name.as_str());
        profiles.push(ProfileSummary {
            name,
            email,
            is_current,
        });
    }

    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(profiles)
}

fn token_fingerprint(auth: &AuthDotJson) -> Option<String> {
    if let Some(key) = &auth.openai_api_key {
        return Some(key.clone());
    }
    let tokens = auth.tokens.as_ref()?;
    let id_token = tokens.id_token.as_ref().and_then(|token| match token {
        IdToken::Raw(raw) => Some(raw.clone()),
        IdToken::Info(info) => info.raw_jwt.clone(),
    });
    Some(format!(
        "{}|{}|{}",
        tokens.access_token,
        tokens.refresh_token,
        id_token.unwrap_or_default()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{IdToken, TokenData};
    use crate::test_support::{EnvGuard, ENV_LOCK};
    use std::fs;

    #[test]
    fn lists_profiles_with_current_marker() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

        let profiles_dir = temp_dir.path().join("profiles");
        fs::create_dir_all(profiles_dir.join("alpha")).unwrap();
        fs::create_dir_all(profiles_dir.join("beta")).unwrap();
        fs::write(profiles_dir.join("alpha").join("auth.json"), "{}").unwrap();
        fs::write(profiles_dir.join("beta").join("auth.json"), "{}").unwrap();
        fs::write(temp_dir.path().join(".current_profile"), "beta").unwrap();

        let profiles = list_profiles_data().unwrap();

        assert!(profiles.iter().any(|p| p.name == "beta" && p.is_current));
    }

    #[test]
    fn list_profiles_creates_profiles_dir_when_missing() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

        let profiles_dir = temp_dir.path().join("profiles");
        assert!(!profiles_dir.exists());

        let profiles = list_profiles_data().unwrap();

        assert!(profiles.is_empty());
        assert!(profiles_dir.exists());
    }

    #[test]
    fn token_fingerprint_prefers_api_key() {
        let auth = AuthDotJson {
            openai_api_key: Some("sk-test".to_string()),
            tokens: None,
            last_refresh: None,
        };

        assert_eq!(token_fingerprint(&auth), Some("sk-test".to_string()));
    }

    #[test]
    fn token_fingerprint_uses_tokens_when_no_api_key() {
        let auth = AuthDotJson {
            openai_api_key: None,
            tokens: Some(TokenData {
                id_token: Some(IdToken::Raw("id.raw".to_string())),
                access_token: "access".to_string(),
                refresh_token: "refresh".to_string(),
                account_id: Some("acct_123".to_string()),
            }),
            last_refresh: None,
        };

        assert_eq!(
            token_fingerprint(&auth),
            Some("access|refresh|id.raw".to_string())
        );
    }

    #[test]
    fn save_profile_noops_when_account_id_matches_and_token_same() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

        let auth = AuthDotJson {
            openai_api_key: None,
            tokens: Some(TokenData {
                id_token: Some(IdToken::Raw("id.raw".to_string())),
                access_token: "access".to_string(),
                refresh_token: "refresh".to_string(),
                account_id: Some("acct_123".to_string()),
            }),
            last_refresh: None,
        };

        fs::write(
            temp_dir.path().join("auth.json"),
            serde_json::to_string_pretty(&auth).unwrap(),
        )
        .unwrap();

        let profiles_dir = temp_dir.path().join("profiles");
        fs::create_dir_all(profiles_dir.join("work")).unwrap();
        fs::write(
            profiles_dir.join("work").join("auth.json"),
            serde_json::to_string_pretty(&auth).unwrap(),
        )
        .unwrap();

        let outcome = save_profile("new").unwrap();

        assert_eq!(
            outcome,
            SaveProfileOutcome::AlreadyExists {
                name: "work".to_string()
            }
        );
    }

    #[test]
    fn save_profile_overwrites_when_account_id_matches_and_token_changes() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

        let old_auth = AuthDotJson {
            openai_api_key: None,
            tokens: Some(TokenData {
                id_token: Some(IdToken::Raw("id.old".to_string())),
                access_token: "access-old".to_string(),
                refresh_token: "refresh-old".to_string(),
                account_id: Some("acct_123".to_string()),
            }),
            last_refresh: None,
        };

        let new_auth = AuthDotJson {
            openai_api_key: None,
            tokens: Some(TokenData {
                id_token: Some(IdToken::Raw("id.new".to_string())),
                access_token: "access-new".to_string(),
                refresh_token: "refresh-new".to_string(),
                account_id: Some("acct_123".to_string()),
            }),
            last_refresh: None,
        };

        fs::write(
            temp_dir.path().join("auth.json"),
            serde_json::to_string_pretty(&new_auth).unwrap(),
        )
        .unwrap();

        let profiles_dir = temp_dir.path().join("profiles");
        fs::create_dir_all(profiles_dir.join("work")).unwrap();
        fs::write(
            profiles_dir.join("work").join("auth.json"),
            serde_json::to_string_pretty(&old_auth).unwrap(),
        )
        .unwrap();

        let outcome = save_profile("ignored").unwrap();

        assert_eq!(
            outcome,
            SaveProfileOutcome::Updated {
                name: "work".to_string()
            }
        );

        let updated = fs::read_to_string(profiles_dir.join("work").join("auth.json")).unwrap();
        let updated_value: serde_json::Value = serde_json::from_str(&updated).unwrap();
        let expected_value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string_pretty(&new_auth).unwrap()).unwrap();
        assert_eq!(updated_value, expected_value);
    }
}

/// Switch to a profile
pub async fn switch_profile(profile_name: &str) -> Result<()> {
    let profiles_dir = get_profiles_dir()?;

    let profile_auth_file = profiles_dir.join(profile_name).join("auth.json");

    if !profile_auth_file.exists() {
        anyhow::bail!("Profile '{}' not found.", profile_name);
    }

    // Read profile auth
    let profile_auth = fs::read_to_string(&profile_auth_file)?;
    let auth: AuthDotJson = serde_json::from_str(&profile_auth)?;

    // Save to main auth.json
    let main_auth_file = get_auth_file()?;
    if let Some(parent) = main_auth_file.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&main_auth_file, serde_json::to_string_pretty(&auth)?)?;

    // Update current profile marker
    save_current_profile(profile_name)?;

    Ok(())
}

/// Save current auth as a profile
pub fn save_profile(profile_name: &str) -> Result<SaveProfileOutcome> {
    // Load current auth
    let auth = auth::load_auth()?;

    let profiles_dir = get_profiles_dir()?;

    // Create profiles directory
    fs::create_dir_all(&profiles_dir)?;

    if let Some(account_id) = auth::get_account_id(&auth) {
        for entry in fs::read_dir(&profiles_dir)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }
            let existing_name = entry.file_name().to_string_lossy().to_string();
            let existing_auth_file = entry.path().join("auth.json");
            let existing_auth = fs::read_to_string(&existing_auth_file)
                .ok()
                .and_then(|contents| serde_json::from_str::<AuthDotJson>(&contents).ok());
            let Some(existing_auth) = existing_auth else {
                continue;
            };
            if auth::get_account_id(&existing_auth).as_deref() != Some(account_id.as_str()) {
                continue;
            }

            let incoming_fp = token_fingerprint(&auth);
            let existing_fp = token_fingerprint(&existing_auth);
            if incoming_fp == existing_fp {
                save_current_profile(&existing_name)?;
                return Ok(SaveProfileOutcome::AlreadyExists {
                    name: existing_name,
                });
            }

            fs::write(&existing_auth_file, serde_json::to_string_pretty(&auth)?)?;
            save_current_profile(&existing_name)?;
            return Ok(SaveProfileOutcome::Updated { name: existing_name });
        }
    }

    let profile_dir = profiles_dir.join(profile_name);
    if profile_dir.exists() {
        anyhow::bail!("Profile '{}' already exists. Delete it first.", profile_name);
    }

    fs::create_dir(&profile_dir)?;
    let profile_auth_file = profile_dir.join("auth.json");
    fs::write(&profile_auth_file, serde_json::to_string_pretty(&auth)?)?;

    if get_current_profile()?.is_none() {
        save_current_profile(profile_name)?;
    }

    Ok(SaveProfileOutcome::Created {
        name: profile_name.to_string(),
    })
}

/// Save the provided auth as a profile without switching the current profile.
pub fn save_auth_as_profile_without_switch(auth: &AuthDotJson) -> Result<SaveProfileOutcome> {
    let profiles_dir = get_profiles_dir()?;
    fs::create_dir_all(&profiles_dir)?;

    if let Some(account_id) = auth::get_account_id(auth) {
        for entry in fs::read_dir(&profiles_dir)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }
            let existing_name = entry.file_name().to_string_lossy().to_string();
            let existing_auth_file = entry.path().join("auth.json");
            let existing_auth = fs::read_to_string(&existing_auth_file)
                .ok()
                .and_then(|contents| serde_json::from_str::<AuthDotJson>(&contents).ok());
            let Some(existing_auth) = existing_auth else {
                continue;
            };
            if auth::get_account_id(&existing_auth).as_deref() != Some(account_id.as_str()) {
                continue;
            }

            let incoming_fp = token_fingerprint(auth);
            let existing_fp = token_fingerprint(&existing_auth);
            if incoming_fp == existing_fp {
                return Ok(SaveProfileOutcome::AlreadyExists {
                    name: existing_name,
                });
            }

            fs::write(&existing_auth_file, serde_json::to_string_pretty(auth)?)?;
            return Ok(SaveProfileOutcome::Updated { name: existing_name });
        }
    }

    let base_name = suggested_profile_name(auth);
    for attempt in 0..100 {
        let candidate = if attempt == 0 {
            base_name.clone()
        } else {
            format!("{base_name}-{}", attempt + 1)
        };
        let profile_dir = profiles_dir.join(&candidate);
        if profile_dir.exists() {
            continue;
        }
        fs::create_dir(&profile_dir)?;
        let profile_auth_file = profile_dir.join("auth.json");
        fs::write(&profile_auth_file, serde_json::to_string_pretty(auth)?)?;
        return Ok(SaveProfileOutcome::Created { name: candidate });
    }

    anyhow::bail!("Failed to find available profile name starting with '{base_name}'");
}

fn suggested_profile_name(auth: &AuthDotJson) -> String {
    let raw = auth::get_email(auth)
        .and_then(|email| email.split('@').next().map(|value| value.to_string()))
        .or_else(|| auth::get_account_id(auth))
        .unwrap_or_else(|| "profile".to_string());
    sanitize_profile_name(&raw)
}

fn sanitize_profile_name(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;

    for ch in input.chars() {
        let normalized = match ch {
            'a'..='z' | '0'..='9' | '_' => Some(ch),
            'A'..='Z' => Some(ch.to_ascii_lowercase()),
            '-' => Some('-'),
            _ => None,
        };

        match normalized {
            Some('-') => {
                if !prev_dash && !out.is_empty() {
                    out.push('-');
                }
                prev_dash = true;
            }
            Some(ch) => {
                out.push(ch);
                prev_dash = false;
            }
            None => {
                if !prev_dash && !out.is_empty() {
                    out.push('-');
                    prev_dash = true;
                }
            }
        }
    }

    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() {
        "profile".to_string()
    } else {
        out
    }
}

/// Delete a profile
pub fn delete_profile(profile_name: &str) -> Result<()> {
    let profiles_dir = get_profiles_dir()?;

    let profile_dir = profiles_dir.join(profile_name);

    if !profile_dir.exists() {
        anyhow::bail!("Profile '{}' not found.", profile_name);
    }

    // Don't allow deleting the current profile
    if let Some(current) = get_current_profile()? {
        if current == profile_name {
            anyhow::bail!(
                "Cannot delete the current profile. Switch to another profile first."
            );
        }
    }

    fs::remove_dir_all(&profile_dir)?;

    Ok(())
}

/// Get the current profile name
fn get_current_profile() -> Result<Option<String>> {
    let current_file = get_current_profile_file()?;

    if !current_file.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&current_file)?;
    Ok(Some(content.trim().to_string()))
}

/// Save the current profile name
fn save_current_profile(profile_name: &str) -> Result<()> {
    let current_file = get_current_profile_file()?;

    if let Some(parent) = current_file.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&current_file, profile_name)?;

    Ok(())
}
