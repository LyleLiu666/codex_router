use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageLog {
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_tokens: Option<u64>,
    pub cost_usd: f64,
    pub profile: String,
}

#[derive(Debug)]
pub struct UsageManager {
    // Simple in-memory cache for pricing for now
    pricing: Mutex<HashMap<String, ModelPricing>>,
}

#[derive(Clone, Copy, Debug)]
struct ModelPricing {
    input_cost_per_m: f64,
    output_cost_per_m: f64,
    cache_read_cost_per_m: f64,
    cache_creation_cost_per_m: f64,
}

impl UsageManager {
    pub fn new() -> Result<Self> {
        // Initialize with rough defaults for popular models
        // TODO: Load from ccusage caching logic later
        let mut pricing = HashMap::new();
        // Simple simplified defaults
        pricing.insert(
            "gpt-4o".to_string(),
            ModelPricing {
                input_cost_per_m: 2.5,
                output_cost_per_m: 10.0,
                cache_read_cost_per_m: 1.25,
                cache_creation_cost_per_m: 0.0,
            },
        );
        pricing.insert(
            "claude-3-5-sonnet-20241022".to_string(),
            ModelPricing {
                input_cost_per_m: 3.0,
                output_cost_per_m: 15.0,
                cache_read_cost_per_m: 0.3,
                cache_creation_cost_per_m: 3.75,
            },
        );

        Ok(Self {
            pricing: Mutex::new(pricing),
        })
    }

    fn get_log_path(&self) -> PathBuf {
        // Try getting CODEX_HOME, fallback to temp dir if it fails (e.g. in tests)
        match config::get_codex_home() {
            Ok(home) => home.join("usage.jsonl"),
            Err(_) => {
                // Determine fallback, e.g. temp dir
                std::env::temp_dir().join("codex_router_usage_fallback.jsonl")
            }
        }
    }

    pub fn log_usage(
        &self,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: Option<u64>,
        cache_creation_tokens: Option<u64>,
        profile: &str,
    ) -> Result<()> {
        let pricing = self.get_approximate_pricing(model);
        let cost_usd = (input_tokens as f64 * pricing.input_cost_per_m
            + output_tokens as f64 * pricing.output_cost_per_m
            + cache_read_tokens.unwrap_or(0) as f64 * pricing.cache_read_cost_per_m
            + cache_creation_tokens.unwrap_or(0) as f64 * pricing.cache_creation_cost_per_m)
            / 1_000_000.0;

        let log = UsageLog {
            timestamp: Utc::now(),
            model: model.to_string(),
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            cost_usd,
            profile: profile.to_string(),
        };

        let json = serde_json::to_string(&log)?;
        let log_path = self.get_log_path();

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;

        writeln!(file, "{}", json)?;
        Ok(())
    }

    fn get_approximate_pricing(&self, model: &str) -> ModelPricing {
        let pricing = self.pricing.lock().unwrap();
        // Very basic fallback
        *pricing
            .get(model)
            .or_else(|| {
                // Try flexible matching
                if model.contains("gpt-4") {
                    pricing.get("gpt-4o")
                } else if model.contains("claude-3-5") {
                    pricing.get("claude-3-5-sonnet-20241022")
                } else {
                    None
                }
            })
            .unwrap_or(&ModelPricing {
                input_cost_per_m: 1.0,
                output_cost_per_m: 2.0,
                cache_read_cost_per_m: 0.0,
                cache_creation_cost_per_m: 0.0,
            })
    }

    pub fn get_stats(&self, days: i64) -> Result<UsageStats> {
        // TODO: Read file backwards or use rev_lines crate for efficiency if file is large
        // For now, simple read
        let log_path = self.get_log_path();
        if !log_path.exists() {
            return Ok(UsageStats::default());
        }

        let content = std::fs::read_to_string(&log_path)?;
        let cutoff = Utc::now() - chrono::Duration::days(days);

        let mut stats = UsageStats::default();

        let mut daily_map: HashMap<String, DailyUsageStats> = HashMap::new();

        for line in content.lines() {
            if let Ok(log) = serde_json::from_str::<UsageLog>(line) {
                if log.timestamp > cutoff {
                    stats.total_cost_usd += log.cost_usd;
                    stats.total_input_tokens += log.input_tokens;
                    stats.total_output_tokens += log.output_tokens;
                    stats.total_cache_read_tokens += log.cache_read_tokens.unwrap_or(0);

                    // Aggregate daily
                    let date_str = log.timestamp.format("%Y-%m-%d").to_string();
                    let daily = daily_map
                        .entry(date_str.clone())
                        .or_insert(DailyUsageStats {
                            date: date_str,
                            ..Default::default()
                        });
                    daily.cost_usd += log.cost_usd;
                    daily.input_tokens += log.input_tokens;
                    daily.output_tokens += log.output_tokens;
                    daily.cache_read_tokens += log.cache_read_tokens.unwrap_or(0);
                }
            }
        }

        // Sort daily stats by date descending
        let mut daily_vec: Vec<DailyUsageStats> = daily_map.into_values().collect();
        daily_vec.sort_by(|a, b| b.date.cmp(&a.date));
        stats.daily_stats = daily_vec;

        Ok(stats)
    }
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct UsageStats {
    pub total_cost_usd: f64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub daily_stats: Vec<DailyUsageStats>,
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct DailyUsageStats {
    pub date: String, // YYYY-MM-DD
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{EnvGuard, ENV_LOCK};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_log_usage() {
        let _lock = ENV_LOCK.lock().unwrap();
        // Mock config::get_codex_home environment?
        // UsageManager calls config::get_codex_home().
        // We probably need to mock that or structure UsageManager to accept a path.
        // It's hard to mock get_codex_home directly without support.
        // Let's modify UsageManager::new to take an optional path or be testable.
        // OR better, since I can't easily change `new` signature without affecting call sites (SharedState),
        // I can just rely on `CODEX_HOME` env var if config uses it.
        // Checking config.rs: it uses env::var("CODEX_HOME").

        let temp_dir = tempdir().unwrap();
        let _guard = EnvGuard::set("CODEX_HOME", temp_dir.path());

        let mgr = UsageManager::new().unwrap();
        mgr.log_usage("gpt-4o", 100, 50, None, None, "default")
            .unwrap();

        // Unset to be safe, though test thread safety with env vars is tricky.
        // But cargo test runs in parallel. `rusty-fork` or `serial_test`?
        // Or just run one test at a time.

        let log_path = temp_dir.path().join("usage.jsonl");
        assert!(log_path.exists());

        let content = fs::read_to_string(log_path).unwrap();
        assert!(content.contains("\"model\":\"gpt-4o\""));
        assert!(content.contains("\"input_tokens\":100"));
    }
}
