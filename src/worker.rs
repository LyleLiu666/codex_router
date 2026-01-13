use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;

use crate::app_state::{AppCommand, AppEvent};
use crate::login_output;
use crate::{api, auth, profile};

pub fn start_worker(cmd_rx: Receiver<AppCommand>, evt_tx: Sender<AppEvent>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("failed to create runtime");

        while let Ok(command) = cmd_rx.recv() {
            match command {
                AppCommand::LoadProfiles => {
                    match profile::list_profiles_data() {
                        Ok(profiles) => {
                            let _ = evt_tx.send(AppEvent::ProfilesLoaded(profiles));
                        }
                        Err(err) => {
                            let _ = evt_tx.send(AppEvent::Error(err.to_string()));
                        }
                    }
                }
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
                AppCommand::SaveProfile(name) => {
                    match profile::save_profile(&name) {
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
                }
                AppCommand::DeleteProfile(name) => {
                    match profile::delete_profile(&name) {
                        Ok(()) => {
                            if let Ok(profiles) = profile::list_profiles_data() {
                                let _ = evt_tx.send(AppEvent::ProfilesLoaded(profiles));
                            }
                        }
                        Err(err) => {
                            let _ = evt_tx.send(AppEvent::Error(err.to_string()));
                        }
                    }
                }
                AppCommand::FetchQuota => {
                    let result = auth::load_auth()
                        .and_then(|auth| runtime.block_on(api::fetch_quota(&auth)));
                    match result {
                        Ok(quota) => {
                            let _ = evt_tx.send(AppEvent::QuotaLoaded(quota));
                        }
                        Err(err) => {
                            let _ = evt_tx.send(AppEvent::Error(err.to_string()));
                        }
                    }
                }
                AppCommand::RunLogin => {
                    let evt_tx = evt_tx.clone();
                    std::thread::spawn(move || {
                        let _ = evt_tx.send(AppEvent::LoginOutput {
                            output: String::new(),
                            parsed: login_output::LoginOutput::default(),
                            running: true,
                        });

                        let mut child = match Command::new("codex")
                            .arg("login")
                            .stdout(Stdio::piped())
                            .stderr(Stdio::piped())
                            .spawn()
                        {
                            Ok(child) => child,
                            Err(err) => {
                                let _ = evt_tx.send(AppEvent::LoginFinished {
                                    success: false,
                                    message: format!("Failed to start codex login: {err}"),
                                });
                                return;
                            }
                        };

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

                        let _ = evt_tx.send(AppEvent::LoginFinished {
                            success,
                            message: message.clone(),
                        });

                        if success {
                            if let Ok(profiles) = profile::list_profiles_data() {
                                let _ = evt_tx.send(AppEvent::ProfilesLoaded(profiles));
                            }

                            let _ = auth::load_auth()
                                .and_then(|auth| {
                                    let runtime = tokio::runtime::Runtime::new()
                                        .expect("failed to create runtime");
                                    runtime.block_on(api::fetch_quota(&auth))
                                })
                                .map(|quota| {
                                    let _ = evt_tx.send(AppEvent::QuotaLoaded(quota));
                                });
                        }
                    });
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
    use std::fs;
    use std::time::Duration;
    use crate::test_support::{EnvGuard, ENV_LOCK};

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
}
