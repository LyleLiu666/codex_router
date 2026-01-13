# Codex Router Desktop UI Implementation Plan
> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the CLI with a lightweight macOS desktop app (eframe/egui) that supports fast account switching and quota viewing with a menu bar tray.

**Architecture:** Keep existing core modules for auth/profile/api/config but refactor them to return structured data. Introduce AppState + background worker (tokio) with command/event channels. UI renders from AppState and never blocks on IO.

**Tech Stack:** Rust, eframe/egui, tray-icon, tokio, reqwest, serde.

### Task 1: Persist router UI state (refresh interval, auto-refresh, last profile)

**Files:**
- Create: `src/state.rs`
- Modify: `src/config.rs`
- Modify: `Cargo.toml`
- Test: `src/state.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn loads_default_state_when_missing() {
    std::env::set_var("CODEX_HOME", temp_dir());
    let state = crate::state::load_state().unwrap();
    assert_eq!(state.refresh_interval_seconds, 600);
    assert_eq!(state.auto_refresh_enabled, true);
    assert!(state.last_selected_profile.is_none());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test state::loads_default_state_when_missing -v`  
Expected: FAIL (module/function missing)

**Step 3: Write minimal implementation**

```rust
#[derive(Default, Serialize, Deserialize)]
pub struct RouterState {
    pub refresh_interval_seconds: u64,
    pub auto_refresh_enabled: bool,
    pub last_selected_profile: Option<String>,
}

pub fn load_state() -> Result<RouterState> { /* read ~/.codex/router/state.json or default */ }
pub fn save_state(state: &RouterState) -> Result<()> { /* create dir + write JSON */ }
```

**Step 4: Run test to verify it passes**

Run: `cargo test state::loads_default_state_when_missing -v`  
Expected: PASS

**Step 5: Commit**

```bash
git add src/state.rs src/config.rs Cargo.toml
git commit -m "feat: add router state persistence"
```

### Task 2: Refactor profile management to return data (no printing)

**Files:**
- Modify: `src/profile.rs`
- Modify: `src/auth.rs`
- Test: `src/profile.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn lists_profiles_with_current_marker() {
    std::env::set_var("CODEX_HOME", temp_dir_with_profiles());
    let profiles = crate::profile::list_profiles_data().unwrap();
    assert!(profiles.iter().any(|p| p.is_current));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test profile::lists_profiles_with_current_marker -v`  
Expected: FAIL (function missing)

**Step 3: Write minimal implementation**

```rust
pub struct ProfileSummary {
    pub name: String,
    pub email: Option<String>,
    pub is_current: bool,
}

pub fn list_profiles_data() -> Result<Vec<ProfileSummary>> { /* no println */ }
pub fn switch_profile(profile_name: &str) -> Result<AuthDotJson> { /* return auth */ }
```

**Step 4: Run test to verify it passes**

Run: `cargo test profile::lists_profiles_with_current_marker -v`  
Expected: PASS

**Step 5: Commit**

```bash
git add src/profile.rs src/auth.rs
git commit -m "refactor: return profile data for UI"
```

### Task 3: Refactor quota API to return QuotaInfo only

**Files:**
- Modify: `src/api.rs`
- Test: `src/api.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn parses_quota_response() {
    let data = serde_json::json!({"data":{"usage":[{"n_requests":5,"n_tokens":10}]}}});
    let info = crate::api::parse_quota_response(&auth_stub(), &data).unwrap();
    assert_eq!(info.used_requests, Some(5));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test api::parses_quota_response -v`  
Expected: FAIL (parse not public or missing test helpers)

**Step 3: Write minimal implementation**

```rust
pub async fn fetch_quota(auth: &AuthDotJson) -> Result<QuotaInfo> { /* no println */ }
pub fn parse_quota_response(...) -> Result<QuotaInfo> { /* keep parser */ }
```

**Step 4: Run test to verify it passes**

Run: `cargo test api::parses_quota_response -v`  
Expected: PASS

**Step 5: Commit**

```bash
git add src/api.rs
git commit -m "refactor: return quota info for UI"
```

