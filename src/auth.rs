use anyhow::{Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;

use crate::config::{get_auth_file, get_codex_home};

/// Auth data structure matching Codex's auth.json format
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthDotJson {
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenData>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,
}

/// Token data structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TokenData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<IdToken>,
    pub access_token: String,
    pub refresh_token: String,
    pub account_id: Option<String>,
}

/// ID Token info from JWT
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IdTokenInfo {
    pub email: Option<String>,
    #[serde(rename = "chatgpt_plan_type")]
    pub chatgpt_plan_type: Option<String>,
    #[serde(rename = "chatgpt_account_id")]
    pub chatgpt_account_id: Option<String>,
    #[serde(
        rename = "https://api.openai.com/auth",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub openai_auth: Option<OpenAiAuthClaims>,
    pub raw_jwt: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum IdToken {
    Raw(String),
    Info(IdTokenInfo),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenAiAuthClaims {
    pub chatgpt_plan_type: Option<String>,
    pub chatgpt_account_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct JwtClaims {
    pub email: Option<String>,
    #[serde(rename = "https://api.openai.com/auth")]
    pub openai_auth: Option<OpenAiAuthClaims>,
}

/// Load auth from the active auth.json file
pub fn load_auth() -> Result<AuthDotJson> {
    let auth_file = get_auth_file()?;

    if !auth_file.exists() {
        anyhow::bail!("Not logged in. Please run 'codex login' first.");
    }

    let content = fs::read_to_string(&auth_file)
        .with_context(|| format!("Failed to read auth file: {:?}", auth_file))?;

    let auth: AuthDotJson = serde_json::from_str(&content)
        .with_context(|| "Failed to parse auth.json")?;

    Ok(auth)
}

/// Save auth to the active auth.json file
pub fn save_auth(auth: &AuthDotJson) -> Result<()> {
    let auth_file = get_auth_file()?;

    // Create parent directory if it doesn't exist
    if let Some(parent) = auth_file.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(auth)?;

    // Write with restrictive permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&auth_file)?.permissions();
        perms.set_mode(0o600);
    }

    fs::write(&auth_file, json)?;

    Ok(())
}

/// Load auth from a profile directory
pub fn load_auth_from_profile(profile_name: &str) -> Result<AuthDotJson> {
    let codex_home = get_codex_home()?;
    let profile_auth_file = codex_home
        .join("profiles")
        .join(profile_name)
        .join("auth.json");

    if !profile_auth_file.exists() {
        anyhow::bail!("Profile '{}' not found", profile_name);
    }

    let content = fs::read_to_string(&profile_auth_file)
        .with_context(|| format!("Failed to read profile auth file: {:?}", profile_auth_file))?;

    let auth: AuthDotJson = serde_json::from_str(&content)
        .with_context(|| "Failed to parse profile auth.json")?;

    Ok(auth)
}

fn decode_jwt_claims(raw_jwt: &str) -> Option<IdTokenInfo> {
    let payload = raw_jwt.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: JwtClaims = serde_json::from_slice(&decoded).ok()?;
    let (chatgpt_plan_type, chatgpt_account_id) = claims
        .openai_auth
        .map(|auth| (auth.chatgpt_plan_type, auth.chatgpt_account_id))
        .unwrap_or((None, None));

    Some(IdTokenInfo {
        email: claims.email,
        chatgpt_plan_type,
        chatgpt_account_id,
        openai_auth: None,
        raw_jwt: Some(raw_jwt.to_string()),
    })
}

fn get_id_token_info(auth: &AuthDotJson) -> Option<IdTokenInfo> {
    let id_token = auth.tokens.as_ref()?.id_token.as_ref()?;
    match id_token {
        IdToken::Info(info) => Some(info.clone()),
        IdToken::Raw(raw) => decode_jwt_claims(raw),
    }
}

fn get_plan_from_info(info: &IdTokenInfo) -> Option<String> {
    info.chatgpt_plan_type.clone().or_else(|| {
        info.openai_auth
            .as_ref()
            .and_then(|auth| auth.chatgpt_plan_type.clone())
    })
}

fn get_account_from_info(info: &IdTokenInfo) -> Option<String> {
    info.chatgpt_account_id.clone().or_else(|| {
        info.openai_auth
            .as_ref()
            .and_then(|auth| auth.chatgpt_account_id.clone())
    })
}

/// Get account ID from auth
pub fn get_account_id(auth: &AuthDotJson) -> Option<String> {
    auth.tokens
        .as_ref()
        .and_then(|t| t.account_id.clone())
        .or_else(|| get_id_token_info(auth).and_then(|info| get_account_from_info(&info)))
}

/// Get email from auth
pub fn get_email(auth: &AuthDotJson) -> Option<String> {
    get_id_token_info(auth).and_then(|info| info.email.clone())
}

/// Get plan type from auth
pub fn get_plan_type(auth: &AuthDotJson) -> Option<String> {
    get_id_token_info(auth).and_then(|info| get_plan_from_info(&info))
}

/// Format auth info for display
pub fn format_auth_info(auth: &AuthDotJson) -> String {
    let email = get_email(auth).unwrap_or_else(|| "N/A".to_string());
    let account_id = get_account_id(auth).unwrap_or_else(|| "N/A".to_string());
    let plan = get_plan_type(auth).unwrap_or_else(|| "N/A".to_string());

    format!("Email: {}\nAccount ID: {}\nPlan: {}", email, account_id, plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{EnvGuard, ENV_LOCK};
    use std::fs;

    #[test]
    fn loads_auth_with_jwt_id_token() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

        let jwt = "eyJhbGciOiJub25lIn0.eyJlbWFpbCI6InVzZXJAZXhhbXBsZS5jb20iLCJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9wbGFuX3R5cGUiOiJwcm8iLCJjaGF0Z3B0X2FjY291bnRfaWQiOiJhY2N0XzEyMyJ9fQ.sig";
        let auth_json = serde_json::json!({
            "OPENAI_API_KEY": null,
            "tokens": {
                "access_token": "access",
                "refresh_token": "refresh",
                "account_id": "acct_123",
                "id_token": jwt
            }
        });

        fs::write(temp_dir.path().join("auth.json"), auth_json.to_string()).unwrap();

        let auth = load_auth().unwrap();
        assert_eq!(get_email(&auth), Some("user@example.com".to_string()));
        assert_eq!(get_account_id(&auth), Some("acct_123".to_string()));
        assert_eq!(get_plan_type(&auth), Some("pro".to_string()));
    }
}
