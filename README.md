# Codex Router

A Rust-based client tool for managing multiple Codex accounts and monitoring quota usage on Mac M4.

## Features

- **Multi-account Management**: Save and switch between multiple Codex accounts
- **Quota Monitoring**: Check API usage and remaining quota
- **Real-time Watch Mode**: Monitor quota with auto-refresh
- **Secure**: Leverages Codex's existing auth storage system

## Installation

### Prerequisites

1. Install Rust (if not already installed):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

2. Install Codex CLI (if not already installed):

```bash
npm install -g @anthropics/codex
# or
pip install codex-cli
```

### Build and Install

```bash
# Clone or navigate to the codex_router directory
cd /Users/liu_y/code/opensource/codex_router

# Build release binary
cargo build --release

# Install to system (optional)
sudo cp target/release/codex-router /usr/local/bin/
```

## Usage

### Initial Setup

First, log in to Codex with your primary account:

```bash
codex login
```

Then save it as a profile:

```bash
codex-router save personal
```

### Profile Commands

#### List all profiles

```bash
codex-router list
```

Output:

```
Available profiles:
* personal (current)
  work - work@example.com
```

#### Switch profile

```bash
codex-router switch work
```

#### Save current login as a profile

```bash
codex-router save client-abc
```

#### Delete a profile

```bash
codex-router delete client-abc
```

#### Show current profile info

```bash
codex-router current
```

Output:

```
Current profile: personal

Email: user@example.com
Account ID: org_abc123
Plan: Plus
```

### Quota Commands

#### Check quota

```bash
codex-router quota
```

Output:

```
Account Information
  Email: user@example.com
  Plan: Plus
  Account ID: org_abc123

Usage
  Requests: Not available
  Tokens: Not available

Note:
  Quota information is fetched from OpenAI's API.
  Some plans may not expose detailed usage information.
```

#### Watch quota (auto-refresh)

```bash
codex-router watch
```

This will display a real-time updating dashboard (refreshes every 30 seconds).

Press `Ctrl+C` to exit.

## Architecture

### File Structure

```
~/.codex/
├── auth.json              # Current active account (managed by Codex)
├── config.toml            # Codex configuration
├── profiles/              # Account profiles (managed by codex-router)
│   ├── personal/
│   │   └── auth.json
│   └── work/
│       └── auth.json
└── .current_profile       # Current profile marker
```

### How it Works

1. **Profile Management**: Copies `auth.json` files between `~/.codex/profiles/<name>/` and `~/.codex/auth.json`
2. **Account Switching**: When switching profiles, replaces the active `auth.json` with the selected profile's auth
3. **Quota Monitoring**: Uses the Codex API to fetch usage information with the current token

## Development

### Project Structure

```
codex_router/
├── Cargo.toml           # Project configuration and dependencies
├── README.md            # This file
└── src/
    ├── main.rs          # CLI entry point
    ├── config.rs        # Configuration and path management
    ├── auth.rs          # Auth data structures and loading
    ├── profile.rs       # Profile management logic
    └── api.rs           # Quota API client
```

### Build for Release

```bash
cargo build --release
```

The binary will be at `target/release/codex-router`.

### Run in Development Mode

```bash
cargo run -- list
cargo run -- switch work
cargo run -- quota
```

## Dependencies

- `clap` - CLI argument parsing
- `reqwest` - HTTP client for API calls
- `tokio` - Async runtime
- `serde` - Serialization/deserialization
- `chrono` - Date/time handling
- `anyhow` - Error handling
- `colored` - Terminal colors

## Security Considerations

- This tool respects Codex's auth file permissions (0600 on Unix)
- Auth tokens are never logged or displayed
- Profile directories inherit the same security as the main `.codex` directory

## Troubleshooting

### "Not logged in" error

Make sure you've logged in with Codex first:

```bash
codex login
```

### Profile not found

Check available profiles:

```bash
codex-router list
```

### Quota information not available

The OpenAI API may not expose detailed quota information for all plan types. Basic account information will still be displayed.

## License

This is a utility tool for managing Codex accounts. Please refer to the Codex project for the main license terms.

## Related

- [Codex](https://github.com/anthropics/codex) - Main Codex CLI project
- [CODEX_ARCHITECTURE.md](../codex/CODEX_ARCHITECTURE.md) - Detailed Codex architecture documentation
