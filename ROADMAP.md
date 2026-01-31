# Roadmap: Usage Intelligence Integration

## Dependencies
`Infrastructure` -> `Core Logic` -> `API`

## Changes

- [ ] **feat-usage-foundation**: Establish `UsageManager` and intercept streams to log token usage. <!-- status: in-progress -->
    - *Goal*: Capture raw token counts to `usage.jsonl`.
- [ ] **feat-cost-engine**: Implement pricing logic and smart caching for model prices.
    - *Goal*: Convert tokens to USD, caching prices daily to avoid latency.
- [ ] **feat-stats-api**: Expose aggregated statistics for the frontend/CLI.
    - *Goal*: `GET /api/codex/stats` serves 5-day history.
