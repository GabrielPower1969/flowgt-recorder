#!/usr/bin/env bash
# fetch-ffmpeg.sh — put ffmpeg/ffprobe sidecars in src-tauri/binaries/ for `tauri dev`.
#
# DEVELOPMENT ONLY. This copies the Homebrew ffmpeg (a GPL build, dynamically linked
# against /opt/homebrew/lib). It must never be used for a release bundle.
# Release builds need an LGPL static build: see src-tauri/binaries/README.md.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/src-tauri/binaries"
mkdir -p "$DEST"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  TRIPLE="aarch64-apple-darwin" ;;
  Darwin-x86_64) TRIPLE="x86_64-apple-darwin" ;;
  Linux-x86_64)  TRIPLE="x86_64-unknown-linux-gnu" ;;
  *) echo "Unsupported host $(uname -s)-$(uname -m). On Windows place ffmpeg-x86_64-pc-windows-msvc.exe and ffprobe-x86_64-pc-windows-msvc.exe in $DEST by hand." >&2; exit 1 ;;
esac

SRC_DIR="${FFMPEG_DIR:-}"
if [ -z "$SRC_DIR" ]; then
  if [ -x /opt/homebrew/bin/ffmpeg ]; then SRC_DIR=/opt/homebrew/bin
  elif [ -x /usr/local/bin/ffmpeg ]; then SRC_DIR=/usr/local/bin
  elif command -v ffmpeg >/dev/null 2>&1; then SRC_DIR="$(dirname "$(command -v ffmpeg)")"
  else echo "ffmpeg not found. Install it (brew install ffmpeg) or set FFMPEG_DIR=/path/to/dir" >&2; exit 1
  fi
fi

for tool in ffmpeg ffprobe; do
  src="$SRC_DIR/$tool"
  [ -x "$src" ] || { echo "missing $src" >&2; exit 1; }
  # Resolve symlinks (Homebrew links bin/ffmpeg -> Cellar/...) so we copy the real binary.
  real="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$src")"
  cp -f "$real" "$DEST/$tool-$TRIPLE"
  chmod +x "$DEST/$tool-$TRIPLE"
  echo "copied $real -> $DEST/$tool-$TRIPLE"
done

"$DEST/ffmpeg-$TRIPLE" -version | head -1

cat <<'NOTE' >&2

======================================================================
 NOTE: these are the Homebrew ffmpeg/ffprobe binaries — a GPL build
 (libx264, dynamically linked against /opt/homebrew/lib).
 They are for `npx tauri dev` on THIS machine only.
 A release bundle must use an LGPL static build with the same file
 names. Steps: src-tauri/binaries/README.md (download LGPL build,
 verify sha256, rename to <tool>-<target-triple>, chmod +x).
======================================================================
NOTE
