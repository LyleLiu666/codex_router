use anyhow::{Context, Result};
use colored::Colorize;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::interval;

use crate::auth;

/// Quota information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QuotaInfo {
    pub account_id: String,
    pub email: String,
    pub plan_type: String,
    pub used_requests: Option<u64>,
    pub total_requests: Option<u64>,
    pub used_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub reset_date: Option<String>,
}

impl QuotaInfo {
    fn format_percentage(used: u64, total: u64) -> String {
        if total == 0 {
            return "N/A".to_string();
        }
        let pct = (used as f64 / total as f64) * 100.0;
        format!("{:.1}%", pct)
    }

    fn format_bar(used: u64, total: u64, width: usize) -> String {
        if total == 0 {
            return "│".repeat(width);
        }

        let filled = ((used as f64 / total as f64) * width as f64).ceil() as usize;
        let filled = filled.min(width);

        let filled_str = "█".repeat(filled);
        let empty_str = "░".repeat(width - filled);

        let bar = format!("{}{}", filled_str, empty_str);

        // Color code based on usage
        let pct = (used as f64 / total as f64) * 100.0;
        if pct >= 90.0 {
            bar.red().to_string()
        } else if pct >= 70.0 {
            bar.yellow().to_string()
        } else {
            bar.green().to_string()
        }
    }
}

/// Check quota for the current profile
pub async fn check_quota() -> Result<()> {
    let auth = auth::load_auth()?;

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let quota_info = fetch_quota(&client, &auth).await?;

    display_quota(&quota_info);

    Ok(())
}

/// Watch quota with auto-refresh
pub async fn watch_quota() -> Result<()> {
    let auth = auth::load_auth()?;

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let mut ticker = interval(Duration::from_secs(30));

    println!("{}", "Watching quota (Ctrl+C to exit)...".dimmed());
    println!();

    loop {
        // Clear screen and print header
        print!("\x1B[2J\x1B[1;1H");
        println!("{}", "Codex Quota Monitor".cyan().bold());
        println!("{}", "Press Ctrl+C to exit".dimmed());
        println!();

        match fetch_quota(&client, &auth).await {
            Ok(quota_info) => {
                display_quota(&quota_info);
                println!();
                println!("{}", format!("Last updated: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")).dimmed());
            }
            Err(e) => {
                println!("{} {}", "Error fetching quota:".red(), e);
            }
        }

        ticker.tick().await;
    }
}

/// Fetch quota from Codex API
async fn fetch_quota(client: &Client, auth: &auth::AuthDotJson) -> Result<QuotaInfo> {
    let access_token = auth
        .tokens
        .as_ref()
        .map(|t| t.access_token.clone())
        .or_else(|| auth.openai_api_key.clone())
        .context("No valid token found")?;

    // Note: The actual quota endpoint may be different
    // This is a placeholder implementation
    let response = client
        .get("https://api.openai.com/v1/usage")
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            let data: serde_json::Value = resp.json().await?;
            parse_quota_response(auth, &data)
        }
        Ok(resp) => {
            let status = resp.status();
            let error = resp.text().await.unwrap_or_default();
            anyhow::bail!("API returned status {}: {}", status, error);
        }
        Err(e) => {
            // If the quota endpoint doesn't work, return a mock response based on auth data
            get_fallback_quota(auth)
        }
    }
}

/// Parse quota API response
fn parse_quota_response(auth: &auth::AuthDotJson, data: &serde_json::Value) -> Result<QuotaInfo> {
    Ok(QuotaInfo {
        account_id: auth::get_account_id(auth).unwrap_or_default(),
        email: auth::get_email(auth).unwrap_or_default(),
        plan_type: auth::get_plan_type(auth).unwrap_or_else(|| "Unknown".to_string()),
        used_requests: data["data"]["usage"][0]["n_requests"].as_u64(),
        total_requests: None, // API may not provide this
        used_tokens: data["data"]["usage"][0]["n_tokens"].as_u64(),
        total_tokens: None,
        reset_date: None,
    })
}

/// Get fallback quota info when API is unavailable
fn get_fallback_quota(auth: &auth::AuthDotJson) -> Result<QuotaInfo> {
    Ok(QuotaInfo {
        account_id: auth::get_account_id(auth).unwrap_or_default(),
        email: auth::get_email(auth).unwrap_or_default(),
        plan_type: auth::get_plan_type(auth).unwrap_or_else(|| "Unknown".to_string()),
        used_requests: None,
        total_requests: None,
        used_tokens: None,
        total_tokens: None,
        reset_date: None,
    })
}

/// Display quota information
fn display_quota(quota: &QuotaInfo) {
    println!("{}", "Account Information".green().bold());
    println!("  {} {}", "Email:".white(), quota.email.cyan());
    println!("  {} {}", "Plan:".white(), quota.plan_type.yellow());
    println!("  {} {}", "Account ID:".white(), quota.account_id.dimmed());
    println!();

    println!("{}", "Usage".green().bold());

    if let (Some(used_req), Some(total_req)) = (quota.used_requests, quota.total_requests) {
        println!("  {} {}/{} ({}%)", "Requests:".white(),
            used_req, total_req,
            QuotaInfo::format_percentage(used_req, total_req)
        );
        println!("  {}", QuotaInfo::format_bar(used_req, total_req, 40));
    } else {
        println!("  {} {}", "Requests:".white(), "Not available".dimmed());
    }

    if let (Some(used_tok), Some(total_tok)) = (quota.used_tokens, quota.total_tokens) {
        println!();
        println!("  {} {}/{} ({}%)", "Tokens:".white(),
            used_tok, total_tok,
            QuotaInfo::format_percentage(used_tok, total_tok)
        );
        println!("  {}", QuotaInfo::format_bar(used_tok, total_tok, 40));
    } else {
        println!("  {} {}", "Tokens:".white(), "Not available".dimmed());
    }

    if let Some(reset_date) = &quota.reset_date {
        println!();
        println!("  {} {}", "Resets:".white(), reset_date.cyan());
    }

    println!();
    println!("{}", "Note:".yellow().italic());
    println!("  Quota information is fetched from OpenAI's API.");
    println!("  Some plans may not expose detailed usage information.");
}
