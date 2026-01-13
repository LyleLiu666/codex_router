# Profile Switch/Save/Login Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add in-window profile switching and saving, create profiles dir on startup, and surface `codex login` (URL/code + open URL) with account_id-aware token updates.

**Architecture:** Keep account logic in `src/profile.rs`/`src/auth.rs` by adding a token fingerprint helper and save outcome enum; update `src/app_state.rs`/`src/worker.rs`/`src/app.rs` for new commands/events and UI; add a small `src/login_output.rs` parser for extracting URL/code from `codex login` output.

**Tech Stack:** Rust, eframe/egui, std::process, serde_json, tokio runtime.

### Task 1: Ensure profiles dir exists on LoadProfiles (@superpowers:test-driven-development)

**Files:**
- Modify: `src/profile.rs`
- Test: `src/profile.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn list_profiles_creates_profiles_dir_when_missing() {
    let _lock = ENV_LOCK.lock().unwrap();
    let temp_dir = tempfile::tempdir().unwrap();
    let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

    let profiles_dir = temp_dir.path().join("profiles");
    assert!(!profiles_dir.exists());

    let profiles = list_profiles_data().unwrap();

    assert!(profiles.is_empty());
    assert!(profiles_dir.exists());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test list_profiles_creates_profiles_dir_when_missing -v`
Expected: FAIL because `profiles/` is not created.

**Step 3: Write minimal implementation**

```rust
if !profiles_dir.exists() {
    fs::create_dir_all(&profiles_dir)?;
    return Ok(Vec::new());
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test list_profiles_creates_profiles_dir_when_missing -v`
Expected: PASS

**Step 5: Commit**

```bash
git add src/profile.rs
git commit -m "feat: create profiles dir on load"
```

### Task 2: Add token fingerprint + save outcome enum (@superpowers:test-driven-development)

**Files:**
- Modify: `src/profile.rs`
- Test: `src/profile.rs`

**Step 1: Write the failing tests**

```rust
#[test]
fn token_fingerprint_prefers_api_key() {
    let auth = AuthDotJson {
        openai_api_key: Some("sk-test".to_string()),
        tokens: None,
        last_refresh: None,
    };

    assert_eq!(token_fingerprint(&auth), Some("sk-test".to_string()));
}

#[test]
fn token_fingerprint_uses_tokens_when_no_api_key() {
    let auth = AuthDotJson {
        openai_api_key: None,
        tokens: Some(TokenData {
            id_token: Some(IdToken::Raw("id.raw".to_string())),
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            account_id: Some("acct_123".to_string()),
        }),
        last_refresh: None,
    };

    assert_eq!(
        token_fingerprint(&auth),
        Some("access|refresh|id.raw".to_string())
    );
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test token_fingerprint_ -v`
Expected: FAIL because `token_fingerprint` does not exist.

**Step 3: Write minimal implementation**

```rust
fn token_fingerprint(auth: &AuthDotJson) -> Option<String> {
    if let Some(key) = &auth.openai_api_key {
        return Some(key.clone());
    }
    let tokens = auth.tokens.as_ref()?;
    let id_token = tokens.id_token.as_ref().and_then(|token| match token {
        IdToken::Raw(raw) => Some(raw.clone()),
        IdToken::Info(info) => info.raw_jwt.clone(),
    });
    Some(format!(
        "{}|{}|{}",
        tokens.access_token,
        tokens.refresh_token,
        id_token.unwrap_or_default()
    ))
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test token_fingerprint_ -v`
Expected: PASS

**Step 5: Commit**

```bash
git add src/profile.rs
git commit -m "feat: add token fingerprint helper"
```

### Task 3: SaveProfile account_id matching + outcomes (@superpowers:test-driven-development)

**Files:**
- Modify: `src/profile.rs`
- Test: `src/profile.rs`

**Step 1: Write the failing tests**

