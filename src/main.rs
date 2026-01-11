mod auth;
mod config;
mod profile;
mod api;
mod state;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;

#[derive(Parser)]
#[command(name = "codex-router")]
#[command(about = "Codex account switcher and quota monitor", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all profiles
    List,
    /// Switch to a profile
    Switch {
        /// Profile name
        name: String,
    },
    /// Save current auth as a profile
    Save {
        /// Profile name
        name: String,
    },
    /// Delete a profile
    Delete {
        /// Profile name
        name: String,
    },
    /// Show current profile info
    Current,
    /// Check quota for current profile
    Quota,
    /// Watch quota (refresh every 30 seconds)
    Watch,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::List => {
            profile::list_profiles()?;
        }
        Commands::Switch { name } => {
            profile::switch_profile(&name).await?;
        }
        Commands::Save { name } => {
            profile::save_profile(&name)?;
        }
        Commands::Delete { name } => {
            profile::delete_profile(&name)?;
        }
        Commands::Current => {
            profile::show_current()?;
        }
        Commands::Quota => {
            api::check_quota().await?;
        }
        Commands::Watch => {
            api::watch_quota().await?;
        }
    }

    Ok(())
}
