use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;

use crate::app_state::{AppCommand, AppEvent};
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