### Task 4: Add AppState + background worker (command/event channel)

**Files:**
- Create: `src/app_state.rs`
- Create: `src/worker.rs`
- Modify: `Cargo.toml`
- Test: `src/app_state.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn applies_profiles_loaded_event() {
    let mut state = AppState::default();
    state.apply_event(AppEvent::ProfilesLoaded(vec![sample_profile()]));
    assert_eq!(state.profiles.len(), 1);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test app_state::applies_profiles_loaded_event -v`  
Expected: FAIL

**Step 3: Write minimal implementation**

```rust
pub struct AppState { /* profiles, current, quota, loading, errors */ }
pub enum AppCommand { LoadProfiles, SwitchProfile(String), FetchQuota, SaveProfile(String), DeleteProfile(String) }
pub enum AppEvent { ProfilesLoaded(Vec<ProfileSummary>), QuotaLoaded(QuotaInfo), Error(String) }

impl AppState {
    pub fn apply_event(&mut self, event: AppEvent) { /* update fields */ }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test app_state::applies_profiles_loaded_event -v`  
Expected: PASS

**Step 5: Commit**

```bash
git add src/app_state.rs src/worker.rs Cargo.toml
git commit -m "feat: add app state and worker channel"
```

### Task 5: Build egui UI and replace main entrypoint

**Files:**
- Modify: `src/main.rs`
- Create: `src/app.rs`
- Modify: `Cargo.toml`

**Step 1: Write the failing test**

```rust
// No direct UI tests; add a compile-only test to ensure AppState wiring.
#[test]
fn app_state_compiles() { let _ = crate::app_state::AppState::default(); }
```

**Step 2: Run test to verify it fails**

Run: `cargo test app_state_compiles -v`  
Expected: FAIL (module missing)

**Step 3: Write minimal implementation**

```rust
fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native("Codex Router", options, Box::new(|_cc| Box::new(RouterApp::new())))
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test app_state_compiles -v`  
Expected: PASS

**Step 5: Commit**

```bash
git add src/main.rs src/app.rs Cargo.toml
git commit -m "feat: add egui app entrypoint"
```

### Task 6: Add menu bar tray with quick switching

**Files:**
- Create: `src/tray.rs`
- Modify: `src/app.rs`
- Modify: `Cargo.toml`

**Step 1: Write the failing test**

```rust
#[test]
fn tray_event_enum_exists() {
    let _ = crate::tray::TrayEvent::OpenWindow;
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test tray_event_enum_exists -v`  
Expected: FAIL

**Step 3: Write minimal implementation**

```rust
pub enum TrayEvent { OpenWindow, SwitchProfile(String), Refresh, Quit }
pub fn start_tray(sender: Sender<TrayEvent>) { /* spawn tray thread */ }
```

**Step 4: Run test to verify it passes**

Run: `cargo test tray_event_enum_exists -v`  
Expected: PASS

**Step 5: Commit**

```bash
git add src/tray.rs src/app.rs Cargo.toml
git commit -m "feat: add tray menu"
```

### Task 7: Remove CLI-only deps and update README

**Files:**
- Modify: `Cargo.toml`
- Modify: `README.md`

**Step 1: Write the failing test**

```rust
// No tests; verify `cargo build` succeeds after dependency cleanup.
```

**Step 2: Run build to verify it fails**

Run: `cargo build`  
Expected: FAIL (if old imports remain)

**Step 3: Write minimal implementation**

```toml
# Remove clap/colored, add eframe/egui/tray-icon
```

**Step 4: Run build to verify it passes**

Run: `cargo build`  
Expected: PASS

**Step 5: Commit**

```bash
git add Cargo.toml README.md
git commit -m "docs: update for desktop UI"
```

---

Plan complete and saved to `docs/plans/2026-01-11-codex-router-desktop-ui-implementation.md`. Two execution options:

1. Subagent-Driven (this session) - I dispatch fresh subagent per task, review between tasks, fast iteration
2. Parallel Session (separate) - Open new session with executing-plans, batch execution with checkpoints

Which approach?
