# Feature: Usage Foundation

## Problem
Currently, `codex_router` relays requests blindly. We have no visibility into token consumption or costs.

## Solution
Interop `ccusage` logic by:
1.  Creating a `UsageManager` to log token counts to `~/.codex_router/usage.jsonl`.
2.  Intercepting upstream response streams to capture `usage` fields.
3.  Injecting `stream_options: {"include_usage": true}` to ensure upstream sends this data.

## Implementation Details
-   `src/usage.rs`: Handles file I/O for `usage.jsonl`.
-   `src/server.rs`: Wraps response body in a stream that scans for usage JSON chunks.
-   `src/codex_types.rs`: Adds `stream_options`.

## Verification
-   Run a chat request.
-   Verify `usage.jsonl` contains a new line with correct token counts.