```rust
#[test]
fn save_profile_noops_when_account_id_matches_and_token_same() {
    let _lock = ENV_LOCK.lock().unwrap();
    let temp_dir = tempfile::tempdir().unwrap();
    let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

    let auth = AuthDotJson {
        openai_api_key: None,
        tokens: Some(TokenData {
            id_token: Some(IdToken::Raw("id.raw".to_string())),
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            account_id: Some("acct_123".to_string()),
        }),
        last_refresh: None,
    };

    fs::write(
        temp_dir.path().join("auth.json"),
        serde_json::to_string_pretty(&auth).unwrap(),
    )
    .unwrap();

    let profiles_dir = temp_dir.path().join("profiles");
    fs::create_dir_all(profiles_dir.join("work")).unwrap();
    fs::write(
        profiles_dir.join("work").join("auth.json"),
        serde_json::to_string_pretty(&auth).unwrap(),
    )
    .unwrap();

    let outcome = save_profile("new").unwrap();

    assert_eq!(outcome, SaveProfileOutcome::AlreadyExists {
        name: "work".to_string()
    });
}

#[test]
fn save_profile_overwrites_when_account_id_matches_and_token_changes() {
    let _lock = ENV_LOCK.lock().unwrap();
    let temp_dir = tempfile::tempdir().unwrap();
    let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

    let old_auth = AuthDotJson {
        openai_api_key: None,
        tokens: Some(TokenData {
            id_token: Some(IdToken::Raw("id.old".to_string())),
            access_token: "access-old".to_string(),
            refresh_token: "refresh-old".to_string(),
            account_id: Some("acct_123".to_string()),
        }),
        last_refresh: None,
    };

    let new_auth = AuthDotJson {
        openai_api_key: None,
        tokens: Some(TokenData {
            id_token: Some(IdToken::Raw("id.new".to_string())),
            access_token: "access-new".to_string(),
            refresh_token: "refresh-new".to_string(),
            account_id: Some("acct_123".to_string()),
        }),
        last_refresh: None,
    };

    fs::write(
        temp_dir.path().join("auth.json"),
        serde_json::to_string_pretty(&new_auth).unwrap(),
    )
    .unwrap();

    let profiles_dir = temp_dir.path().join("profiles");
    fs::create_dir_all(profiles_dir.join("work")).unwrap();
    fs::write(
        profiles_dir.join("work").join("auth.json"),
        serde_json::to_string_pretty(&old_auth).unwrap(),
    )
    .unwrap();

    let outcome = save_profile("ignored").unwrap();

    assert_eq!(outcome, SaveProfileOutcome::Updated {
        name: "work".to_string()
    });

    let updated = fs::read_to_string(profiles_dir.join("work").join("auth.json")).unwrap();
    let updated_value: serde_json::Value = serde_json::from_str(&updated).unwrap();
    let expected_value: serde_json::Value = serde_json::from_str(&serde_json::to_string_pretty(&new_auth).unwrap()).unwrap();
    assert_eq!(updated_value, expected_value);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test save_profile_ -v`
Expected: FAIL because outcomes and logic are missing.

**Step 3: Write minimal implementation**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveProfileOutcome {
    Created { name: String },
    Updated { name: String },
    AlreadyExists { name: String },
}

