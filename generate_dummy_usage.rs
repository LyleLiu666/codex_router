use chrono::{Duration, Utc};
use serde::Serialize;
use std::io::Write;

#[derive(Serialize)]
struct UsageLog {
    timestamp: String,
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: Option<u64>,
    cache_creation_tokens: Option<u64>,
    cost_usd: f64,
    profile: String,
}

fn main() {
    let home = std::env::var("HOME").unwrap();
    let path = std::path::Path::new(&home).join(".codex_router/usage.jsonl");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();

    let models = vec![
        ("gpt-4o", 2.5, 10.0),
        ("claude-3-5-sonnet-20241022", 3.0, 15.0),
    ];

    for i in 0..5 {
        let date = Utc::now() - Duration::days(i);
        // Generate 3 entries per day
        for j in 0..3 {
            let (model, in_cost, out_cost) = models[j % 2];
            let input = 100 * (j + 1) as u64;
            let output = 50 * (j + 1) as u64;
            let cost = (input as f64 * in_cost + output as f64 * out_cost) / 1_000_000.0;

            let log = UsageLog {
                timestamp: date.to_rfc3339(),
                model: model.to_string(),
                input_tokens: input,
                output_tokens: output,
                cache_read_tokens: Some(10 * (j as u64)),
                cache_creation_tokens: None,
                cost_usd: cost,
                profile: "default".to_string(),
            };
            writeln!(file, "{}", serde_json::to_string(&log).unwrap()).unwrap();
        }
    }
    println!("Generated dummy data.");
}
