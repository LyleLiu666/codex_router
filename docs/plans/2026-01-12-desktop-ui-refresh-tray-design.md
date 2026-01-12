# Codex Router Desktop UI Refresh/Tray Design

## Context
Codex Router is now a desktop UI with a tray menu for fast profile switching. The next
milestone focuses on making quota refresh predictable, configurable from the UI, and
ensuring the window closes to the tray instead of exiting. This should remain lightweight
and responsive on macOS (M4), with no heavy polling or background work outside the
existing worker thread.

## Decisions
- Auto-refresh is UI-managed with a lightweight scheduler (no additional threads).
- Auto-refresh triggers quota refresh only (no profile refresh loop).
- Refresh interval is configured in the UI, defaulting to 10 minutes.
- Closing the window hides it and keeps the app in the tray; tray "Quit" exits.
- Refresh settings and last selected profile persist in router state.

## UI Design
- Main window starts with a "Quota" section:
  - "Refresh Now" button.
  - Auto-refresh toggle.
  - Interval input in minutes (clamped, default 10).
  - Last updated timestamp and basic quota fields.
- Profile list remains below, with current profile marked.
- Errors display inline, but do not block rendering.

## Data Flow
- On startup: load router state, apply to UI state, then load profiles.
- When profiles load: update tray menu, compare current profile to last stored, and
  request quota if the profile changed.
- Auto-refresh: UI tick checks schedule; when due, it sends FetchQuota and reschedules.
- Settings changes update router state immediately and persist to disk.

## Persistence
Router state continues to live in `~/.codex/router/state.json`:
- refresh_interval_seconds
- auto_refresh_enabled
- last_selected_profile

## Tray Behavior
- Window close: cancel close and hide window.
- Tray "Open Window": show and focus window.
- Tray "Quit": allow close and exit cleanly.
