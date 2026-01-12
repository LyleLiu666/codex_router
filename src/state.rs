use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;

use crate::config::get_router_state_file;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouterState {
    pub refresh_interval_seconds: u64,
    pub auto_refresh_enabled: bool,
    pub last_selected_profile: Option<String>,
}

impl Default for RouterState {
    fn default() -> Self {
        Self {
            refresh_interval_seconds: 600,
            auto_refresh_enabled: true,
            last_selected_profile: None,
        }
    }
}

pub fn load_state() -> Result<RouterState> {
    let state_file = get_router_state_file()?;
    if !state_file.exists() {
        return Ok(RouterState::default());
    }

    let contents = fs::read_to_string(&state_file)
        .with_context(|| format!("Failed to read state file: {:?}", state_file))?;
    let state: RouterState = serde_json::from_str(&contents)
        .with_context(|| "Failed to parse router state")?;
    Ok(state)
}

pub fn save_state(state: &RouterState) -> Result<()> {
    let state_file = get_router_state_file()?;
    if let Some(parent) = state_file.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = serde_json::to_string_pretty(state)?;
    fs::write(&state_file, contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::get_router_state_file;
    use crate::test_support::{EnvGuard, ENV_LOCK};

    #[test]
    fn loads_default_state_when_missing() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

        let state = load_state().unwrap();

        assert_eq!(state.refresh_interval_seconds, 600);
        assert!(state.auto_refresh_enabled);
        assert!(state.last_selected_profile.is_none());
    }

    #[test]
    fn saves_and_loads_state() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

        let original = RouterState {
            refresh_interval_seconds: 300,
            auto_refresh_enabled: false,
            last_selected_profile: Some("work".to_string()),
        };

        save_state(&original).unwrap();
        let state_file = get_router_state_file().unwrap();
        assert!(state_file.exists(), "state file missing at {:?}", state_file);
        let loaded = load_state().unwrap();

        assert_eq!(loaded, original);
    }
}
