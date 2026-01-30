use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

use crate::{auth, config};

const DEFAULT_USAGE_URL: &str = "https://api.openai.com/v1/usage";
const DEFAULT_CHATGPT_BASE_URL: &str = "https://chatgpt.com/backend-api";
const CODEX_USAGE_PATH: &str = "/api/codex/usage";
const CODEX_USAGE_FALLBACK_PATH: &str = "/codex/usage";
const DEFAULT_CHATGPT_FALLBACK_BASE_URL: &str = "https://chat.openai.com/backend-api";
const DEFAULT_ORIGINATOR: &str = "codex_cli_rs";

// OAuth token refresh constants
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

/// Quota information
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct QuotaInfo {
    pub account_id: String,
    pub email: String,
    pub plan_type: String,
    pub used_requests: Option<u64>,
    pub total_requests: Option<u64>,
    pub used_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub reset_date: Option<String>,
    pub secondary_reset_date: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Token expired")]
    Expired,
}

impl QuotaInfo {}

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
#[serde(untagged)]
enum CodexResetAt {
    IsoString(String),
    EpochSeconds(i64),
}

impl CodexResetAt {
    fn as_rfc3339(&self) -> String {
        match self {
            CodexResetAt::IsoString(value) => value.clone(),
            CodexResetAt::EpochSeconds(value) => epoch_seconds_to_rfc3339(*value),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CodexRateLimitWindowSnapshot {
    #[serde(default)]
    used_percent: Option<f64>,
    #[serde(default)]
    reset_at: Option<CodexResetAt>,
    #[serde(default)]
    reset_after_seconds: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct CodexCreditStatus {
    #[serde(default)]
    unlimited: Option<bool>,
    #[serde(default)]
    balance: Option<String>,
}

/// Check quota for the current profile
#[allow(dead_code)]
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
    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;

    if auth.tokens.is_some() && auth.openai_api_key.is_none() {
        let mut failures: Vec<anyhow::Error> = Vec::new();
        for url in codex_usage_urls() {
            match fetch_quota_with_client(&client, auth, &url).await {
                Ok(quota) => return Ok(quota),
                Err(err) => {
                    tracing::warn!(url = %url, error = %err, "quota request failed");
                    failures.push(err);
                }
            }
        }
        if !failures.is_empty() {
            if failures
                .iter()
                .any(|e| e.downcast_ref::<AuthError>().is_some())
            {
                return Err(AuthError::Expired.into());
            }
            anyhow::bail!(
                "All quota endpoints failed:\n{}",
                failures
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        anyhow::bail!("No quota endpoints configured");
    }

    fetch_quota_with_client(&client, auth, DEFAULT_USAGE_URL).await
}

/// Watch quota with auto-refresh (deprecated in UI mode)
#[allow(dead_code)]
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
            request = request.header("ChatGPT-Account-Id", account_id);
        }
        request = request
            .header("originator", DEFAULT_ORIGINATOR)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::USER_AGENT, config::default_user_agent());
    }

    let response = request.send().await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            if is_tokens_flow {
                let status = resp.status();
                let content_type = resp
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("-")
                    .to_string();
                let bytes = resp.bytes().await.with_context(|| {
                    format!(
                        "Failed to read response body from {} (status {}, content-type {})",
                        url, status, content_type
                    )
                })?;
                let payload: CodexUsagePayload = serde_json::from_slice(&bytes).map_err(|err| {
                    let preview = body_preview(&bytes);
                    anyhow::anyhow!(
                        "Failed to parse JSON from {} (status {}, content-type {}): {}. Body preview: {}",
                        url,
                        status,
                        content_type,
                        err,
                        preview
                    )
                })?;
                Ok(codex_payload_to_quota_info(auth, payload))
            } else {
                let data: serde_json::Value = resp.json().await?;
                parse_quota_response(auth, &data)
            }
        }
        Ok(resp) => {
            let status = resp.status();
            let error = resp.text().await.unwrap_or_default();
            if status == StatusCode::UNAUTHORIZED {
                if auth.openai_api_key.is_none()
                    && auth.tokens.is_some()
                    && should_fallback_on_unauthorized(&error)
                {
                    return get_fallback_quota(auth);
                }
                return Err(AuthError::Expired.into());
            }
            anyhow::bail!("API returned status {} for {}: {}", status, url, error);
        }
        Err(e) => {
            anyhow::bail!("API request failed for {}: {}", url, e);
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
        .and_then(|window| {
            calculate_reset_time(window.reset_at.as_ref(), window.reset_after_seconds)
        });
    let secondary_reset_date = payload
        .rate_limit
        .as_ref()
        .and_then(|limit| limit.secondary_window.as_ref())
        .and_then(|window| {
            calculate_reset_time(window.reset_at.as_ref(), window.reset_after_seconds)
        });

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
        secondary_reset_date,
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

fn codex_usage_urls() -> Vec<String> {
    let configured = env::var("CODEX_ROUTER_CHATGPT_BASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let base_urls = if let Some(configured) = configured {
        vec![configured]
    } else {
        vec![
            DEFAULT_CHATGPT_BASE_URL.to_string(),
            DEFAULT_CHATGPT_FALLBACK_BASE_URL.to_string(),
        ]
    };

    let mut urls = Vec::new();
    for base in base_urls {
        // Prefer the shorter/non-API path first; some deployments expose it without the /api prefix.
        for path in [CODEX_USAGE_FALLBACK_PATH, CODEX_USAGE_PATH] {
            let url = join_url(&base, path);
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
    }
    urls
}

fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    format!("{}/{}", base, path)
}

fn should_fallback_on_unauthorized(body: &str) -> bool {
    body.contains("\"invalid_api_key\"") || body.contains("Incorrect API key provided")
}

fn calculate_reset_time(
    reset_at: Option<&CodexResetAt>,
    reset_after_seconds: Option<i64>,
) -> Option<String> {
    if let Some(seconds) = reset_after_seconds {
        let now = Utc::now();
        let future = now + chrono::Duration::seconds(seconds);
        return Some(future.to_rfc3339_opts(SecondsFormat::Secs, true));
    }

    reset_at.map(CodexResetAt::as_rfc3339)
}

fn epoch_seconds_to_rfc3339(raw: i64) -> String {
    let seconds = if raw >= 1_000_000_000_000 {
        raw / 1000
    } else {
        raw
    };

    DateTime::<Utc>::from_timestamp(seconds, 0)
        .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_else(|| seconds.to_string())
}

fn body_preview(bytes: &[u8]) -> String {
    let preview_len = bytes.len().min(256);
    let preview = String::from_utf8_lossy(&bytes[..preview_len]);
    preview
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .trim()
        .to_string()
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
        secondary_reset_date: None,
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
        secondary_reset_date: None,
    })
}

