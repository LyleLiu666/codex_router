use anyhow::{Context, Result};
use std::path::PathBuf;
use std::env;

/// Get the Codex home directory
pub fn get_codex_home() -> Result<PathBuf> {
    if let Ok(val) = env::var("CODEX_HOME") {
        if !val.is_empty() {
            return Ok(PathBuf::from(val).canonicalize()?);
        }
    }

    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .context("Cannot determine home directory")?;

    Ok(PathBuf::from(home).join(".codex"))
}

/// Get the profiles directory
pub fn get_profiles_dir() -> Result<PathBuf> {
    let codex_home = get_codex_home()?;
    Ok(codex_home.join("profiles"))
}

/// Get the current auth file path
pub fn get_auth_file() -> Result<PathBuf> {
    let codex_home = get_codex_home()?;
    Ok(codex_home.join("auth.json"))
}

/// Get the current profile marker file path
pub fn get_current_profile_file() -> Result<PathBuf> {
    let codex_home = get_codex_home()?;
    Ok(codex_home.join(".current_profile"))
}

/// Get the router config directory
pub fn get_router_config_dir() -> Result<PathBuf> {
    let codex_home = get_codex_home()?;
    Ok(codex_home.join("router"))
}

/// Get the router state file path
pub fn get_router_state_file() -> Result<PathBuf> {
    let config_dir = get_router_config_dir()?;
    Ok(config_dir.join("state.json"))
}
