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

## 鉴权原理

Codex Router 的鉴权机制旨在无缝集成 OpenAI 的多种认证方式：

1. **双重鉴权模式**：
   - **API Key 模式**：直接使用 `OPENAI_API_KEY` 进行认证。
   - **OAuth 模式 (Tokens)**：采用标准的 OAuth2 PKCE 流程，通过浏览器登录 OpenAI 获取 `access_token`、`refresh_token` 以及包含用户信息的 `id_token` (JWT)。

2. **多账号与 Profile 管理**：
   - 鉴权信息持久化于本地的 `auth.json` 文件（默认路径为 `~/.codex/auth.json`）。
   - 支持通过 Profile 隔离多个账号，每个 Profile 拥有独立的 `auth.json`。
   - 路由时根据当前激活的 Profile 自动选择对应的凭证。

3. **自动请求封装**：
   - 转发请求时，Router 会自动在 Header 中添加 `Authorization: Bearer <token>`。
   - 针对某些后台 API，会从 `id_token` 中提取 `ChatGPT-Account-Id` 并添加到 Header 中，确保请求路由到正确的账号空间。

4. **动态令牌刷新**：
   - 当 `access_token` 过期时，系统会自动利用 `refresh_token` 换取新的访问令牌，无需用户重新登录。

5. **关键信息提取**：
   - 系统会自动解析 `id_token` 中的 JWT 载荷，从中提取用户邮箱、订阅计划类型（如 Pro/Team）以及账号 ID，用于界面展示和路由决策。

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

### 账号切换说明

**重要提示**：由于 Codex (例如 VSCode 插件) 通常会将认证信息加载到内存中，在通过本工具切换 Profile 后：

1. **需要重启 Codex**：为了使新账号生效，您需要重启正在使用 Codex 的应用程序或服务。
2. **VSCode 用户**：可以通过 VSCode 的命令面板执行 `Developer: Reload Window`，或者在插件管理界面重启 Codex 插件。

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