pub fn save_profile(profile_name: &str) -> Result<SaveProfileOutcome> {
    let auth = auth::load_auth()?;
    let profiles_dir = get_profiles_dir()?;
    fs::create_dir_all(&profiles_dir)?;

    if let Some(account_id) = auth::get_account_id(&auth) {
        for entry in fs::read_dir(&profiles_dir)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }
            let existing_name = entry.file_name().to_string_lossy().to_string();
            let existing_auth_file = entry.path().join("auth.json");
            let existing_auth = fs::read_to_string(&existing_auth_file)
                .ok()
                .and_then(|contents| serde_json::from_str::<AuthDotJson>(&contents).ok());
            let Some(existing_auth) = existing_auth else { continue; };
            if auth::get_account_id(&existing_auth).as_deref() != Some(account_id.as_str()) {
                continue;
            }
            let incoming_fp = token_fingerprint(&auth);
            let existing_fp = token_fingerprint(&existing_auth);
            if incoming_fp == existing_fp {
                save_current_profile(&existing_name)?;
                return Ok(SaveProfileOutcome::AlreadyExists { name: existing_name });
            }
            fs::write(&existing_auth_file, serde_json::to_string_pretty(&auth)?)?;
            save_current_profile(&existing_name)?;
            return Ok(SaveProfileOutcome::Updated { name: existing_name });
        }
    }

    let profile_dir = profiles_dir.join(profile_name);
    if profile_dir.exists() {
        anyhow::bail!("Profile '{}' already exists. Delete it first.", profile_name);
    }
    fs::create_dir(&profile_dir)?;
    let profile_auth_file = profile_dir.join("auth.json");
    fs::write(&profile_auth_file, serde_json::to_string_pretty(&auth)?)?;
    if get_current_profile()?.is_none() {
        save_current_profile(profile_name)?;
    }

    Ok(SaveProfileOutcome::Created {
        name: profile_name.to_string(),
    })
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test save_profile_ -v`
Expected: PASS

**Step 5: Commit**

```bash
git add src/profile.rs
git commit -m "feat: handle account id conflicts when saving profiles"
```

### Task 4: Wire save outcomes into AppState/worker (@superpowers:test-driven-development)

**Files:**
- Modify: `src/app_state.rs`
- Modify: `src/worker.rs`
- Test: `src/app_state.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn applies_profile_saved_event() {
    let mut state = AppState::default();
    state.apply_event(AppEvent::ProfileSaved(SaveProfileOutcome::Created {
        name: "work".to_string(),
    }));
    assert_eq!(state.profile_message.as_deref(), Some("Saved profile: work"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test applies_profile_saved_event -v`
Expected: FAIL because event/field not present.

**Step 3: Write minimal implementation**

```rust
pub enum AppEvent {
    // ...
    ProfileSaved(SaveProfileOutcome),
}

pub struct AppState {
    // ...
    pub profile_message: Option<String>,
}

impl AppState {
    pub fn apply_event(&mut self, event: AppEvent) {
        match event {
            // ...
            AppEvent::ProfileSaved(outcome) => {
                self.profile_message = Some(match outcome {
                    SaveProfileOutcome::Created { name } => format!("Saved profile: {name}"),
                    SaveProfileOutcome::Updated { name } => format!("Updated profile: {name}"),
                    SaveProfileOutcome::AlreadyExists { name } => {
                        format!("Profile already saved: {name}")
                    }
                });
            }
            _ => {}
        }
    }
}
```

Update worker:

```rust
AppCommand::SaveProfile(name) => match profile::save_profile(&name) {
    Ok(outcome) => {
        let _ = evt_tx.send(AppEvent::ProfileSaved(outcome));
        if let Ok(profiles) = profile::list_profiles_data() {
            let _ = evt_tx.send(AppEvent::ProfilesLoaded(profiles));
        }
    }
    Err(err) => {
        let _ = evt_tx.send(AppEvent::Error(err.to_string()));
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test applies_profile_saved_event -v`
Expected: PASS

**Step 5: Commit**

```bash
git add src/app_state.rs src/worker.rs
git commit -m "feat: surface profile save outcomes"
```

### Task 5: Add login output parser + tests (@superpowers:test-driven-development)

**Files:**
- Create: `src/login_output.rs`
- Modify: `src/main.rs`
- Test: `src/login_output.rs`

**Step 1: Write the failing tests**

```rust
#[test]
fn parses_device_code_output() {
    let output = "Open this link\nhttps://auth.openai.com/codex/device\n\nEnter this one-time code\nABCD-EFGH\n";
    let parsed = parse_login_output(output);
    assert_eq!(parsed.url.as_deref(), Some("https://auth.openai.com/codex/device"));
    assert_eq!(parsed.code.as_deref(), Some("ABCD-EFGH"));
}

#[test]
fn parses_local_server_output() {
    let output = "Starting local login server on http://localhost:1455.\nIf your browser did not open, navigate to this URL to authenticate:\n\nhttp://localhost:1455/auth/authorize?foo=bar\n";
    let parsed = parse_login_output(output);
    assert_eq!(parsed.url.as_deref(), Some("http://localhost:1455/auth/authorize?foo=bar"));
    assert_eq!(parsed.code, None);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test parses_device_code_output -v`
Expected: FAIL because parser does not exist.

**Step 3: Write minimal implementation**

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoginOutput {
    pub url: Option<String>,
    pub code: Option<String>,
}

pub fn parse_login_output(raw: &str) -> LoginOutput {
    let mut output = LoginOutput::default();
    for line in raw.lines() {
        let clean = strip_ansi(line).trim().to_string();
        if output.url.is_none() {
            if let Some(url) = extract_url(&clean) {
                output.url = Some(url);
            }
        }
        if output.code.is_none() && looks_like_code(&clean) {
            output.code = Some(clean);
        }
    }
    output
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test parses_device_code_output -v`
Expected: PASS

**Step 5: Commit**

```bash
git add src/login_output.rs src/main.rs
git commit -m "feat: parse codex login output"
```

### Task 6: Add login commands, state, and UI wiring (@superpowers:test-driven-development)

**Files:**
- Modify: `src/app_state.rs`
- Modify: `src/worker.rs`
- Modify: `src/app.rs`

**Step 1: Write the failing AppState test**

```rust
#[test]
fn applies_login_output_event() {
    let mut state = AppState::default();
    state.apply_event(AppEvent::LoginOutput {
        output: "hello".to_string(),
        parsed: LoginOutput {
            url: Some("http://localhost".to_string()),
            code: None,
        },
        running: true,
    });
    assert!(state.login_running);
    assert!(state.login_output.contains("hello"));
    assert_eq!(state.login_url.as_deref(), Some("http://localhost"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test applies_login_output_event -v`
Expected: FAIL because event/fields not present.

**Step 3: Write minimal implementation**

- Add `AppCommand::RunLogin` and `AppCommand::OpenLoginUrl(String)`.
- Add `AppEvent::LoginOutput { output, parsed, running }` and `AppEvent::LoginFinished { success, message }`.
- Add fields to `AppState`: `login_output`, `login_url`, `login_code`, `login_running`.
- Update `apply_event` to append output, update parsed url/code, set running status.

Worker implementation sketch:

```rust
AppCommand::RunLogin => {
    let evt_tx = evt_tx.clone();
    std::thread::spawn(move || {
        let mut child = Command::new("codex")
            .arg("login")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        // Read stdout/stderr, send LoginOutput events with parsed url/code.
        // On exit, send LoginFinished, then ProfilesLoaded + FetchQuota.
    });
}

AppCommand::OpenLoginUrl(url) => {
    let _ = Command::new("open").arg(url).status();
}
```

UI wiring sketch:

```rust
if ui.button("Run codex login").clicked() {
    let _ = self.cmd_tx.send(AppCommand::RunLogin);
}
if let Some(url) = &self.state.login_url {
    if ui.button("Open URL").clicked() {
        let _ = self.cmd_tx.send(AppCommand::OpenLoginUrl(url.clone()));
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test applies_login_output_event -v`
Expected: PASS

**Step 5: Commit**

```bash
git add src/app_state.rs src/worker.rs src/app.rs
git commit -m "feat: add login flow UI and worker"
```

### Task 7: Add profile list UI (save input + switch buttons) (@superpowers:test-driven-development)

**Files:**
- Modify: `src/app.rs`
- Modify: `src/app_state.rs`

**Step 1: Write the failing AppState test**

```rust
#[test]
fn applies_save_input_change() {
    let mut state = AppState::default();
    state.profile_name_input = "work".to_string();
    assert_eq!(state.profile_name_input, "work");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test applies_save_input_change -v`
Expected: FAIL because field missing.

**Step 3: Write minimal implementation**

- Add `profile_name_input: String` to `AppState::default()`.
- In UI: render input + Save button. On click, send `SaveProfile(profile_name_input.trim())`.
- Each row shows Switch button; current row shows disabled `Current`.
- Show `profile_message` inline under the header.

**Step 4: Run test to verify it passes**

Run: `cargo test applies_save_input_change -v`
Expected: PASS

**Step 5: Commit**

```bash
git add src/app_state.rs src/app.rs
git commit -m "feat: add profile save/switch UI"
```

### Task 8: Full test run

**Step 1: Run full tests**

Run: `cargo test`
Expected: PASS

**Step 2: Commit (if needed)**

```bash
git status --short
```

