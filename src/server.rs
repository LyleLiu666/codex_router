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

    // 3. Prepare Request for Upstream
    // Extract instructions and input from messages
    let mut instructions: Option<String> = None;
    let mut input_items: Vec<serde_json::Value> = Vec::new();

    for msg in payload.messages.into_iter() {
        if let Some(role) = msg.get("role").and_then(|r| r.as_str()) {
            if role == "system" {
                instructions = msg
                    .get("content")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string());
            } else {
                input_items.push(msg);
            }
        }
    }

    // Construct the new request body for the /codex/responses endpoint
    use crate::codex_types::{ContentPart, Reasoning, ResponseItem, ResponsesApiRequest};

    let responses_req = ResponsesApiRequest {
        model: payload.model.clone(),
        instructions: instructions.unwrap_or_default(),
        input: input_items
            .into_iter()
            .map(|v| {
                let role = v
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("user")
                    .to_string();
                let content_str = v
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();
                ResponseItem::Message {
                    id: Some(format!("msg_{}", uuid::Uuid::new_v4().simple())),
                    role,
                    content: vec![ContentPart::Text { text: content_str }],
                }
            })
            .collect(),
        tools: vec![],
        tool_choice: "auto".to_string(),
        parallel_tool_calls: false,
        reasoning: payload.reasoning_effort.map(|effort| Reasoning { effort }),
        store: false,
        stream: true,
        include: vec![],
        prompt_cache_key: None,
        text: None,
    };

    let body_json = serde_json::to_value(&responses_req).unwrap();
    tracing::info!("Sending payload: {}", body_json);

    // 4. Try Candidates
    let client = reqwest::Client::new();

    for profile in candidates {
        tracing::info!("Trying profile: {}", profile.name);

        let auth = match crate::profile::load_profile_auth(&profile.name) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("Failed to load auth for {}: {}", profile.name, e);
                continue;
            }
        };

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

        let base_url = std::env::var("CODEX_ROUTER_CHATGPT_BASE_URL")
            .unwrap_or_else(|_| "https://chatgpt.com/backend-api".to_string());

        let url = format!("{}/codex/responses", base_url.trim_end_matches('/'));
        tracing::info!("Using upstream URL: {}", url);

        let mut req = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("originator", "codex_cli_rs")
            .header("User-Agent", "codex-cli")
            .json(&body_json);

        if let Some(account_id) = auth::get_account_id(&auth) {
            req = req.header("ChatGPT-Account-Id", account_id);
        }

        match req.send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    let status = resp.status();
                    let headers = resp.headers().clone();
                    let body = axum::body::Body::from_stream(resp.bytes_stream());

                    let mut builder = Response::builder().status(status);
                    for (key, value) in &headers {
                        builder = builder.header(key, value);
                    }
                    return builder.body(body).unwrap_or_default();
                } else {
                    let status = resp.status();
                    let error_text = resp.text().await.unwrap_or_default();
                    tracing::warn!(
                        "Profile {} error {}, body: {}, trying next",
                        profile.name,
                        status,
                        error_text
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

    #[test]
    fn test_select_candidates_includes_profiles_without_quota() {
        let profiles = vec![ProfileSummary {
            name: "p1".to_string(),
            email: None,
            is_current: false,
            is_valid: true,
            quota: None,
        }];

        let candidates = select_candidates(profiles);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "p1");
    }
}
