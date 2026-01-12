# Codex Router

A lightweight macOS desktop app for switching between multiple Codex accounts. It runs as a small window plus a menu bar tray for fast profile switching.

## Features

- Menu bar tray with quick profile switching
- Desktop window showing profiles and emails
- Refresh profiles from disk

## Requirements

- macOS
- Rust toolchain (for building)

## Setup

Codex Router reads the same auth files as Codex:

- Active auth: `~/.codex/auth.json`
- Profiles: `~/.codex/profiles/<name>/auth.json`
- Current profile marker: `~/.codex/.current_profile`

To add accounts, create a profile directory and place an `auth.json` for each account.

You can override the location with `CODEX_HOME`.

State file location: `~/.codex/router/state.json` (reserved for refresh settings).

## Build

```bash
cargo build --release
```

## Run

```bash
cargo run
```

## Usage

- Open the app window to view profiles.
- Use the tray menu to switch profiles quickly.
- Use "Refresh Profiles" to rescan `~/.codex/profiles`.

## Development

Project structure:

```
codex_router/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs
    ├── app.rs
    ├── app_state.rs
    ├── worker.rs
    ├── tray.rs
    ├── state.rs
    ├── config.rs
    ├── auth.rs
    ├── profile.rs
    └── api.rs
```

## Dependencies

- `eframe` / `egui` - desktop UI
- `tray-icon` - menu bar tray
- `reqwest` / `tokio` - API client
- `serde` / `serde_json` - serialization
- `chrono` - date/time
- `anyhow` / `thiserror` - error handling

## Security Considerations

- Respects Codex auth file permissions (0600 on Unix)
- Auth tokens are never logged
- Profile directories inherit the same security as `~/.codex`

## Troubleshooting

### "Not logged in" error

Make sure the active account has a valid `auth.json` under `~/.codex`.
