use anyhow::Result;
use std::fs;

use crate::auth::{self, AuthDotJson};
use crate::config::{get_auth_file, get_current_profile_file, get_profiles_dir};

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileSummary {
    pub name: String,
    pub email: Option<String>,
    pub is_current: bool,
}

pub fn list_profiles_data() -> Result<Vec<ProfileSummary>> {
    let profiles_dir = get_profiles_dir()?;

    if !profiles_dir.exists() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use crate::test_support::{EnvGuard, ENV_LOCK};

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
pub fn save_profile(profile_name: &str) -> Result<()> {
    // Load current auth
    let auth = auth::load_auth()?;

    let profiles_dir = get_profiles_dir()?;

    // Create profiles directory
    fs::create_dir_all(&profiles_dir)?;

    let profile_dir = profiles_dir.join(profile_name);

    // Check if profile already exists
    if profile_dir.exists() {
        anyhow::bail!("Profile '{}' already exists. Delete it first.", profile_name);
    }

    // Create profile directory
    fs::create_dir(&profile_dir)?;

    // Save auth to profile
    let profile_auth_file = profile_dir.join("auth.json");
    let auth_json = serde_json::to_string_pretty(&auth)?;
    fs::write(&profile_auth_file, auth_json)?;

    // Set as current if no current profile
    if get_current_profile()?.is_none() {
        save_current_profile(profile_name)?;
    }

    Ok(())
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
