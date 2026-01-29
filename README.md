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

## API Usage Example (Python)

The router exposes an OpenAI-compatible endpoint at `http://localhost:9876/v1`. You can use the standard `openai` Python library:

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:9876/v1",
    api_key="unused"  # API key is managed by the router's active profile
)

response = client.chat.completions.create(
    model="gpt-5.2-codex",
    messages=[
        {"role": "user", "content": "Hello, how are you?"}
    ],
    stream=True,
    # Optional: Codex-specific parameters
    extra_body={
        "reasoning_effort": "medium" 
    }
)

for chunk in response:
    if chunk.choices[0].delta.content:
        print(chunk.choices[0].delta.content, end="", flush=True)
```

## Development

Project structure:

```text
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
