# Desktop UI Refresh/Tray Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add UI-configured quota refresh (default 10 minutes), persist settings, and close-to-tray behavior without blocking the UI.

**Architecture:** Keep auto-refresh on the UI thread via a small scheduler (RefreshSchedule). Use small helper functions in `src/app.rs` to keep logic testable, and persist settings via `state::save_state` when UI values change.

**Tech Stack:** Rust, eframe/egui, tray-icon, chrono, serde_json, tokio.

### Task 1: RefreshSchedule tick behavior

**Files:**
- Create: `src/refresh.rs`
- Modify: `src/main.rs`
- Test: `src/refresh.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn schedules_next_when_missing() {
    let mut schedule = RefreshSchedule::new();
    let now = Instant::now();
    let interval = Duration::from_secs(60);

    let triggered = schedule.tick(now, interval);

    assert!(!triggered);
    assert_eq!(schedule.next_due(), Some(now + interval));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test refresh::tests::schedules_next_when_missing -q`
Expected: FAIL with `refresh scheduling not implemented`.

**Step 3: Write minimal implementation**

```rust
pub fn tick(&mut self, now: Instant, interval: Duration) -> bool {
    match self.next_due {
        None => {
            self.next_due = Some(now + interval);
            false
        }
        Some(due) if now >= due => {
            self.next_due = Some(now + interval);
            true
        }
        Some(_) => false,
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test refresh::tests -q`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/refresh.rs src/main.rs
git commit -m "feat: add refresh schedule"
```

### Task 2: App helper logic (TDD)

**Files:**
- Modify: `src/app.rs`
- Test: `src/app.rs`

**Step 1: Write failing tests**

```rust
#[test]
fn close_action_hides_unless_allowed() {
    assert!(matches!(close_action(false), CloseAction::Hide));
    assert!(matches!(close_action(true), CloseAction::Close));
}

#[test]
fn profile_change_triggers_refresh() {
    assert!(should_fetch_on_profile_change(None, Some("a")));
    assert!(should_fetch_on_profile_change(Some("a"), Some("b")));
    assert!(!should_fetch_on_profile_change(Some("a"), Some("a")));
    assert!(!should_fetch_on_profile_change(None, None));
}

#[test]
fn applies_router_state_to_app_state() {
    let mut app_state = AppState::default();
    let router_state = RouterState {
        refresh_interval_seconds: 300,
        auto_refresh_enabled: false,
        last_selected_profile: Some("work".to_string()),
    };

    apply_router_state(&mut app_state, &router_state);

    assert_eq!(app_state.refresh_interval_seconds, 300);
    assert!(!app_state.auto_refresh_enabled);
}

#[test]
fn update_router_state_settings_returns_change() {
    let mut app_state = AppState::default();
    let mut router_state = RouterState::default();

    let changed = update_router_state_settings(&mut router_state, &mut app_state, 900, false);

    assert!(changed);
    assert_eq!(router_state.refresh_interval_seconds, 900);
    assert!(!router_state.auto_refresh_enabled);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test app::tests::close_action_hides_unless_allowed -q`
Expected: FAIL with missing items.

**Step 3: Write minimal implementation**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseAction {
    Hide,
    Close,
}

fn close_action(allow_close: bool) -> CloseAction {
    if allow_close {
        CloseAction::Close
    } else {
        CloseAction::Hide
    }
}

fn should_fetch_on_profile_change(prev: Option<&str>, next: Option<&str>) -> bool {
    match (prev, next) {
        (None, None) => false,
        (Some(prev), Some(next)) => prev != next,
        (None, Some(_)) => true,
        (Some(_), None) => false,
    }
}

fn apply_router_state(app_state: &mut AppState, router_state: &RouterState) {
    app_state.refresh_interval_seconds = router_state.refresh_interval_seconds;
    app_state.auto_refresh_enabled = router_state.auto_refresh_enabled;
}

fn update_router_state_settings(
    router_state: &mut RouterState,
    app_state: &mut AppState,
    interval_seconds: u64,
    auto_refresh_enabled: bool,
) -> bool {
    let mut changed = false;
    if router_state.refresh_interval_seconds != interval_seconds {
        router_state.refresh_interval_seconds = interval_seconds;
        app_state.refresh_interval_seconds = interval_seconds;
        changed = true;
    }
    if router_state.auto_refresh_enabled != auto_refresh_enabled {
        router_state.auto_refresh_enabled = auto_refresh_enabled;
        app_state.auto_refresh_enabled = auto_refresh_enabled;
        changed = true;
    }
    changed
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test app::tests::close_action_hides_unless_allowed -q`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/app.rs
git commit -m "test: add app helper logic"
```

### Task 3: Wire UI refresh settings, auto-refresh, and close-to-tray

**Files:**
- Modify: `src/app.rs`
- Modify: `src/main.rs`
- Modify: `src/state.rs` (if needed)

**Step 1: Write failing tests for auto-refresh gating**

```rust
#[test]
fn auto_refresh_disabled_never_triggers() {
    let mut schedule = RefreshSchedule::new();
    let now = Instant::now();
    let interval = Duration::from_secs(60);

    let triggered = auto_refresh_tick(false, &mut schedule, now, interval);

    assert!(!triggered);
    assert!(schedule.next_due().is_none());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test app::tests::auto_refresh_disabled_never_triggers -q`
Expected: FAIL with missing items.

**Step 3: Write minimal implementation**

```rust
fn auto_refresh_tick(
    enabled: bool,
    schedule: &mut RefreshSchedule,
    now: Instant,
    interval: Duration,
) -> bool {
    if !enabled {
        return false;
    }
    schedule.tick(now, interval)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test app::tests::auto_refresh_disabled_never_triggers -q`
Expected: PASS.

**Step 5: Implement wiring in RouterApp::update**

- Load router state in `RouterApp::new` and apply to `AppState`.
- Store `router_state` and `quota_refresh` in `RouterApp`.
- On `ProfilesLoaded`, if current profile changes, persist `last_selected_profile` and send `FetchQuota`.
- When close is requested and `allow_close` is false, cancel close and hide window.
- On tray `Quit`, set `allow_close = true` then close window.
- Add UI controls for refresh settings and trigger `save_state` on change.
- If auto-refresh is enabled, call `auto_refresh_tick` and send `FetchQuota` when due.
- Use `ctx.request_repaint_after` with the refresh interval while enabled.

**Step 6: Manual verification**

Run: `cargo test -q`
Expected: PASS.

**Step 7: Commit**

```bash
git add src/app.rs src/main.rs src/state.rs
git commit -m "feat: add ui refresh settings and close-to-tray"
```

### Task 4: Update docs and PR

**Files:**
- Modify: `README.md` (optional, if UI behavior needs clarification)

**Step 1: Update docs**

Add a short note about close-to-tray and refresh settings in the main window.

**Step 2: Commit**

```bash
git add README.md
git commit -m "docs: update desktop ui behavior"
```

**Step 3: Push and create PR**

```bash
git push -u origin desktop-ui
```

Create a PR targeting `main` and provide a review report.
