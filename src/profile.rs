use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::PathBuf;

use crate::auth::{self, AuthDotJson};
use crate::config::{
    get_auth_file, get_current_profile_file, get_profiles_dir, get_router_state_file,
};

/// List all available profiles
pub fn list_profiles() -> Result<()> {
    let profiles_dir = get_profiles_dir()?;

    if !profiles_dir.exists() {
        println!("{}", "No profiles found. Use 'save' to create one.".yellow());
        return Ok(());
    }

    let current_profile = get_current_profile()?;

    let mut profiles: Vec<String> = fs::read_dir(&profiles_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();

    profiles.sort();

    if profiles.is_empty() {
        println!("{}", "No profiles found. Use 'save' to create one.".yellow());
        return Ok(());
    }

    println!("{}", "Available profiles:".green().bold());

    for profile in &profiles {
        let is_current = current_profile.as_deref() == Some(profile.as_str());
        let marker = if is_current { "*" } else { " " };
        let profile_str = if is_current {
            format!("{} {} (current)", marker.cyan().bold(), profile.bold())
        } else {
            format!("{} {}", marker.white(), profile)
        };

        // Try to load and display additional info
        let profile_auth_file = profiles_dir.join(profile).join("auth.json");
        if let Ok(auth_json) = fs::read_to_string(&profile_auth_file) {
            if let Ok(auth) = serde_json::from_str::<AuthDotJson>(&auth_json) {
                if let Some(email) = auth::get_email(&auth) {
                    println!("  {} - {}", profile_str, email.dimmed());
                } else {
                    println!("  {}", profile_str);
                }
            } else {
                println!("  {}", profile_str);
            }
        } else {
            println!("  {}", profile_str);
        }
    }

    Ok(())
}

/// Switch to a profile
pub async fn switch_profile(profile_name: &str) -> Result<()> {
    let profiles_dir = get_profiles_dir()?;

    let profile_auth_file = profiles_dir.join(profile_name).join("auth.json");

    if !profile_auth_file.exists() {
        anyhow::bail!(
            "Profile '{}' not found. Use 'list' to see available profiles.",
            profile_name
        );
    }

    // Read profile auth
    let profile_auth = fs::read_to_string(&profile_auth_file)?;
    let auth: AuthDotJson = serde_json::from_str(&profile_auth)?;

    // Save to main auth.json
    let main_auth_file = get_auth_file()?;
    if let Some(parent) = main_auth_file.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&main_auth_file, serde_json::to_string_pretty(&auth)?)?;

    // Update current profile marker
    save_current_profile(profile_name)?;

    println!(
        "{} {}",
        "Switched to profile:".green().bold(),
        profile_name.cyan().bold()
    );

    // Display account info
    if let Some(email) = auth::get_email(&auth) {
        println!("{} {}", "Email:".white(), email.white());
    }

    Ok(())
}

/// Save current auth as a profile
pub fn save_profile(profile_name: &str) -> Result<()> {
    // Load current auth
    let auth = auth::load_auth()?;

    let profiles_dir = get_profiles_dir()?;

    // Create profiles directory
    fs::create_dir_all(&profiles_dir)?;

    let profile_dir = profiles_dir.join(profile_name);

    // Check if profile already exists
    if profile_dir.exists() {
        anyhow::bail!(
            "Profile '{}' already exists. Delete it first with 'delete'.",
            profile_name
        );
    }

    // Create profile directory
    fs::create_dir(&profile_dir)?;

    // Save auth to profile
    let profile_auth_file = profile_dir.join("auth.json");
    let auth_json = serde_json::to_string_pretty(&auth)?;
    fs::write(&profile_auth_file, auth_json)?;

    println!(
        "{} {}",
        "Saved current auth as profile:".green().bold(),
        profile_name.cyan().bold()
    );

    // Set as current if no current profile
    if get_current_profile()?.is_none() {
        save_current_profile(profile_name)?;
        println!("{}", "(Set as current profile)".dimmed());
    }

    Ok(())
}

/// Delete a profile
pub fn delete_profile(profile_name: &str) -> Result<()> {
    let profiles_dir = get_profiles_dir()?;

    let profile_dir = profiles_dir.join(profile_name);

    if !profile_dir.exists() {
        anyhow::bail!("Profile '{}' not found.", profile_name);
    }

    // Don't allow deleting the current profile
    if let Some(current) = get_current_profile()? {
        if current == profile_name {
            anyhow::bail!(
                "Cannot delete the current profile. Switch to another profile first."
            );
        }
    }

    fs::remove_dir_all(&profile_dir)?;

    println!(
        "{} {}",
        "Deleted profile:".green().bold(),
        profile_name.cyan().bold()
    );

    Ok(())
}

/// Show current profile info
pub fn show_current() -> Result<()> {
    let auth = auth::load_auth()?;

    if let Some(profile_name) = get_current_profile()? {
        println!(
            "{} {}",
            "Current profile:".green().bold(),
            profile_name.cyan().bold()
        );
    } else {
        println!("{}", "Current profile:".green().bold());
        println!("  (unnamed/default)");
    }

    println!();
    println!("{}", auth::format_auth_info(&auth));

    Ok(())
}

/// Get the current profile name
fn get_current_profile() -> Result<Option<String>> {
    let current_file = get_current_profile_file()?;

    if !current_file.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&current_file)?;
    Ok(Some(content.trim().to_string()))
}

/// Save the current profile name
fn save_current_profile(profile_name: &str) -> Result<()> {
    let current_file = get_current_profile_file()?;

    if let Some(parent) = current_file.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&current_file, profile_name)?;

    Ok(())
}
