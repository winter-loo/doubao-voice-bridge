#!/usr/bin/env bash
set -euo pipefail

REPOSITORY_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cd "$REPOSITORY_ROOT"
shasum -a 256 -c branding/voice-t/SHA256SUMS
