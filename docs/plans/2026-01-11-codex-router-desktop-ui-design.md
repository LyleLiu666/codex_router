# Codex Router Desktop UI (egui) Design

## Context
Codex Router is currently a CLI for switching multiple low-quota accounts. The goal is to
replace the CLI with a lightweight macOS desktop app that keeps switching fast and reliable.

## Goals
- Fast account switching for many low-quota accounts.
- Always-on menu bar entry with one-click switch.
- Simple main window for profile management and quota view.
- Non-blocking UI with smooth rendering.
- Configurable auto-refresh (default 10 minutes).

## Non-Goals
- Full native macOS look. UI can be clean but cross-platform.
- Advanced analytics or historical charts.
- Account creation or login flows (still use `codex login`).

## Primary User Flows
1. Switch account from the menu bar.
2. Open the main window, search a profile, switch.
3. View current account info and quota status.
4. Adjust auto-refresh interval or toggle.
5. Save/delete profiles.

## UI Layout
- Main window:
  - Left: profile list with search, status indicator, and current marker.
  - Top actions: Refresh, Auto-refresh toggle, Interval dropdown, Save, Delete.
  - Center: Account card (email, plan, account id).
  - Right: Quota card (requests/tokens, status message, last updated).
  - Bottom: status bar for errors and hints.
- Menu bar:
  - Title: short profile name + health badge (OK/Unknown/Low).
  - Menu items: profile list (click to switch), Refresh now, Open window,
    Auto-refresh toggle, Quit.

## Architecture
- Binary launches eframe/egui app; CLI subcommands are removed.
- Core modules keep file and API logic:
  - auth: load/save auth.json
  - profile: list/save/delete/switch profiles
  - api: fetch quota with fallback
  - config: locate ~/.codex paths and router state
- UI module:
  - AppState: profiles, current profile, account info, quota info, refresh config,
    last updated, loading/error flags, UI inputs.
  - AppCommand/AppEvent channel between UI and background worker.
- Background worker:
  - Owns a tokio runtime.
  - Executes file IO and HTTP requests.
  - Emits events back to UI; no UI blocking.

## Data Flow
- UI requests LoadProfiles and LoadCurrent at startup.
- Switching profile:
  - UI sends SwitchProfile(name).
  - Worker writes auth.json, updates current marker.
  - UI updates state and triggers quota refresh.
- Quota refresh:
  - UI sends FetchQuota.
  - Worker calls API and returns QuotaInfo or fallback.
- Auto-refresh:
  - Worker timer ticks using configured interval; emits FetchQuota.

## Persistence
- Router state stored in `~/.codex/router/state.json`:
  - refresh_interval_seconds (default 600)
  - auto_refresh_enabled
  - last_selected_profile
  - window size/position (optional)
- Reads on startup, writes on change.

## Error Handling
- Missing auth.json: show empty state with "Run codex login".
- Failed quota endpoint: show account info with "Quota unavailable" and retry option.
- Profile conflicts: show non-blocking error toast.

## Performance
- UI thread only renders; all blocking work is off-thread.
- Avoid heavy polling; auto-refresh uses 10 min default.

## Testing
- Unit tests for auth/profile/config path logic.
- Mock API tests for quota parsing and fallback.
- Manual UI smoke tests for switch/refresh/tray menu.

## Packaging
- Single binary with eframe + tray-icon.
- macOS app bundle optional later; initial build via cargo.
