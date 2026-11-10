#!/usr/bin/env bash
# build-helper.sh: build the ScreenCaptureKit helpers and copy them where Tauri bundles them from.
#   sck-audio  — system audio / mic as PCM for the ffmpeg path (fallback)
#   sck-record — the native recorder (screen + camera + audio → MP4), D27
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
case "$(uname -m)" in
  arm64)  TRIPLE="aarch64-apple-darwin" ;;
  x86_64) TRIPLE="x86_64-apple-darwin" ;;
esac
for helper in sck-audio sck-record; do
  (cd "$ROOT/helpers/$helper" && swift build -c release)
  install -m 755 "$ROOT/helpers/$helper/.build/release/$helper" "$ROOT/src-tauri/binaries/$helper-$TRIPLE"
  echo "copied to src-tauri/binaries/$helper-$TRIPLE"
done
