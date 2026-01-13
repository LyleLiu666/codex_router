# Codex Router Profile Switch + Save + Login Design

## Context
Codex Router lists profiles from `~/.codex/profiles/<name>/auth.json`, but currently
only the tray supports switching and the window is read-only. Users also need a
way to save the current login as a profile, handle account reuse safely, and
trigger `codex login` from the UI when the active token is invalid.

## Goals
- Window supports switching profiles and saving the current login.
- Profiles directory is created on startup to avoid empty trays/menus.
- Saving handles account reuse by `account_id` and updates stale tokens.
- UI can run `codex login` and surface the login URL/code.

## Non-Goals
- Reimplement OAuth in-app.
- Manage multiple active accounts simultaneously.

## Decisions
- `LoadProfiles` creates `profiles/` if missing.
- Each profile row has a Switch button; current row shows a disabled Current badge.
- "Save Current Profile" writes current `auth.json` into `profiles/<name>/auth.json`.
- If `account_id` matches an existing profile, compare token fingerprints:
  - Same fingerprint: treat as already saved (no-op).
  - Different fingerprint: overwrite existing profile with current auth.
- If no `account_id`, fall back to name uniqueness (no overwrite by name).
- Login uses `codex login` as a child process; output is parsed for URL and code.

## UI Design
- Profiles header: input field + "Save Current Profile" button.
- Rows: profile name, email, and right-aligned Switch/Current badge.
- Empty state: "No profiles yet. Save current login or run codex login."
- Login area: status line + "Run codex login" button; shows parsed URL/code and
  an "Open URL" button (macOS `open`).

## Data Flow
- Startup: `LoadProfiles` -> ensure `profiles/` exists -> list profiles -> update tray.
- Save: UI -> `SaveProfile(name)` -> worker reads `auth.json`, resolves conflicts,
  writes/overwrites profile, updates `.current_profile`, reloads profiles.
- Switch: UI -> `SwitchProfile(name)` -> overwrite `~/.codex/auth.json` and update marker.
- Login: UI -> `RunLogin` -> spawn `codex login`, capture output, parse URL/code,
  on exit reload auth + profiles, then refresh quota.

## Token Fingerprint
- If `OPENAI_API_KEY` present: fingerprint = api key string.
- Else tokens present: fingerprint = concatenation of access_token, refresh_token,
  and id_token (raw string when available).
- Never log fingerprint values.

## Error Handling
- Login failures show captured output and keep the "Run codex login" button enabled.
- Save conflicts by name (no account_id) show inline error.
- Profile parse errors show a warning but do not block listing other profiles.

## Testing
- Unit tests for token fingerprint and account_id conflict update in `profile.rs`.
- Worker test for `LoadProfiles` creating `profiles/` when missing.
