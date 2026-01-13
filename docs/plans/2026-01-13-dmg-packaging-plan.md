# DMG Packaging Script Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a macOS packaging script that builds a .app bundle and outputs a DMG, with optional codesign/notarize support.

**Architecture:** The script will build the release binary, assemble a minimal .app bundle (Info.plist + MacOS binary), stage an Applications symlink, and create a compressed DMG via hdiutil. Optional signing and notarization steps are gated by environment variables so the script works without Apple credentials and upgrades to signed/notarized builds later.

**Tech Stack:** Bash, cargo, hdiutil, codesign, xcrun notarytool, stapler.

### Task 1: Add a failing packaging-script test (@superpowers:test-driven-development)

**Files:**
- Create: `tests/package_script.rs`

**Step 1: Write the failing test**

```rust
use std::env;
use std::process::Command;

#[test]
fn package_script_dry_run_prints_steps() {
    let repo_root = env::current_dir().expect("current dir");
    let script = repo_root.join("scripts").join("package-dmg.sh");

    let output = Command::new("bash")
        .arg(script)
        .env("DRY_RUN", "1")
        .env("SKIP_BUILD", "1")
        .output()
        .expect("run packaging script");

    assert!(
        output.status.success(),
        "script failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hdiutil create"), "missing DMG step");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test package_script_dry_run_prints_steps -v`  
Expected: FAIL with "No such file or directory" (script missing)

**Step 3: Commit**

```bash
git add tests/package_script.rs
git commit -m "test: add packaging script dry-run check"
```

### Task 2: Implement the packaging script

**Files:**
- Create: `scripts/package-dmg.sh`

**Step 1: Write minimal implementation**

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

APP_NAME="${APP_NAME:-Codex Router}"
BUNDLE_ID="${BUNDLE_ID:-com.codex.router}"
BINARY_NAME="${BINARY_NAME:-codex_router}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/dist}"
BUILD_DIR="${BUILD_DIR:-$OUT_DIR/build}"

VERSION="$(awk -F '\"' '/^version\\s*=/{print $2; exit}' "$ROOT_DIR/Cargo.toml")"
DMG_NAME="${DMG_NAME:-Codex-Router-$VERSION}"
DMG_PATH="$OUT_DIR/$DMG_NAME.dmg"

APP_DIR="$BUILD_DIR/$APP_NAME.app"
APP_CONTENTS="$APP_DIR/Contents"
MACOS_DIR="$APP_CONTENTS/MacOS"

DRY_RUN="${DRY_RUN:-0}"
SKIP_BUILD="${SKIP_BUILD:-0}"

run_cmd() {
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "DRY RUN: $*"
  else
    "$@"
  fi
}

if [[ "$(uname)" != "Darwin" ]]; then
  echo "This script only runs on macOS." >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

if [[ "$SKIP_BUILD" != "1" ]]; then
  run_cmd cargo build --release
fi

if [[ "$DRY_RUN" != "1" ]]; then
  if [[ ! -f "$ROOT_DIR/target/release/$BINARY_NAME" ]]; then
    echo "Missing binary: $ROOT_DIR/target/release/$BINARY_NAME" >&2
    exit 1
  fi
fi

if [[ "$DRY_RUN" == "1" ]]; then
  echo "DRY RUN: rm -rf \"$BUILD_DIR\""
else
  rm -rf "$BUILD_DIR"
fi

run_cmd mkdir -p "$MACOS_DIR"

if [[ "$DRY_RUN" == "1" ]]; then
  echo "DRY RUN: install -m 755 \"$ROOT_DIR/target/release/$BINARY_NAME\" \"$MACOS_DIR/$BINARY_NAME\""
else
  install -m 755 "$ROOT_DIR/target/release/$BINARY_NAME" "$MACOS_DIR/$BINARY_NAME"
fi

if [[ "$DRY_RUN" == "1" ]]; then
  echo "DRY RUN: write Info.plist"
else
  cat >"$APP_CONTENTS/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleExecutable</key><string>$BINARY_NAME</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF
fi

if [[ -n "${CODESIGN_IDENTITY:-}" ]]; then
  run_cmd codesign --force --options runtime --timestamp --deep --sign "$CODESIGN_IDENTITY" "$APP_DIR"
fi

DMG_STAGE="$BUILD_DIR/dmg-stage"
run_cmd mkdir -p "$DMG_STAGE"

if [[ "$DRY_RUN" == "1" ]]; then
  echo "DRY RUN: cp -R \"$APP_DIR\" \"$DMG_STAGE/\""
else
  cp -R "$APP_DIR" "$DMG_STAGE/"
fi

if [[ "$DRY_RUN" == "1" ]]; then
  echo "DRY RUN: ln -s /Applications \"$DMG_STAGE/Applications\""
else
  ln -s /Applications "$DMG_STAGE/Applications"
fi

run_cmd hdiutil create -volname "$APP_NAME" -srcfolder "$DMG_STAGE" -format UDZO -ov "$DMG_PATH"

if [[ -n "${NOTARY_PROFILE:-}" ]]; then
  run_cmd xcrun notarytool submit "$DMG_PATH" --keychain-profile "$NOTARY_PROFILE" --wait
  run_cmd xcrun stapler staple "$DMG_PATH"
fi

echo "DMG created at: $DMG_PATH"
```

**Step 2: Run test to verify it passes**

Run: `cargo test package_script_dry_run_prints_steps -v`  
Expected: PASS

**Step 3: Commit**

```bash
git add scripts/package-dmg.sh
git commit -m "feat: add DMG packaging script"
```
