use anyhow::{Context, Result};
use reqwest::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

use crate::auth;

const DEFAULT_USAGE_URL: &str = "https://api.openai.com/v1/usage";
const DEFAULT_CHATGPT_BASE_URL: &str = "https://chatgpt.com/backend-api";
const CODEX_USAGE_PATH: &str = "/api/codex/usage";
const DEFAULT_ORIGINATOR: &str = "codex_cli_rs";

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

#[derive(Debug, Clone, Deserialize)]
struct CodexUsagePayload {
    plan_type: Option<String>,
    #[serde(default)]
    rate_limit: Option<CodexRateLimitStatus>,
    #[serde(default)]
    credits: Option<CodexCreditStatus>,
}

#[derive(Debug, Clone, Deserialize)]
struct CodexRateLimitStatus {
    #[serde(default)]
    primary_window: Option<CodexRateLimitWindowSnapshot>,
    #[serde(default)]
    secondary_window: Option<CodexRateLimitWindowSnapshot>,
}

#[derive(Debug, Clone, Deserialize)]
struct CodexRateLimitWindowSnapshot {
    #[serde(default)]
    used_percent: Option<f64>,
    #[serde(default)]
    reset_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CodexCreditStatus {
    #[serde(default)]
    unlimited: Option<bool>,
    #[serde(default)]
    balance: Option<String>,
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

    let url = if auth.tokens.is_some() {
        join_url(&chatgpt_base_url(), CODEX_USAGE_PATH)
    } else {
        DEFAULT_USAGE_URL.to_string()
    };

    fetch_quota_with_client(&client, auth, &url).await
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

    let is_tokens_flow = auth.tokens.is_some() && auth.openai_api_key.is_none();
    let account_id = auth::get_account_id(auth);

    let mut request = client
        .get(url)
        .header("Authorization", format!("Bearer {}", access_token));

    if is_tokens_flow {
        if let Some(account_id) = account_id.as_deref() {
            request = request.header("chatgpt-account-id", account_id);
        }
        request = request
            .header("originator", DEFAULT_ORIGINATOR)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::USER_AGENT, default_user_agent());
    }

    let response = request.send().await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            if is_tokens_flow {
                let payload: CodexUsagePayload = resp.json().await?;
                Ok(codex_payload_to_quota_info(auth, payload))
            } else {
                let data: serde_json::Value = resp.json().await?;
                parse_quota_response(auth, &data)
            }
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

fn codex_payload_to_quota_info(auth: &auth::AuthDotJson, payload: CodexUsagePayload) -> QuotaInfo {
    let primary_used = payload
        .rate_limit
        .as_ref()
        .and_then(|limit| limit.primary_window.as_ref())
        .and_then(|window| window.used_percent)
        .map(|value| value.clamp(0.0, 100.0).round() as u64);
    let secondary_used = payload
        .rate_limit
        .as_ref()
        .and_then(|limit| limit.secondary_window.as_ref())
        .and_then(|window| window.used_percent)
        .map(|value| value.clamp(0.0, 100.0).round() as u64);
    let reset_date = payload
        .rate_limit
        .as_ref()
        .and_then(|limit| limit.primary_window.as_ref())
        .and_then(|window| window.reset_at.clone());

    let plan_type = payload
        .plan_type
        .or_else(|| auth::get_plan_type(auth))
        .unwrap_or_else(|| "Unknown".to_string());
    let plan_type = format_plan_type_with_credits(plan_type, payload.credits.as_ref());

    QuotaInfo {
        account_id: auth::get_account_id(auth).unwrap_or_default(),
        email: auth::get_email(auth).unwrap_or_default(),
        plan_type,
        used_requests: primary_used,
        total_requests: primary_used.map(|_| 100),
        used_tokens: secondary_used,
        total_tokens: secondary_used.map(|_| 100),
        reset_date,
    }
}

fn format_plan_type_with_credits(plan_type: String, credits: Option<&CodexCreditStatus>) -> String {
    let Some(credits) = credits else {
        return plan_type;
    };
    if credits.unlimited.unwrap_or(false) {
        return format!("{plan_type} (credits: unlimited)");
    }
    let Some(balance) = credits.balance.as_deref() else {
        return plan_type;
    };
    format!("{plan_type} (credits: {balance})")
}

fn chatgpt_base_url() -> String {
    env::var("CODEX_ROUTER_CHATGPT_BASE_URL").unwrap_or_else(|_| DEFAULT_CHATGPT_BASE_URL.to_string())
}

fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    format!("{}/{}", base, path)
}

fn default_user_agent() -> String {
    format!(
        "codex_router/{} ({}/{})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
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

    #[tokio::test]
    async fn fetch_quota_parses_codex_usage_and_sends_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (req_tx, req_rx) = std::sync::mpsc::channel::<String>();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                let n = stream.read(&mut chunk).unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
                if buf.len() > 8192 {
                    break;
                }
            }
            let _ = req_tx.send(String::from_utf8_lossy(&buf).to_string());

            let body = r#"{"plan_type":"pro","rate_limit":{"allowed":true,"limit_reached":false,"primary_window":{"used_percent":25.0,"limit_window_seconds":3600,"reset_after_seconds":120,"reset_at":"2026-01-13T00:00:00Z"},"secondary_window":{"used_percent":10.0,"limit_window_seconds":86400,"reset_after_seconds":3600,"reset_at":"2026-01-14T00:00:00Z"}},"credits":{"has_credits":true,"unlimited":false,"balance":"12.34"}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let auth = auth::AuthDotJson {
            openai_api_key: None,
            tokens: Some(auth::TokenData {
                id_token: None,
                access_token: "access".to_string(),
                refresh_token: "refresh".to_string(),
                account_id: Some("acct_123".to_string()),
            }),
            last_refresh: None,
        };

        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let url = format!("http://{}/api/codex/usage", addr);

        let quota = fetch_quota_with_client(&client, &auth, &url).await.unwrap();

        let request = req_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap()
            .to_lowercase();
        assert!(
            request.starts_with("get /api/codex/usage "),
            "unexpected request line: {request:?}"
        );
        assert!(request.contains("authorization: bearer access"));
        assert!(request.contains("chatgpt-account-id: acct_123"));
        assert!(request.contains("originator: codex_cli_rs"));

        assert_eq!(quota.account_id, "acct_123");
        assert_eq!(quota.plan_type, "pro (credits: 12.34)");
        assert_eq!(quota.used_requests, Some(25));
        assert_eq!(quota.total_requests, Some(100));
        assert_eq!(quota.used_tokens, Some(10));
        assert_eq!(quota.total_tokens, Some(100));
        assert_eq!(quota.reset_date.as_deref(), Some("2026-01-13T00:00:00Z"));

        server.join().unwrap();
    }
}
