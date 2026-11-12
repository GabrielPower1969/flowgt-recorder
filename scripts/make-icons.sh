#!/usr/bin/env bash
# make-icons.sh — rebuild src-tauri/icons/ from the two rendered sources.
#
#   src-tauri/icons/icon-src.png         the full drawing, 1024 px  -> every picture of 64 px and larger
#   src-tauri/icons/icon-src-small.png   the simple drawing, 512 px -> every picture of 50 px and smaller
#
# The sources are rendered with Blender from scripts/icons/icon_blender.py (see src-tauri/icons/README.md).
# This script needs Node (for the Tauri CLI) and an ffmpeg: the sidecar in src-tauri/binaries/, or $FFMPEG,
# or one on PATH. The tray icons (tray*.png) are separate drawings and are left alone.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
ICONS="src-tauri/icons"

FFMPEG="${FFMPEG:-$(ls src-tauri/binaries/ffmpeg-* 2>/dev/null | head -1)}"
[ -n "$FFMPEG" ] || FFMPEG="$(command -v ffmpeg || true)"
[ -n "$FFMPEG" ] || { echo "FAIL: no ffmpeg (run scripts/fetch-ffmpeg.sh or set FFMPEG)" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cp "$ICONS/icon-src.png" "$TMP/full.png"
cp "$ICONS/icon-src-small.png" "$TMP/small.png"

# macOS draws icons on a grid where the plate is 824 of 1024 px wide; the render has it at 957 of 1024.
# So the .icns is made from copies scaled to 86 % and centred on a transparent canvas.
for v in full small; do
  "$FFMPEG" -y -loglevel error -i "$TMP/$v.png" \
    -vf "scale=iw*882/1024:-1:flags=lanczos,format=rgba,pad=iw*1024/882:ih*1024/882:(ow-iw)/2:(oh-ih)/2:color=black@0" \
    "$TMP/$v-mac.png"
done

for v in full small full-mac small-mac; do
  mkdir "$TMP/out-$v"
  npx tauri icon "$TMP/$v.png" -o "$TMP/out-$v" >/dev/null
done

for f in 64x64.png 128x128.png 128x128@2x.png icon.png Square71x71Logo.png Square89x89Logo.png \
         Square107x107Logo.png Square142x142Logo.png Square150x150Logo.png Square284x284Logo.png Square310x310Logo.png; do
  cp "$TMP/out-full/$f" "$ICONS/$f"
done
for f in 32x32.png Square30x30Logo.png Square44x44Logo.png StoreLogo.png; do
  cp "$TMP/out-small/$f" "$ICONS/$f"
done
node scripts/icons/merge.cjs ico "$TMP/out-full/icon.ico" "$TMP/out-small/icon.ico" "$ICONS/icon.ico"
node scripts/icons/merge.cjs icns "$TMP/out-full-mac/icon.icns" "$TMP/out-small-mac/icon.icns" "$ICONS/icon.icns"

echo "PASS: $ICONS rebuilt"
