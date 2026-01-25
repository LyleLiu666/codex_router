use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const AUTH_BASE_URL: &str = "https://auth.openai.com";
const CALLBACK_PORT: u16 = 1455;

/// Result of a successful login
#[derive(Debug, Clone)]
pub struct AuthResult {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: Option<String>,
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

/// Generate PKCE code_verifier and code_challenge
fn generate_pkce() -> (String, String) {
    let mut rng = rand::rng();
    let code_verifier: String = (0..64)
        .map(|_| {
            let idx = rng.random_range(0..62);
            let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
            chars[idx] as char
        })
        .collect();

    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let hash = hasher.finalize();
    let code_challenge = URL_SAFE_NO_PAD.encode(hash);

    (code_verifier, code_challenge)
}

/// Generate random state for CSRF protection
fn generate_state() -> String {
    let mut rng = rand::rng();
    (0..32)
        .map(|_| {
            let idx = rng.random_range(0..62);
            let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
            chars[idx] as char
        })
        .collect()
}

/// Build the authorization URL
fn build_auth_url(code_challenge: &str, state: &str, redirect_uri: &str) -> String {
    let scope = "openid profile email offline_access";
    format!(
        "{}/oauth/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&id_token_add_organizations=true&codex_cli_simplified_flow=true&state={}&originator=codex_cli_rs",
        AUTH_BASE_URL,
        CLIENT_ID,
        urlencoding::encode(redirect_uri),
        urlencoding::encode(scope),
        code_challenge,
        state
    )
}

/// Extract authorization code from callback request
fn extract_code_from_request(request: &str) -> Option<(String, String)> {
    // Parse: GET /auth/callback?code=...&state=... HTTP/1.1
    let first_line = request.lines().next()?;
    let path = first_line.split_whitespace().nth(1)?;

    if !path.starts_with("/auth/callback") && !path.starts_with("/callback") {
        return None;
    }

    let query_start = path.find('?')?;
    let query = &path[query_start + 1..];

    let mut code = None;
    let mut state = None;

    for param in query.split('&') {
        if let Some((key, value)) = param.split_once('=') {
            match key {
                "code" => code = Some(urlencoding::decode(value).ok()?.into_owned()),
                "state" => state = Some(urlencoding::decode(value).ok()?.into_owned()),
                _ => {}
            }
        }
    }

    Some((code?, state?))
}

/// Send success response to browser
fn send_success_response(stream: &mut std::net::TcpStream) {
    let body = r#"<!DOCTYPE html>
<html>
<head><title>Login Successful</title></head>
<body style="font-family: -apple-system, BlinkMacSystemFont, sans-serif; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #1a1a1a; color: #fff;">
<div style="text-align: center;">
<h1 style="color: #10a37f;">✓ Login Successful</h1>
<p>You can close this window and return to the application.</p>
</div>
</body>
</html>"#;

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// Send error response to browser
fn send_error_response(stream: &mut std::net::TcpStream, message: &str) {
    let body = format!(
        r#"<!DOCTYPE html>
<html>
<head><title>Login Failed</title></head>
<body style="font-family: -apple-system, BlinkMacSystemFont, sans-serif; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: #1a1a1a; color: #fff;">
<div style="text-align: center;">
<h1 style="color: #ef4444;">✗ Login Failed</h1>
<p>{}</p>
</div>
</body>
</html>"#,
        message
    );

    let response = format!(
        "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// Exchange authorization code for tokens
async fn exchange_code_for_tokens(
    client: &Client,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<TokenExchangeResp> {
    let url = format!("{}/oauth/token", AUTH_BASE_URL);

    let body = TokenExchangeReq {
        grant_type: "authorization_code".to_string(),
        client_id: CLIENT_ID.to_string(),
        redirect_uri: redirect_uri.to_string(),
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

/// Start browser-based OAuth login flow
pub fn start_browser_login<F>(on_status: F, cancel_flag: Arc<AtomicBool>) -> Result<AuthResult>
where
    F: Fn(String) + Send + Sync,
{
    // 1. Start local callback server
    let listener = TcpListener::bind(format!("127.0.0.1:{}", CALLBACK_PORT)).context(format!(
        "Failed to start callback server on port {}",
        CALLBACK_PORT
    ))?;
    listener.set_nonblocking(true)?;

    let redirect_uri = format!("http://localhost:{}/auth/callback", CALLBACK_PORT);

    // 2. Generate PKCE and state
    let (code_verifier, code_challenge) = generate_pkce();
    let state = generate_state();

    // 3. Build and open authorization URL
    let auth_url = build_auth_url(&code_challenge, &state, &redirect_uri);
    on_status(format!("Opening browser for login..."));

    if let Err(e) = open::that(&auth_url) {
        on_status(format!(
            "Failed to open browser: {}. Please open this URL manually:\n{}",
            e, auth_url
        ));
    }

    on_status(format!(
        "Waiting for login callback on port {}...",
        CALLBACK_PORT
    ));

    // 4. Wait for callback
    let timeout = Duration::from_secs(5 * 60); // 5 minute timeout
    let start = std::time::Instant::now();
    let mut auth_code = None;

    loop {
        if cancel_flag.load(Ordering::Relaxed) {
            anyhow::bail!("Login cancelled by user");
        }

        if start.elapsed() > timeout {
            anyhow::bail!("Login timed out after 5 minutes");
        }

        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut reader = BufReader::new(stream.try_clone()?);
                let mut request = String::new();

                // Read request line and headers
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty() {
                        break;
                    }
                    request.push_str(&line);
                }

                // Try to extract code
                if let Some((code, received_state)) = extract_code_from_request(&request) {
                    if received_state != state {
                        send_error_response(&mut stream, "State mismatch - possible CSRF attack");
                        continue;
                    }

                    send_success_response(&mut stream);
                    auth_code = Some(code);
                    break;
                } else if request.contains("error=") {
                    send_error_response(&mut stream, "Authentication was denied");
                    anyhow::bail!("Authentication was denied by user");
                } else {
                    // Ignore other requests (favicon, etc)
                    let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                    let _ = stream.write_all(response.as_bytes());
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                anyhow::bail!("Failed to accept connection: {}", e);
            }
        }
    }

    let code = auth_code.context("No authorization code received")?;
    on_status("Exchanging authorization code for tokens...".to_string());

    // 5. Exchange code for tokens (need to run async in sync context)
    let runtime = tokio::runtime::Runtime::new()?;
    let client = Client::builder()
        .user_agent(crate::config::default_user_agent())
        .build()?;

    let tokens = runtime.block_on(async {
        exchange_code_for_tokens(&client, &code, &code_verifier, &redirect_uri).await
    })?;

    Ok(AuthResult {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        id_token: tokens.id_token,
    })
}
