# Project Vision: Smart Usage Intelligence for Codex Router

## 1. North Star (北极星愿景)
**“让每一分钱花的明明白白”**
Integrate the granular token usage analysis and cost calculation capabilities of `ccusage` into `codex_router`. 
Users should instantly know their consumption (last 5 days) without complex setups. Caching should be smart to avoid network waste.

## 2. Core Value Proposition (Wow Moments)
1.  **Zero-Config Visibility**: Just run `codex_router` and see costs. No extra tools needed.
2.  **Smart Caching**: Model prices are cached daily. No repeated slow fetches.
3.  **Unified Experience**: Usage data is naturally part of the router's lifecycle.

## 3. Product Specs (产品规格)
-   **Usage Logging**: Intercept every request/response stream and log token usage (prompt, completion, cache_read, cache_creation).
-   **Stats API**: `GET /api/codex/stats` returning simplified 5-day rolling window stats.
-   **Cost Engine**: Embedded pricing model (migrated from ccusage) with offline-first design.
-   **Performance**: Caching for remote resources (e.g. dynamic pricing if enabled) to prevent latency spikes.

## 4. Updates
-   [2026-01-31] Initial Vision: Porting ccusage logic.
