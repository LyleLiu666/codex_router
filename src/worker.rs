use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::{fs, path::Path};

/// Shared holder for the login child process, allowing cancellation from the main thread.
type LoginProcessHolder = Arc<Mutex<Option<u32>>>;

use crate::app_state::{AppCommand, AppEvent};
use crate::config;
use crate::login_output;
use crate::{api, auth, profile};

/// Find the codex binary using multiple detection strategies.
/// Priority order:
/// 1. Manual PATH search (more reliable than `which` in GUI apps)
/// 2. Node version managers: nvm, volta, fnm, asdf
/// 3. npm global prefix detection
/// 4. Common system locations
fn find_codex_binary() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let home_path = Path::new(&home);

    // Strategy 1: Search PATH manually (works better in GUI apps than `which`)
    if let Some(path) = search_in_path("codex") {
        tracing::debug!("Found codex in PATH: {:?}", path);
        return Some(path);
    }

    // Strategy 2: Check node version managers

    // nvm - check current version first, then all versions
    if let Some(path) = find_in_nvm(home_path) {
        tracing::debug!("Found codex via nvm: {:?}", path);
        return Some(path);
    }

    // volta
    let volta_bin = home_path.join(".volta/bin/codex");
    if volta_bin.exists() {
        tracing::debug!("Found codex via volta: {:?}", volta_bin);
        return Some(volta_bin);
    }

    // fnm (Fast Node Manager)
    if let Some(path) = find_in_fnm(home_path) {
        tracing::debug!("Found codex via fnm: {:?}", path);
        return Some(path);
    }

    // asdf
    if let Some(path) = find_in_asdf(home_path) {
        tracing::debug!("Found codex via asdf: {:?}", path);
        return Some(path);
    }

    // Strategy 3: Query npm for global prefix
    if let Some(path) = find_via_npm_prefix() {
        tracing::debug!("Found codex via npm prefix: {:?}", path);
        return Some(path);
    }

    // Strategy 4: Check common system locations
    let system_locations = [
        PathBuf::from("/opt/homebrew/bin/codex"), // Homebrew Apple Silicon
        PathBuf::from("/usr/local/bin/codex"),    // Homebrew Intel / manual install
        home_path.join(".local/bin/codex"),       // XDG local bin
        home_path.join(".npm-global/bin/codex"),  // Common npm global config
        home_path.join("bin/codex"),              // User bin
    ];

    for location in &system_locations {
        if location.exists() {
            tracing::debug!("Found codex at system location: {:?}", location);
            return Some(location.clone());
        }
    }

    tracing::warn!("codex binary not found in any known location");
    None
}