// OAuth token refresh types

#[derive(Serialize)]
struct RefreshRequest<'a> {
    client_id: &'a str,
    grant_type: &'a str,
    refresh_token: &'a str,
    scope: &'a str,
}

#[derive(Deserialize)]
pub struct RefreshResponse {
    pub _id_token: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

/// Refresh expired OAuth tokens using the refresh_token grant
pub async fn refresh_token(refresh_token: &str) -> Result<RefreshResponse> {
    let request = RefreshRequest {
        client_id: CLIENT_ID,
        grant_type: "refresh_token",
        refresh_token,
        scope: "openid profile email",
    };

    let client = Client::builder()
        .user_agent(config::default_user_agent())
        .timeout(Duration::from_secs(30))
        .build()?;

    let refresh_url = format!("{}/oauth/token", crate::config::get_auth_domain());

    let response = client
        .post(&refresh_url)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .context("Failed to send refresh token request")?;

    let status = response.status();
    if status.is_success() {
        let refresh_response: RefreshResponse = response
            .json()
            .await
            .context("Failed to parse refresh token response")?;
        Ok(refresh_response)
    } else {
        let body = response.text().await.unwrap_or_default();
        if status == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Refresh token expired or invalid: {}", body);
        }
        anyhow::bail!("Failed to refresh token: {} - {}", status, body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ENV_LOCK;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    struct StringEnvGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl StringEnvGuard {
        fn unset(key: &'static str) -> Self {
            let original = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, original }
        }
    }

    impl Drop for StringEnvGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.original {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

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

            let body = r#"{"plan_type":"pro","rate_limit":{"allowed":true,"limit_reached":false,"primary_window":{"used_percent":25.0,"limit_window_seconds":3600,"reset_at":"2026-01-13T00:00:00Z"},"secondary_window":{"used_percent":10.0,"limit_window_seconds":86400,"reset_at":"2026-01-14T00:00:00Z"}},"credits":{"has_credits":true,"unlimited":false,"balance":"12.34"}}"#;
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
        assert!(request.contains("user-agent: codex-cli"));

        assert_eq!(quota.account_id, "acct_123");
        assert_eq!(quota.plan_type, "pro (credits: 12.34)");
        assert_eq!(quota.used_requests, Some(25));
        assert_eq!(quota.total_requests, Some(100));
        assert_eq!(quota.used_tokens, Some(10));
        assert_eq!(quota.total_tokens, Some(100));
        assert_eq!(quota.reset_date.as_deref(), Some("2026-01-13T00:00:00Z"));

        server.join().unwrap();
    }

    #[test]
    fn codex_usage_urls_prefers_non_api_path_first() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = StringEnvGuard::unset("CODEX_ROUTER_CHATGPT_BASE_URL");

        let urls = codex_usage_urls();

        assert_eq!(urls[0], "https://chatgpt.com/backend-api/codex/usage");
        assert_eq!(urls[1], "https://chatgpt.com/backend-api/api/codex/usage");
        assert!(urls.contains(&"https://chat.openai.com/backend-api/codex/usage".to_string()));
        assert!(urls.contains(
            &"https://chat.openai.com/backend-api/api/codex/usage".to_string()
        ));
    }

