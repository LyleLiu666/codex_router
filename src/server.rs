use axum::{
    extract::{Json, State},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::auth;
use crate::profile::ProfileSummary;
use crate::shared::SharedState;

pub async fn start_server(state: Arc<SharedState>) {
    // Add CORS layer to allow all origins/methods/headers for local dev
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let app = Router::new()
        .route("/v1/chat/completions", post(handle_chat_completions))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 9876));
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct ChatRequest {
    model: String,
    reasoning_effort: Option<String>,
    #[serde(default)]
    messages: Vec<serde_json::Value>,
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

async fn handle_chat_completions(
    State(state): State<Arc<SharedState>>,
    Json(mut payload): Json<ChatRequest>,
) -> Response {
    // 1. Model Logic Adjustment
    if payload.model.to_lowercase().contains("mini") {
        let current_effort = payload.reasoning_effort.as_deref().unwrap_or("medium");
        if ["low", "xhigh"].contains(&current_effort) {
            payload.reasoning_effort = Some("medium".to_string());
            tracing::info!("Adjusted reasoning_effort to medium for mini model");
        }
    }

    // 2. Select Candidates
    let profiles = state.profiles.read().unwrap().clone();
    let candidates = select_candidates(profiles);

    if candidates.is_empty() {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "No available accounts with quota"})),
        )
            .into_response();
    }

    // 3. Try Candidates
    let client = reqwest::Client::new();

    // Reconstruct body
    let body_json = serde_json::to_value(&payload).unwrap();
    // Flatten extra fields back if needed, but serde already handled it in struct via flattened `extra`.
    // Actually, `serde_json::to_value` on `ChatRequest` will produce the correct structure including flattened extra.

    for profile in candidates {
        tracing::info!("Trying profile: {}", profile.name);

        // Load auth
        let auth = match crate::profile::load_profile_auth(&profile.name) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("Failed to load auth for {}: {}", profile.name, e);
                continue;
            }
        };

        // Determine token and headers
        let access_token = match auth.tokens.as_ref().map(|t| t.access_token.clone()) {
            Some(t) => t,
            None => {
                if let Some(key) = &auth.openai_api_key {
                    key.clone()
                } else {
                    continue;
                }
            }
        };

        // TODO: This URL logic is simplified. Real logic might need to respect CODEX_ROUTER_CHATGPT_BASE_URL.
        // Assuming typical OpenAI compatible endpoint for now, or the one from api.rs constants.
        // api.rs has `DEFAULT_CHATGPT_BASE_URL`.
        // Let's use `https://api.openai.com/v1/chat/completions` as default fallback or check env.
        // Actually, the user requirement mentions `gpt-5.2-codex` etc. which implies it might proxy to Codex backend or OpenAI.
        // If it's Codex backend, we need `ChatGPT-Account-Id`.

        let base_url = std::env::var("CODEX_ROUTER_CHATGPT_BASE_URL")
            .unwrap_or_else(|_| "https://chatgpt.com/backend-api".to_string());
        // Try the fallback-style path first as it seems more likely common
        let url = format!("{}/codex/response", base_url.trim_end_matches('/'));
        tracing::info!("Using upstream URL: {}", url);

        let mut req = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&body_json);

        if let Some(account_id) = auth::get_account_id(&auth) {
            req = req.header("ChatGPT-Account-Id", account_id);
        }

        match req.send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    // Success! Stream response back.
                    let status = resp.status();
                    let headers = resp.headers().clone();
                    let body = axum::body::Body::from_stream(resp.bytes_stream());

                    let mut builder = Response::builder().status(status);
                    for (key, value) in &headers {
                        builder = builder.header(key, value);
                    }
                    return builder.body(body).unwrap_or_default();
                } else if resp.status() == 429 {
                    tracing::warn!("Profile {} rate limited (429), trying next", profile.name);
                    continue;
                } else if resp.status() == 401 || resp.status() == 403 {
                    tracing::warn!(
                        "Profile {} auth error ({}), trying next",
                        profile.name,
                        resp.status()
                    );
                    continue;
                } else {
                    // Other errors (500 etc), maybe temporary, or fatal?
                    // Let's assume we try next for robustness.
                    tracing::warn!(
                        "Profile {} error {}, trying next",
                        profile.name,
                        resp.status()
                    );
                    continue;
                }
            }
            Err(e) => {
                tracing::warn!("Profile {} network error: {}, trying next", profile.name, e);
                continue;
            }
        }
    }

    (
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({"error": "All accounts failed or exhausted"})),
    )
        .into_response()
}

fn select_candidates(profiles: Vec<ProfileSummary>) -> Vec<ProfileSummary> {
    let mut candidates: Vec<ProfileSummary> = profiles
        .into_iter()
        .filter(|p| {
            if let Some(quota) = &p.quota {
                let used = quota.used_tokens.unwrap_or(0);
                let total = quota.total_tokens.unwrap_or(100);
                used < total
            } else {
                false
            }
        })
        .collect();

    candidates.sort_by(|a, b| {
        let date_a = a
            .quota
            .as_ref()
            .and_then(|q| q.secondary_reset_date.as_deref())
            .unwrap_or("z"); // "z" to put None at end
        let date_b = b
            .quota
            .as_ref()
            .and_then(|q| q.secondary_reset_date.as_deref())
            .unwrap_or("z");
        date_a.cmp(date_b)
    });

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::QuotaInfo;

    fn mock_profile(name: &str, used: u64, total: u64, reset: Option<&str>) -> ProfileSummary {
        ProfileSummary {
            name: name.to_string(),
            email: None,
            is_current: false,
            is_valid: true,
            quota: Some(QuotaInfo {
                account_id: "id".to_string(),
                email: "email".to_string(),
                plan_type: "plan".to_string(),
                used_requests: None,
                total_requests: None,
                used_tokens: Some(used),
                total_tokens: Some(total),
                reset_date: None,
                secondary_reset_date: reset.map(|s| s.to_string()),
            }),
        }
    }

    #[test]
    fn test_select_candidates_sorts_by_reset_date() {
        let p1 = mock_profile("p1", 50, 100, Some("2026-01-20T10:00:00Z"));
        let p2 = mock_profile("p2", 50, 100, Some("2026-01-19T10:00:00Z")); // Earlier
        let p3 = mock_profile("p3", 50, 100, Some("2026-01-21T10:00:00Z"));

        let candidates = select_candidates(vec![p1, p2, p3]);
        assert_eq!(candidates[0].name, "p2");
        assert_eq!(candidates[1].name, "p1");
        assert_eq!(candidates[2].name, "p3");
    }

    #[test]
    fn test_select_candidates_filters_exhausted() {
        let p1 = mock_profile("p1", 50, 100, Some("2026-01-20T10:00:00Z"));
        let p2 = mock_profile("p2", 100, 100, Some("2026-01-19T10:00:00Z")); // Exhausted

        let candidates = select_candidates(vec![p1, p2]);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "p1");
    }
}
