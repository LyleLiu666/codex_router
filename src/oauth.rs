use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use tokio::time::sleep;

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const SCOPE: &str = "openid profile email offline_access";
const AUDIENCE: &str = "https://api.openai.com/v1/"; // Standard OpenAI audience

fn get_auth_domain() -> String {
    std::env::var("CODEX_ROUTER_AUTH_DOMAIN")
        .unwrap_or_else(|_| "https://auth.openai.com".to_string())
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TokenResponse {
    Success {
        access_token: String,
        refresh_token: String,
        id_token: Option<String>,
        #[serde(rename = "expires_in")]
        _expires_in: u64,
    },
    Error {
        error: String,
        #[serde(rename = "error_description")]
        _error_description: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct AuthResult {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: Option<String>,
}

pub async fn start_login_flow<F>(on_code: F) -> Result<AuthResult>
where
    F: Fn(String, String) + Send + Sync, // (user_code, verification_uri)
{
    let client = Client::builder()
        .user_agent(crate::config::default_user_agent())
        .build()?;

    // 1. Request Device Code
    let device_code_url = format!("{}/oauth/device/code", get_auth_domain());
    let params = [
        ("client_id", CLIENT_ID),
        ("scope", SCOPE),
        ("audience", AUDIENCE),
    ];

    let resp = client
        .post(&device_code_url)
        .form(&params)
        .send()
        .await
        .context("Failed to request device code")?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Device code request failed: {}", text);
    }

    let code_res: DeviceCodeResponse = resp
        .json()
        .await
        .context("Failed to parse device code response")?;

    // 2. Notify User & Open Browser
    on_code(
        code_res.user_code.clone(),
        code_res.verification_uri.clone(),
    );

    // Try to open browser automatically
    let url_to_open = code_res
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&code_res.verification_uri);

    let _ = open::that(url_to_open);

    // 3. Poll for Token
    let token_url = format!("{}/oauth/token", get_auth_domain());
    let expiry = std::time::Instant::now() + Duration::from_secs(code_res.expires_in);
    let mut interval = Duration::from_secs(code_res.interval);

    while std::time::Instant::now() < expiry {
        sleep(interval).await;

        let params = [
            ("client_id", CLIENT_ID),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", &code_res.device_code),
        ];

        let resp = client.post(&token_url).form(&params).send().await;

        match resp {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    let token_res: TokenResponse = response.json().await?;
                    match token_res {
                        TokenResponse::Success {
                            access_token,
                            refresh_token,
                            id_token,
                            ..
                        } => {
                            return Ok(AuthResult {
                                access_token,
                                refresh_token,
                                id_token,
                            });
                        }
                        TokenResponse::Error { error, .. } => {
                            if error != "authorization_pending" && error != "slow_down" {
                                anyhow::bail!("Login failed: {}", error);
                            }
                            if error == "slow_down" {
                                interval += Duration::from_secs(5);
                            }
                        }
                    }
                } else {
                    // Start checking error body for authorization_pending
                    let body: serde_json::Value = response.json().await.unwrap_or_default();
                    if let Some(error) = body["error"].as_str() {
                        if error != "authorization_pending" && error != "slow_down" {
                            anyhow::bail!("Login failed: {}", error);
                        }
                        if error == "slow_down" {
                            interval += Duration::from_secs(5);
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Token poll request failed: {}", e);
            }
        }
    }

    anyhow::bail!("Login timed out")
}