    #[tokio::test]
    async fn fetch_quota_includes_url_in_error_message() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = r#"{"detail":"Not Found"}"#;
            let response = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
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

        let err = fetch_quota_with_client(&client, &auth, &url)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(&url), "missing url: {msg}");
        assert!(msg.contains("404"), "missing status: {msg}");

        server.join().unwrap();
    }

    #[tokio::test]
    async fn fetch_quota_includes_url_when_json_decode_fails() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = "not-json";
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

        let err = fetch_quota_with_client(&client, &auth, &url)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(&url), "missing url: {msg}");

        server.join().unwrap();
    }

    #[tokio::test]
    async fn fetch_quota_parses_codex_usage_with_epoch_reset_at() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = r#"{"plan_type":"team","rate_limit":{"allowed":true,"limit_reached":false,"primary_window":{"used_percent":19,"limit_window_seconds":18000,"reset_at":1735689600},"secondary_window":{"used_percent":10,"limit_window_seconds":86400,"reset_at":1735689600}}}"#;
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
        let url = format!("http://{}/codex/usage", addr);

        let quota = fetch_quota_with_client(&client, &auth, &url).await.unwrap();
        assert_eq!(quota.plan_type, "team");
        assert_eq!(quota.used_requests, Some(19));
        assert_eq!(quota.reset_date.as_deref(), Some("2025-01-01T00:00:00Z"));

        server.join().unwrap();
    }

    #[test]
    fn calculates_reset_time_from_reset_after_seconds() {
        let reset_at = Some(CodexResetAt::EpochSeconds(1000));
        let reset_after_seconds = Some(3600);

        let result = calculate_reset_time(reset_at.as_ref(), reset_after_seconds);
        assert!(result.is_some());

        // Check if the time is roughly 1 hour from now
        let parsed = DateTime::parse_from_rfc3339(&result.unwrap()).unwrap();
        let now = Utc::now();
        let diff = parsed.with_timezone(&Utc) - now;
        assert!(diff.num_seconds() > 3590 && diff.num_seconds() < 3610);
    }
}
