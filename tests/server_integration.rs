use axum::extract::{Json, State};
use axum::response::IntoResponse;
use codex_router::{
    api::QuotaInfo,
    profile::ProfileSummary,
    server::{handle_chat_completions, ChatRequest},
    shared::SharedState,
};
use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;
use std::sync::{Arc, RwLock};
use tempfile::TempDir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvVarGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let original = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(original) = &self.original {
            std::env::set_var(self.key, original);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

#[tokio::test]
async fn test_auto_switching_on_failure() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());

    // 1. Setup Wiremock
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    // 2. Setup Mock Environment (CODEX_HOME) for Profile Loading
    let temp_dir = TempDir::new().unwrap();
    let _base_url_guard = EnvVarGuard::set("CODEX_ROUTER_CHATGPT_BASE_URL", &base_url);
    let _codex_home_guard = EnvVarGuard::set("CODEX_HOME", temp_dir.path());

    // Create Profile Directories and Auth Files
    create_profile(temp_dir.path(), "p1", "token1");
    create_profile(temp_dir.path(), "p2", "token2");

    // Highest "used_tokens" is preferred first (least remaining). If it fails, it should fall back.
    // p2 is tried first -> 503, then p1 succeeds -> 200.
    Mock::given(method("POST"))
        .and(path("/codex/responses"))
        .and(header("Authorization", "Bearer token2"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&mock_server)
        .await;

    // P1 calls succeeds
    Mock::given(method("POST"))
        .and(path("/codex/responses"))
        .and(header("Authorization", "Bearer token1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "msg_success"})),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    // 3. Setup SharedState with Profiles
    let profiles = vec![
        mock_profile_summary("p1", 10),
        mock_profile_summary("p2", 90),
    ];

    let state = Arc::new(SharedState {
        profiles: Arc::new(RwLock::new(profiles)),
    });

    // 4. Invoke Handler
    let req = ChatRequest {
        model: "gpt-5.2-codex".to_string(),
        reasoning_effort: Some("medium".to_string()),
        messages: vec![serde_json::json!({"role": "user", "content": "hi"})],
        extra: HashMap::new(),
    };

    let response = handle_chat_completions(State(state), Json(req)).await;

    // 5. Assert
    let status = response.into_response().status();
    assert_eq!(status, 200);
}

fn create_profile(codex_home: &std::path::Path, name: &str, token: &str) {
    let dir = codex_home.join("profiles").join(name);
    fs::create_dir_all(&dir).unwrap();

    let auth_json = serde_json::json!({
        "tokens": {
            "access_token": token,
            "refresh_token": "refresh",
            "account_id": format!("acct_{}", name)
        }
    });
    fs::write(dir.join("auth.json"), auth_json.to_string()).unwrap();
}

fn mock_profile_summary(name: &str, used_tokens: u64) -> ProfileSummary {
    ProfileSummary {
        name: name.to_string(),
        email: Some(format!("{}@example.com", name)),
        is_current: false,
        is_valid: true,
        quota: Some(QuotaInfo {
            account_id: format!("acct_{}", name),
            email: format!("{}@example.com", name),
            plan_type: "plus".to_string(),
            used_requests: Some(0),
            total_requests: Some(100),
            used_tokens: Some(used_tokens),
            total_tokens: Some(100),
            reset_date: None,
            secondary_reset_date: None,
        }),
    }
}
