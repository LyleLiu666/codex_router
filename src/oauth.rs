use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::time::sleep;

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const AUTH_BASE_URL: &str = "https://auth.openai.com";

/// Result of a successful login
#[derive(Debug, Clone)]
pub struct AuthResult {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: Option<String>,
}

// Request/Response types for OpenAI's proprietary device auth
#[derive(Serialize)]
struct UserCodeReq {
    client_id: String,
}

#[derive(Deserialize)]
struct UserCodeResp {
    device_auth_id: String,
    #[serde(alias = "user_code", alias = "usercode")]
    user_code: String,
    #[serde(default)]
    interval: u64,
}

#[derive(Serialize)]
struct TokenPollReq {
    device_auth_id: String,
    user_code: String,
}

#[derive(Deserialize)]
struct CodeSuccessResp {
    authorization_code: String,
    code_challenge: String,
    code_verifier: String,
}

#[derive(Serialize)]
struct TokenExchangeReq {
    grant_type: String,
    client_id: String,
    redirect_uri: String,
    code: String,
    code_verifier: String,
}

#[derive(Deserialize)]
struct TokenExchangeResp {
    access_token: String,
    refresh_token: String,
    id_token: Option<String>,
}

/// Request user code from OpenAI's device auth endpoint
async fn request_user_code(client: &Client) -> Result<UserCodeResp> {
    let url = format!("{}/api/accounts/deviceauth/usercode", AUTH_BASE_URL);
    let body = serde_json::to_string(&UserCodeReq {
        client_id: CLIENT_ID.to_string(),
    })?;

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .context("Failed to request user code")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Device code request failed ({}): {}", status, text);
    }

    let text = resp.text().await?;
    serde_json::from_str(&text).context("Failed to parse user code response")
}

/// Poll token endpoint until user authorizes
async fn poll_for_token(
    client: &Client,
    device_auth_id: &str,
    user_code: &str,
    interval: u64,
) -> Result<CodeSuccessResp> {
    let url = format!("{}/api/accounts/deviceauth/token", AUTH_BASE_URL);
    let max_wait = Duration::from_secs(15 * 60);
    let start = Instant::now();
    let poll_interval = if interval > 0 { interval } else { 5 };

    loop {
        sleep(Duration::from_secs(poll_interval)).await;

        let body = serde_json::to_string(&TokenPollReq {
            device_auth_id: device_auth_id.to_string(),
            user_code: user_code.to_string(),
        })?;

        let resp = client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .context("Token poll request failed")?;

        let status = resp.status();

        if status.is_success() {
            let text = resp.text().await?;
            return serde_json::from_str(&text).context("Failed to parse token response");
        }

        // 403/404 means user hasn't authorized yet
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::NOT_FOUND {
            if start.elapsed() >= max_wait {
                anyhow::bail!("Device auth timed out after 15 minutes");
            }
            continue;
        }

        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Device auth failed ({}): {}", status, text);
    }
}

/// Exchange authorization code for tokens
async fn exchange_code_for_tokens(
    client: &Client,
    code: &str,
    code_verifier: &str,
) -> Result<TokenExchangeResp> {
    let redirect_uri = format!("{}/deviceauth/callback", AUTH_BASE_URL);
    let url = format!("{}/oauth/token", AUTH_BASE_URL);

    let body = TokenExchangeReq {
        grant_type: "authorization_code".to_string(),
        client_id: CLIENT_ID.to_string(),
        redirect_uri,
        code: code.to_string(),
        code_verifier: code_verifier.to_string(),
    };

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Token exchange request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Token exchange failed ({}): {}", status, text);
    }

    resp.json()
        .await
        .context("Failed to parse token exchange response")
}

/// Start the device code login flow
/// Returns DeviceCode for the caller to display, then continues polling in background
pub async fn start_login_flow<F>(on_code: F) -> Result<AuthResult>
where
    F: Fn(String, String) + Send + Sync, // (user_code, verification_uri)
{
    let client = Client::builder()
        .user_agent(crate::config::default_user_agent())
        .build()?;

    // 1. Request device code
    let uc = request_user_code(&client).await?;

    let verification_url = format!("{}/codex/device", AUTH_BASE_URL);

    // 2. Notify user with code and URL
    on_code(uc.user_code.clone(), verification_url.clone());

    // Try to open browser automatically
    let _ = open::that(&verification_url);

    // 3. Poll for authorization
    let code_resp = poll_for_token(&client, &uc.device_auth_id, &uc.user_code, uc.interval).await?;

    // 4. Exchange code for tokens
    let tokens = exchange_code_for_tokens(
        &client,
        &code_resp.authorization_code,
        &code_resp.code_verifier,
    )
    .await?;

    Ok(AuthResult {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        id_token: tokens.id_token,
    })
}
