#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

APP_NAME="${APP_NAME:-Codex Router}"
BUNDLE_ID="${BUNDLE_ID:-com.codex.router}"
BINARY_NAME="${BINARY_NAME:-codex_router}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/dist}"
BUILD_DIR="${BUILD_DIR:-$OUT_DIR/build}"

VERSION="$(grep "^version =" "$ROOT_DIR/Cargo.toml" | head -n1 | cut -d '"' -f 2)"

if [[ -z "$VERSION" ]]; then
  echo "Error: Could not extract version from Cargo.toml" >&2
  exit 1
fi
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
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF
fi

# Generate AppIcon.icns
RESOURCES_DIR="$APP_CONTENTS/Resources"
run_cmd mkdir -p "$RESOURCES_DIR"

if [[ "$DRY_RUN" == "1" ]]; then
  echo "DRY RUN: Generate AppIcon.icns from assets/icon.png"
else
  ICON_SOURCE="$ROOT_DIR/assets/icon.png"
  if [[ -f "$ICON_SOURCE" ]]; then
    ICONSET_DIR="$BUILD_DIR/AppIcon.iconset"
    mkdir -p "$ICONSET_DIR"
    
    # Generate various sizes
    sips -z 16 16     "$ICON_SOURCE" --out "$ICONSET_DIR/icon_16x16.png" > /dev/null
    sips -z 32 32     "$ICON_SOURCE" --out "$ICONSET_DIR/icon_16x16@2x.png" > /dev/null
    sips -z 32 32     "$ICON_SOURCE" --out "$ICONSET_DIR/icon_32x32.png" > /dev/null
    sips -z 64 64     "$ICON_SOURCE" --out "$ICONSET_DIR/icon_32x32@2x.png" > /dev/null
    sips -z 128 128   "$ICON_SOURCE" --out "$ICONSET_DIR/icon_128x128.png" > /dev/null
    sips -z 256 256   "$ICON_SOURCE" --out "$ICONSET_DIR/icon_128x128@2x.png" > /dev/null
    sips -z 256 256   "$ICON_SOURCE" --out "$ICONSET_DIR/icon_256x256.png" > /dev/null
    sips -z 512 512   "$ICON_SOURCE" --out "$ICONSET_DIR/icon_256x256@2x.png" > /dev/null
    sips -z 512 512   "$ICON_SOURCE" --out "$ICONSET_DIR/icon_512x512.png" > /dev/null
    sips -z 1024 1024 "$ICON_SOURCE" --out "$ICONSET_DIR/icon_512x512@2x.png" > /dev/null
    
    iconutil -c icns "$ICONSET_DIR" -o "$RESOURCES_DIR/AppIcon.icns"
    rm -rf "$ICONSET_DIR"
  else
    echo "Warning: assets/icon.png not found, skipping icon generation"
  fi
fi

if [[ -n "${CODESIGN_IDENTITY:-}" ]]; then
  run_cmd codesign --force --options runtime --timestamp --deep --sign "$CODESIGN_IDENTITY" "$APP_DIR"
else
  # Ad-hoc sign for local use (required on Apple Silicon)
  echo "No identity provided, performing ad-hoc signing..."
  run_cmd codesign --force --deep --sign - "$APP_DIR"
fi

# Create DMG with drag-and-drop interface
if command -v create-dmg &>/dev/null; then
  # Remove old DMG if exists (create-dmg doesn't have -ov flag)
  rm -f "$DMG_PATH"
  
  run_cmd create-dmg \
    --volname "$APP_NAME" \
    --window-pos 200 120 \
    --window-size 600 400 \
    --icon-size 100 \
    --icon "$APP_NAME.app" 150 190 \
    --app-drop-link 450 190 \
    --no-internet-enable \
    "$DMG_PATH" \
    "$APP_DIR"
else
  echo "Warning: create-dmg not found, using basic hdiutil (no visual interface)"
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
  
  run_cmd hdiutil create -volname "$APP_NAME" -srcfolder "$DMG_STAGE" -ov -format UDZO "$DMG_PATH"
fi

if [[ -n "${NOTARY_PROFILE:-}" ]]; then
  run_cmd xcrun notarytool submit "$DMG_PATH" --keychain-profile "$NOTARY_PROFILE" --wait
  run_cmd xcrun stapler staple "$DMG_PATH"
fi

echo "DMG created at: $DMG_PATH"
