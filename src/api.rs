use anyhow::{Context, Result};
use reqwest::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::auth;

const DEFAULT_USAGE_URL: &str = "https://api.openai.com/v1/usage";

/// Quota information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QuotaInfo {
    pub account_id: String,
    pub email: String,
    pub plan_type: String,
    pub used_requests: Option<u64>,
    pub total_requests: Option<u64>,
    pub used_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub reset_date: Option<String>,
}

impl QuotaInfo {
}

/// Check quota for the current profile
pub async fn check_quota() -> Result<QuotaInfo> {
    let auth = auth::load_auth()?;
    fetch_quota(&auth).await
}

/// Fetch quota for a given auth profile
pub async fn fetch_quota(auth: &auth::AuthDotJson) -> Result<QuotaInfo> {
    let _ = auth
        .tokens
        .as_ref()
        .map(|t| t.access_token.clone())
        .or_else(|| auth.openai_api_key.clone())
        .context("No valid token found")?;
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    fetch_quota_with_client(&client, auth, DEFAULT_USAGE_URL).await
}

/// Watch quota with auto-refresh (deprecated in UI mode)
pub async fn watch_quota() -> Result<()> {
    anyhow::bail!("watch_quota is only available in the desktop UI")
}

/// Fetch quota from Codex API
async fn fetch_quota_with_client(
    client: &Client,
    auth: &auth::AuthDotJson,
    url: &str,
) -> Result<QuotaInfo> {
    let access_token = auth
        .tokens
        .as_ref()
        .map(|t| t.access_token.clone())
        .or_else(|| auth.openai_api_key.clone())
        .context("No valid token found")?;

    // Note: The actual quota endpoint may be different
    // This is a placeholder implementation
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            let data: serde_json::Value = resp.json().await?;
            parse_quota_response(auth, &data)
        }
        Ok(resp) => {
            let status = resp.status();
            let error = resp.text().await.unwrap_or_default();
            if status == StatusCode::UNAUTHORIZED && auth.openai_api_key.is_none() && auth.tokens.is_some() {
                return get_fallback_quota(auth);
            }
            anyhow::bail!("API returned status {}: {}", status, error);
        }
        Err(_e) => {
            // If the quota endpoint doesn't work, return a mock response based on auth data
            get_fallback_quota(auth)
        }
    }
}

/// Parse quota API response
fn parse_quota_response(auth: &auth::AuthDotJson, data: &serde_json::Value) -> Result<QuotaInfo> {
    Ok(QuotaInfo {
        account_id: auth::get_account_id(auth).unwrap_or_default(),
        email: auth::get_email(auth).unwrap_or_default(),
        plan_type: auth::get_plan_type(auth).unwrap_or_else(|| "Unknown".to_string()),
        used_requests: data["data"]["usage"][0]["n_requests"].as_u64(),
        total_requests: None, // API may not provide this
        used_tokens: data["data"]["usage"][0]["n_tokens"].as_u64(),
        total_tokens: None,
        reset_date: None,
    })
}

/// Get fallback quota info when API is unavailable
fn get_fallback_quota(auth: &auth::AuthDotJson) -> Result<QuotaInfo> {
    Ok(QuotaInfo {
        account_id: auth::get_account_id(auth).unwrap_or_default(),
        email: auth::get_email(auth).unwrap_or_default(),
        plan_type: auth::get_plan_type(auth).unwrap_or_else(|| "Unknown".to_string()),
        used_requests: None,
        total_requests: None,
        used_tokens: None,
        total_tokens: None,
        reset_date: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn auth_stub() -> auth::AuthDotJson {
        auth::AuthDotJson {
            openai_api_key: None,
            tokens: None,
            last_refresh: None,
        }
    }

    #[test]
    fn parses_quota_response() {
        let data = serde_json::json!({
            "data": { "usage": [ { "n_requests": 5, "n_tokens": 10 } ] }
        });
        let info = parse_quota_response(&auth_stub(), &data).unwrap();
        assert_eq!(info.used_requests, Some(5));
        assert_eq!(info.used_tokens, Some(10));
    }

    #[tokio::test]
    async fn fetch_quota_errors_without_token() {
        let err = fetch_quota(&auth_stub()).await.unwrap_err();
        assert!(err.to_string().contains("No valid token"));
    }

    #[tokio::test]
    async fn fetch_quota_falls_back_on_invalid_api_key_when_using_tokens() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = r#"{"error":{"message":"Incorrect API key provided","type":"invalid_request_error","code":"invalid_api_key"}}"#;
            let response = format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let auth = auth::AuthDotJson {
            openai_api_key: None,
            tokens: Some(auth::TokenData {
                id_token: None,
                access_token: "eyJhbGci".to_string(),
                refresh_token: "refresh".to_string(),
                account_id: Some("acct_123".to_string()),
            }),
            last_refresh: None,
        };
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let url = format!("http://{}/v1/usage", addr);

        let quota = fetch_quota_with_client(&client, &auth, &url).await.unwrap();
        assert_eq!(quota.account_id, "acct_123");
        assert!(quota.used_requests.is_none());
        assert!(quota.used_tokens.is_none());

        server.join().unwrap();
    }
}
