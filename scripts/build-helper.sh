#!/usr/bin/env bash
# build-helper.sh: build the ScreenCaptureKit audio helper and copy it where Tauri bundles it from.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
(cd "$ROOT/helpers/sck-audio" && swift build -c release)
case "$(uname -m)" in
  arm64)  TRIPLE="aarch64-apple-darwin" ;;
  x86_64) TRIPLE="x86_64-apple-darwin" ;;
esac
install -m 755 "$ROOT/helpers/sck-audio/.build/release/sck-audio" "$ROOT/src-tauri/binaries/sck-audio-$TRIPLE"
echo "copied to src-tauri/binaries/sck-audio-$TRIPLE"
