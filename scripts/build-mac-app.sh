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

SIGNING_IDENTITY="${DOUBAO_CODESIGN_IDENTITY:-}"
if [[ -z "$SIGNING_IDENTITY" ]]; then
    SIGNING_IDENTITY="$({ security find-identity -v -p codesigning 2>/dev/null || true; } \
        | awk '/^[[:space:]]*[0-9]+\)/ { print $2; exit }')"
fi

if [[ -n "$SIGNING_IDENTITY" ]] && codesign --force --deep --sign "$SIGNING_IDENTITY" "$APP_DIR"; then
    echo "Signed with identity: $SIGNING_IDENTITY"
else
    codesign \
        --force \
        --deep \
        --sign - \
        --identifier local.doubao.voicebridge \
        --requirements '=designated => identifier "local.doubao.voicebridge"' \
        "$APP_DIR"
    echo "warning: using ad-hoc signing with a stable designated requirement" >&2
fi

echo "$APP_DIR"
