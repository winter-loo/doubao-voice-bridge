#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP_DIR="$ROOT_DIR/dist/DoubaoVoiceBridge.app"

cd "$ROOT_DIR"
swift build -c release

rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
cp "$ROOT_DIR/.build/release/doubao-bridge-mac" "$APP_DIR/Contents/MacOS/doubao-bridge-mac"
cp "$ROOT_DIR/packaging/DoubaoVoiceBridge-Info.plist" "$APP_DIR/Contents/Info.plist"
chmod +x "$APP_DIR/Contents/MacOS/doubao-bridge-mac"
codesign --force --deep --sign - "$APP_DIR"

echo "$APP_DIR"