/// Search for a binary in PATH environment variable
fn search_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(':') {
        let candidate = Path::new(dir).join(binary_name);
        if candidate.exists() && candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Find codex in nvm installations
fn find_in_nvm(home: &Path) -> Option<PathBuf> {
    let nvm_dir = home.join(".nvm/versions/node");
    if !nvm_dir.exists() {
        return None;
    }

    // Try to get current nvm version from NVM_BIN or .nvmrc
    if let Ok(nvm_bin) = std::env::var("NVM_BIN") {
        let codex_path = Path::new(&nvm_bin).join("codex");
        if codex_path.exists() {
            return Some(codex_path);
        }
    }

    // Fallback: search all installed node versions, prefer newer versions
    let mut versions: Vec<_> = fs::read_dir(&nvm_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();

    // Sort by version number descending (newer first)
    versions.sort_by(|a, b| {
        let va = a.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let vb = b.file_name().and_then(|s| s.to_str()).unwrap_or("");
        version_cmp(vb, va)
    });

    for version_dir in versions {
        let codex_path = version_dir.join("bin/codex");
        if codex_path.exists() {
            return Some(codex_path);
        }
    }
    None
}

/// Find codex in fnm installations
fn find_in_fnm(home: &Path) -> Option<PathBuf> {
    // fnm stores versions in different locations based on config
    let fnm_dirs = [
        home.join(".fnm/node-versions"),
        home.join("Library/Application Support/fnm/node-versions"),
    ];

    for fnm_dir in &fnm_dirs {
        if !fnm_dir.exists() {
            continue;
        }

        let mut versions: Vec<_> = fs::read_dir(fnm_dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect();

        versions.sort_by(|a, b| {
            let va = a.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let vb = b.file_name().and_then(|s| s.to_str()).unwrap_or("");
            version_cmp(vb, va)
        });

        for version_dir in versions {
            let codex_path = version_dir.join("installation/bin/codex");
            if codex_path.exists() {
                return Some(codex_path);
            }
        }
    }
    None
}

/// Find codex in asdf installations
fn find_in_asdf(home: &Path) -> Option<PathBuf> {
    let asdf_dir = home.join(".asdf/installs/nodejs");
    if !asdf_dir.exists() {
        return None;
    }

    let mut versions: Vec<_> = fs::read_dir(&asdf_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();

    versions.sort_by(|a, b| {
        let va = a.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let vb = b.file_name().and_then(|s| s.to_str()).unwrap_or("");
        version_cmp(vb, va)
    });

    for version_dir in versions {
        let codex_path = version_dir.join("bin/codex");
        if codex_path.exists() {
            return Some(codex_path);
        }
    }
    None
}

/// Find codex by querying npm for its global prefix
fn find_via_npm_prefix() -> Option<PathBuf> {
    // Try to find npm first
    let npm_path = search_in_path("npm")?;

    let output = Command::new(&npm_path)
        .args(["config", "get", "prefix"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if prefix.is_empty() {
        return None;
    }

    let codex_path = Path::new(&prefix).join("bin/codex");
    if codex_path.exists() {
        return Some(codex_path);
    }
    None
}

/// Compare version strings (e.g., "v20.10.0" vs "v18.19.1")
fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parse_version = |s: &str| -> Vec<u32> {
        s.trim_start_matches('v')
            .split('.')
            .filter_map(|part| part.parse().ok())
            .collect()
    };

    let va = parse_version(a);
    let vb = parse_version(b);
    va.cmp(&vb)
}

fn load_profiles_with_quota(
    runtime: &tokio::runtime::Runtime,
) -> anyhow::Result<Vec<profile::ProfileSummary>> {
    let mut profiles = profile::list_profiles_data()?;
    for profile_summary in &mut profiles {
        let mut auth = match profile::load_profile_auth(&profile_summary.name) {
            Ok(auth) => auth,
            Err(_) => continue,
        };
        match runtime.block_on(api::fetch_quota(&auth)) {
            Ok(quota) => {
                profile_summary.quota = Some(quota);
            }
            Err(err) => {
                if let Some(api::AuthError::Expired) = err.downcast_ref::<api::AuthError>() {
                    // Attempt to refresh the token
                    if let Some(ref tokens) = auth.tokens {
                        tracing::info!(
                            profile = %profile_summary.name,
                            "Access token expired, attempting refresh"
                        );
                        match runtime.block_on(api::refresh_token(&tokens.refresh_token)) {
                            Ok(refresh_response) => {
                                // Update auth with new tokens
                                if let Some(ref mut tokens) = auth.tokens {
                                    if let Some(new_access) = refresh_response.access_token {
                                        tokens.access_token = new_access;
                                    }
                                    if let Some(new_refresh) = refresh_response.refresh_token {
                                        tokens.refresh_token = new_refresh;
                                    }
                                    // Note: id_token update requires parsing, skip for now
                                }
                                auth.last_refresh = Some(chrono::Utc::now());

                                // Save updated auth to profile
                                if let Err(save_err) =
                                    profile::save_profile_auth(&profile_summary.name, &auth)
                                {
                                    tracing::warn!(
                                        profile = %profile_summary.name,
                                        error = %save_err,
                                        "Failed to save refreshed tokens"
                                    );
                                }

                                // Retry quota fetch with new tokens
                                match runtime.block_on(api::fetch_quota(&auth)) {
                                    Ok(quota) => {
                                        profile_summary.quota = Some(quota);
                                        tracing::info!(
                                            profile = %profile_summary.name,
                                            "Token refresh successful"
                                        );
                                    }
                                    Err(retry_err) => {
                                        tracing::warn!(
                                            profile = %profile_summary.name,
                                            error = %retry_err,
                                            "Quota fetch failed after token refresh"
                                        );
                                        profile_summary.is_valid = false;
                                    }
                                }
                            }
                            Err(refresh_err) => {
                                tracing::warn!(
                                    profile = %profile_summary.name,
                                    error = %refresh_err,
                                    "Token refresh failed"
                                );
                                profile_summary.is_valid = false;
                            }
                        }
                    } else {
                        profile_summary.is_valid = false;
                    }
                }
            }
        }
    }
    Ok(profiles)
}

fn read_existing_auth_json(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn finalize_login(previous_auth_json: Option<String>) -> anyhow::Result<()> {
    let new_auth = auth::load_auth()?;
    let outcome = profile::save_auth_as_profile_without_switch(&new_auth)?;

    let name = match &outcome {
        profile::SaveProfileOutcome::Created { name }
        | profile::SaveProfileOutcome::Updated { name }
        | profile::SaveProfileOutcome::AlreadyExists { name } => name.clone(),
    };

    let auth_file = config::get_auth_file()?;
    if let Some(parent) = auth_file.parent() {
        fs::create_dir_all(parent)?;
    }

    match previous_auth_json {
        Some(previous) => {
            fs::write(auth_file, previous)?;
        }
        None => {
            let current_profile_file = config::get_current_profile_file()?;
            let needs_current_profile = match fs::read_to_string(&current_profile_file) {
                Ok(contents) => contents.trim().is_empty(),
                Err(_) => true,
            };
            if needs_current_profile {
                if let Some(parent) = current_profile_file.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&current_profile_file, &name)?;
            }
            fs::write(auth_file, serde_json::to_string_pretty(&new_auth)?)?;
        }
    }

    Ok(())
}

pub fn start_worker(cmd_rx: Receiver<AppCommand>, evt_tx: Sender<AppEvent>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("failed to create runtime");
        let login_process: LoginProcessHolder = Arc::new(Mutex::new(None));

        while let Ok(command) = cmd_rx.recv() {
            match command {
                AppCommand::LoadProfiles => match profile::list_profiles_data() {
                    Ok(profiles) => {
                        let _ = evt_tx.send(AppEvent::ProfilesLoaded(profiles));
                    }
                    Err(err) => {
                        let _ = evt_tx.send(AppEvent::Error(err.to_string()));
                    }
                },
                AppCommand::SwitchProfile(name) => {
                    let result = runtime.block_on(profile::switch_profile(&name));
                    match result {
                        Ok(_) => {
                            if let Ok(profiles) = profile::list_profiles_data() {
                                let _ = evt_tx.send(AppEvent::ProfilesLoaded(profiles));
                            }
                        }
                        Err(err) => {
                            let _ = evt_tx.send(AppEvent::Error(err.to_string()));
                        }
                    }
                }
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
                },
                AppCommand::DeleteProfile(name) => match profile::delete_profile(&name) {
                    Ok(()) => {
                        if let Ok(profiles) = profile::list_profiles_data() {
                            let _ = evt_tx.send(AppEvent::ProfilesLoaded(profiles));
                        }
                    }
                    Err(err) => {
                        let _ = evt_tx.send(AppEvent::Error(err.to_string()));
                    }
                },
                AppCommand::FetchQuota => {
                    match load_profiles_with_quota(&runtime) {
                        Ok(profiles) => {
                            let _ = evt_tx.send(AppEvent::ProfilesLoaded(profiles));
                        }
                        Err(err) => {
                            let _ = evt_tx.send(AppEvent::Error(err.to_string()));
                        }
                    };
                }
                AppCommand::FetchProfileQuota(name) => match profile::load_profile_auth(&name) {
                    Ok(auth) => match runtime.block_on(api::fetch_quota(&auth)) {
                        Ok(quota) => {
                            let _ = evt_tx.send(AppEvent::ProfileQuotaLoaded { name, quota });
                        }
                        Err(err) => {
                            let _ = evt_tx.send(AppEvent::Error(format!(
                                "Failed to fetch quota for {}: {}",
                                name, err
                            )));
                        }
                    },
                    Err(err) => {
                        let _ = evt_tx.send(AppEvent::Error(format!(
                            "Failed to load profile {}: {}",
                            name, err
                        )));
                    }
                },
                AppCommand::RunLogin => {
                    let previous_auth_json = config::get_auth_file()
                        .ok()
                        .and_then(|path| read_existing_auth_json(&path));

                    let evt_tx = evt_tx.clone();
                    let login_process = Arc::clone(&login_process);
                    std::thread::spawn(move || {
                        let _ = evt_tx.send(AppEvent::LoginOutput {
                            output: String::new(),
                            parsed: login_output::LoginOutput::default(),
                            running: true,
                        });

                        let codex_path = match find_codex_binary() {
                            Some(path) => path,
                            None => {
                                let _ = evt_tx.send(AppEvent::LoginFinished {
                                    success: false,
                                    message: "codex binary not found. Please install it via: npm install -g @openai/codex".to_string(),
                                });
                                return;
                            }
                        };

                        let mut command = Command::new(&codex_path);
                        command
                            .arg("login")
                            .stdout(Stdio::piped())
                            .stderr(Stdio::piped());

                        // Add the directory containing the codex binary to PATH
                        // This ensures that `#!/usr/bin/env node` finds the node executable (which is often in the same dir)
                        if let Some(parent) = codex_path.parent() {
                            if let Ok(path) = std::env::var("PATH") {
                                let new_path = format!("{}:{}", parent.display(), path);
                                command.env("PATH", new_path);
                            }
                        }

                        let mut child = match command.spawn() {
                            Ok(child) => child,
                            Err(err) => {
                                let _ = evt_tx.send(AppEvent::LoginFinished {
                                    success: false,
                                    message: format!("Failed to start codex login: {err}"),
                                });
                                return;
                            }
                        };

                        // Store the child process ID for cancellation
                        {
                            let mut holder = login_process.lock().unwrap();
                            *holder = Some(child.id());
                        }

                        let stdout = child.stdout.take();
                        let stderr = child.stderr.take();

                        let spawn_reader = |stream: Option<std::process::ChildStdout>| {
                            let Some(stream) = stream else { return None };
                            let evt_tx = evt_tx.clone();
                            Some(std::thread::spawn(move || {
                                let reader = BufReader::new(stream);
                                for line in reader.lines().map_while(Result::ok) {
                                    let mut chunk = line;
                                    chunk.push('\n');
                                    let parsed = login_output::parse_login_output(&chunk);
                                    let _ = evt_tx.send(AppEvent::LoginOutput {
                                        output: chunk,
                                        parsed,
                                        running: true,
                                    });
                                }
                            }))
                        };

                        let stdout_handle = spawn_reader(stdout);

                        let stderr_handle = stderr.map(|stream| {
                            let evt_tx = evt_tx.clone();
                            std::thread::spawn(move || {
                                let reader = BufReader::new(stream);
                                for line in reader.lines().map_while(Result::ok) {
                                    let mut chunk = line;
                                    chunk.push('\n');
                                    let parsed = login_output::parse_login_output(&chunk);
                                    let _ = evt_tx.send(AppEvent::LoginOutput {
                                        output: chunk,
                                        parsed,
                                        running: true,
                                    });
                                }
                            })
                        });

                        let status = child.wait();

                        if let Some(handle) = stdout_handle {
                            let _ = handle.join();
                        }
                        if let Some(handle) = stderr_handle {
                            let _ = handle.join();
                        }

                        let (success, message) = match status {
                            Ok(status) if status.success() => (true, "codex login finished".into()),
                            Ok(status) => (false, format!("codex login failed: {status}")),
                            Err(err) => (false, format!("codex login failed: {err}")),
                        };

                        // Clear the process holder
                        {
                            let mut holder = login_process.lock().unwrap();
                            *holder = None;
                        }

                        let _ = evt_tx.send(AppEvent::LoginFinished {
                            success,
                            message: message.clone(),
                        });

                        if success {
                            if let Err(err) = finalize_login(previous_auth_json) {
                                let _ = evt_tx.send(AppEvent::Error(err.to_string()));
                                return;
                            }

                            let runtime =
                                tokio::runtime::Runtime::new().expect("failed to create runtime");
                            match load_profiles_with_quota(&runtime) {
                                Ok(profiles) => {
                                    let current_quota = profiles
                                        .iter()
                                        .find(|profile| profile.is_current)
                                        .and_then(|profile| profile.quota.clone());
                                    let _ = evt_tx.send(AppEvent::ProfilesLoaded(profiles));
                                    if let Some(quota) = current_quota {
                                        let _ = evt_tx.send(AppEvent::QuotaLoaded(quota));
                                    }
                                }
                                Err(err) => {
                                    let _ = evt_tx.send(AppEvent::Error(err.to_string()));
                                }
                            }
                        }
                    });
                }
                AppCommand::CancelLogin => {
                    let pid = {
                        let mut holder = login_process.lock().unwrap();
                        holder.take()
                    };
                    if let Some(pid) = pid {
                        // Send SIGTERM to gracefully terminate the login process
                        #[cfg(unix)]
                        {
                            let _ = Command::new("kill")
                                .args(["-TERM", &pid.to_string()])
                                .status();
                        }
                        #[cfg(not(unix))]
                        {
                            let _ = Command::new("taskkill")
                                .args(["/PID", &pid.to_string(), "/F"])
                                .status();
                        }
                        let _ = evt_tx.send(AppEvent::LoginFinished {
                            success: false,
                            message: "Login cancelled".to_string(),
                        });
                    }
                }
                AppCommand::OpenLoginUrl(url) => {
                    let _ = Command::new("open").arg(url).status();
                }
                AppCommand::Shutdown => break,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{EnvGuard, ENV_LOCK};
    use std::env;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    struct StringEnvGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl StringEnvGuard {
        fn set(key: &'static str, value: String) -> Self {
            let original = env::var(key).ok();
            env::set_var(key, value);
            Self { key, original }
        }
    }

    impl Drop for StringEnvGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.original {
                env::set_var(self.key, value);
            } else {
                env::remove_var(self.key);
            }
        }
    }

    #[test]
    fn worker_emits_profiles_loaded() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

        let profiles_dir = temp_dir.path().join("profiles");
        fs::create_dir_all(profiles_dir.join("alpha")).unwrap();
        fs::create_dir_all(profiles_dir.join("beta")).unwrap();
        fs::write(profiles_dir.join("alpha").join("auth.json"), "{}").unwrap();
        fs::write(profiles_dir.join("beta").join("auth.json"), "{}").unwrap();
        fs::write(temp_dir.path().join(".current_profile"), "beta").unwrap();

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let (evt_tx, evt_rx) = std::sync::mpsc::channel();
        let handle = start_worker(cmd_rx, evt_tx);

        cmd_tx.send(AppCommand::LoadProfiles).unwrap();

        let event = evt_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        match event {
            AppEvent::ProfilesLoaded(profiles) => {
                assert_eq!(profiles.len(), 2);
                assert!(profiles.iter().any(|p| p.name == "beta" && p.is_current));
            }
            _ => panic!("unexpected event"),
        }

        cmd_tx.send(AppCommand::Shutdown).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn finalize_login_adds_profile_without_switching_current() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

        let old_jwt = "eyJhbGciOiJub25lIn0.eyJlbWFpbCI6Im9sZEBleGFtcGxlLmNvbSIsImh0dHBzOi8vYXBpLm9wZW5haS5jb20vYXV0aCI6eyJjaGF0Z3B0X3BsYW5fdHlwZSI6InBybyIsImNoYXRncHRfYWNjb3VudF9pZCI6ImFjY3Rfb2xkIn19.sig";
        let old_auth = auth::AuthDotJson {
            openai_api_key: None,
            tokens: Some(auth::TokenData {
                id_token: Some(auth::IdToken::Raw(old_jwt.to_string())),
                access_token: "old-access".to_string(),
                refresh_token: "old-refresh".to_string(),
                account_id: Some("acct_old".to_string()),
            }),
            last_refresh: None,
        };

        let new_jwt = "eyJhbGciOiJub25lIn0.eyJlbWFpbCI6Im5ld0BleGFtcGxlLmNvbSIsImh0dHBzOi8vYXBpLm9wZW5haS5jb20vYXV0aCI6eyJjaGF0Z3B0X3BsYW5fdHlwZSI6InRlYW0iLCJjaGF0Z3B0X2FjY291bnRfaWQiOiJhY2N0X25ldyJ9fQ.sig";
        let new_auth = auth::AuthDotJson {
            openai_api_key: None,
            tokens: Some(auth::TokenData {
                id_token: Some(auth::IdToken::Raw(new_jwt.to_string())),
                access_token: "new-access".to_string(),
                refresh_token: "new-refresh".to_string(),
                account_id: Some("acct_new".to_string()),
            }),
            last_refresh: None,
        };

        let profiles_dir = temp_dir.path().join("profiles");
        fs::create_dir_all(profiles_dir.join("work")).unwrap();
        fs::write(
            profiles_dir.join("work").join("auth.json"),
            serde_json::to_string_pretty(&old_auth).unwrap(),
        )
        .unwrap();
        fs::write(temp_dir.path().join(".current_profile"), "work").unwrap();

        let auth_path = temp_dir.path().join("auth.json");
        fs::write(&auth_path, serde_json::to_string_pretty(&old_auth).unwrap()).unwrap();
        let previous_auth_json = fs::read_to_string(&auth_path).unwrap();

        fs::write(&auth_path, serde_json::to_string_pretty(&new_auth).unwrap()).unwrap();

        let _ = finalize_login(Some(previous_auth_json)).unwrap();

        let restored = fs::read_to_string(&auth_path).unwrap();
        let restored_value: serde_json::Value = serde_json::from_str(&restored).unwrap();
        let expected_value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string_pretty(&old_auth).unwrap()).unwrap();
        assert_eq!(restored_value, expected_value);
        assert_eq!(
            fs::read_to_string(temp_dir.path().join(".current_profile")).unwrap(),
            "work"
        );
        assert!(profiles_dir.join("new").exists());
    }

    #[test]
    fn run_login_populates_profile_quotas() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let _base_url_guard =
            StringEnvGuard::set("CODEX_ROUTER_CHATGPT_BASE_URL", format!("http://{addr}"));
        let server = thread::spawn(move || {
            let expected_requests = 2;
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            let mut handled = 0;

            while handled < expected_requests && std::time::Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(err) => panic!("failed to accept quota request: {err}"),
                };

                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let body = r#"{"plan_type":"pro","rate_limit":{"primary_window":{"used_percent":10,"reset_at":"2025-01-01T00:00:00Z"},"secondary_window":{"used_percent":20}}}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
                handled += 1;
            }

            handled
        });

        let old_jwt = "eyJhbGciOiJub25lIn0.eyJlbWFpbCI6Im9sZEBleGFtcGxlLmNvbSIsImh0dHBzOi8vYXBpLm9wZW5haS5jb20vYXV0aCI6eyJjaGF0Z3B0X3BsYW5fdHlwZSI6InBybyIsImNoYXRncHRfYWNjb3VudF9pZCI6ImFjY3Rfb2xkIn19.sig";
        let old_auth = auth::AuthDotJson {
            openai_api_key: None,
            tokens: Some(auth::TokenData {
                id_token: Some(auth::IdToken::Raw(old_jwt.to_string())),
                access_token: "old-access".to_string(),
                refresh_token: "old-refresh".to_string(),
                account_id: Some("acct_old".to_string()),
            }),
            last_refresh: None,
        };

        let new_jwt = "eyJhbGciOiJub25lIn0.eyJlbWFpbCI6Im5ld0BleGFtcGxlLmNvbSIsImh0dHBzOi8vYXBpLm9wZW5haS5jb20vYXV0aCI6eyJjaGF0Z3B0X3BsYW5fdHlwZSI6InRlYW0iLCJjaGF0Z3B0X2FjY291bnRfaWQiOiJhY2N0X25ldyJ9fQ.sig";
        let new_auth = auth::AuthDotJson {
            openai_api_key: None,
            tokens: Some(auth::TokenData {
                id_token: Some(auth::IdToken::Raw(new_jwt.to_string())),
                access_token: "new-access".to_string(),
                refresh_token: "new-refresh".to_string(),
                account_id: Some("acct_new".to_string()),
            }),
            last_refresh: None,
        };

        let profiles_dir = temp_dir.path().join("profiles");
        fs::create_dir_all(profiles_dir.join("work")).unwrap();
        fs::write(
            profiles_dir.join("work").join("auth.json"),
            serde_json::to_string_pretty(&old_auth).unwrap(),
        )
        .unwrap();
        fs::write(temp_dir.path().join(".current_profile"), "work").unwrap();

        let auth_path = temp_dir.path().join("auth.json");
        fs::write(&auth_path, serde_json::to_string_pretty(&old_auth).unwrap()).unwrap();

        let bin_dir = temp_dir.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let codex_path = bin_dir.join("codex");
        fs::write(
            &codex_path,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"login\" ]; then\n  cat > \"$CODEX_HOME/auth.json\" <<'EOF'\n{}\nEOF\n  echo \"ok\"\n  exit 0\nfi\nexit 1\n",
                serde_json::to_string_pretty(&new_auth).unwrap()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&codex_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&codex_path, perms).unwrap();
        }

        let original_path = env::var("PATH").ok();
        let _path_guard = StringEnvGuard::set(
            "PATH",
            match original_path {
                Some(existing) if existing.is_empty() => bin_dir.to_string_lossy().to_string(),
                Some(existing) => format!("{}:{}", bin_dir.to_string_lossy(), existing),
                None => bin_dir.to_string_lossy().to_string(),
            },
        );

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let (evt_tx, evt_rx) = std::sync::mpsc::channel();
        let handle = start_worker(cmd_rx, evt_tx);

        cmd_tx.send(AppCommand::RunLogin).unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut profiles_with_quota = None;
        while std::time::Instant::now() < deadline {
            let Ok(event) = evt_rx.recv_timeout(Duration::from_millis(200)) else {
                continue;
            };
            let AppEvent::ProfilesLoaded(profiles) = event else {
                continue;
            };
            if profiles.iter().any(|profile| profile.name == "new")
                && profiles.iter().all(|profile| profile.quota.is_some())
            {
                profiles_with_quota = Some(profiles);
                break;
            }
        }

        let profiles =
            profiles_with_quota.expect("expected profiles loaded with quota after login");
        assert!(profiles.iter().all(|profile| profile.quota.is_some()));

        cmd_tx.send(AppCommand::Shutdown).unwrap();
        handle.join().unwrap();
        assert_eq!(
            server.join().unwrap(),
            2,
            "expected the quota server to receive both profile requests"
        );
    }

    #[test]
    fn fetch_profile_quota_updates_specific_profile() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let _base_url_guard =
            StringEnvGuard::set("CODEX_ROUTER_CHATGPT_BASE_URL", format!("http://{addr}"));
        let server = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);

            // Try to accept connection until deadline
            let (mut stream, _) = loop {
                if std::time::Instant::now() > deadline {
                    panic!("timed out waiting for connection");
                }
                match listener.accept() {
                    Ok(v) => break v,
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(e) => panic!("accept failed: {}", e),
                }
            };

            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = r#"{"plan_type":"pro","rate_limit":{"primary_window":{"used_percent":50,"reset_at":1735689600},"secondary_window":{"used_percent":50,"reset_at":1735689600}}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let profiles_dir = temp_dir.path().join("profiles");
        fs::create_dir_all(profiles_dir.join("alpha")).unwrap();
        // Mock valid auth
        let jwt = "eyJhbGciOiJub25lIn0.eyJlbWFpbCI6ImFscGhhQGV4YW1wbGUuY29tIiwiaHR0cHM6Ly9hcGkub3BlbmFpLmNvbS9hdXRoIjp7ImNoYXRncHRfcGxhbl90eXBlIjoicHJvIiwiY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjdF9hbHBoYSJ9fQ.sig";
        let auth = auth::AuthDotJson {
            openai_api_key: None,
            tokens: Some(auth::TokenData {
                id_token: Some(auth::IdToken::Raw(jwt.to_string())),
                access_token: "access".to_string(),
                refresh_token: "refresh".to_string(),
                account_id: Some("acct_alpha".to_string()),
            }),
            last_refresh: None,
        };
        fs::write(
            profiles_dir.join("alpha").join("auth.json"),
            serde_json::to_string(&auth).unwrap(),
        )
        .unwrap();

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let (evt_tx, evt_rx) = std::sync::mpsc::channel();
        let handle = start_worker(cmd_rx, evt_tx);

        cmd_tx
            .send(AppCommand::FetchProfileQuota("alpha".to_string()))
            .unwrap();

        let event = evt_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        match event {
            AppEvent::ProfileQuotaLoaded { name, quota } => {
                assert_eq!(name, "alpha");
                assert_eq!(quota.used_requests, Some(50));
            }
            _ => panic!("unexpected event: {:?}", event),
        }

        cmd_tx.send(AppCommand::Shutdown).unwrap();
        handle.join().unwrap();
        server.join().unwrap();
    }

    #[test]
    fn test_load_profiles_marks_expired_on_401() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let _base_url_guard =
            StringEnvGuard::set("CODEX_ROUTER_CHATGPT_BASE_URL", format!("http://{addr}"));

        let server_thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = "Unauthorized";
            let response = format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let auth = auth::AuthDotJson {
            openai_api_key: None,
            tokens: Some(auth::TokenData {
                id_token: None,
                access_token: "expired-token".to_string(),
                refresh_token: "refresh".to_string(),
                account_id: Some("acct_123".to_string()),
            }),
            last_refresh: None,
        };

        let profiles_dir = temp_dir.path().join("profiles");
        fs::create_dir_all(profiles_dir.join("expired_profile")).unwrap();
        fs::write(
            profiles_dir.join("expired_profile").join("auth.json"),
            serde_json::to_string_pretty(&auth).unwrap(),
        )
        .unwrap();

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let profiles = load_profiles_with_quota(&runtime).unwrap();

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "expired_profile");
        assert_eq!(profiles[0].is_valid, false);
        assert!(profiles[0].quota.is_none());

        server_thread.join().unwrap();
    }
}
