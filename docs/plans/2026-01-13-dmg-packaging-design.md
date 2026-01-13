# DMG Packaging Design

**Goal:** Provide a single command that turns the Rust release binary into a macOS .app bundle and a distributable DMG. The script must work without Apple Developer credentials and optionally support codesign and notarization when those credentials exist.

**Scope:** Build the release binary, assemble a minimal bundle (Info.plist + MacOS binary), stage a DMG with an Applications symlink, and produce a compressed DMG. The output should be deterministic and safe to run multiple times. The script should not depend on third-party tooling beyond standard macOS utilities and cargo.

## Packaging Flow

The script will run from the repository root, call `cargo build --release`, and then create `dist/build/<App>.app/Contents/MacOS/` to host the executable. It writes a minimal Info.plist using values derived from Cargo.toml (version) and environment overrides for app name, bundle ID, and binary name. A staging directory is then created that includes the app bundle and an `Applications` symlink, and `hdiutil create` produces the DMG. This keeps the packaging logic transparent and simple to debug.

## Signing and Notarization

Signing and notarization are optional and driven by environment variables. If `CODESIGN_IDENTITY` is set, the script will sign the bundle with hardened runtime options. If `NOTARY_PROFILE` is set, the script will submit the DMG with `xcrun notarytool` and staple the result. This design allows immediate use without credentials and a smooth upgrade path once the developer account is configured.
