#!/usr/bin/env bash
# make-icons.sh — rebuild every picture the app ships from the sources in design/brand/.
#
#   design/brand/icon-full-1024.png   the full drawing   -> every picture of 64 px and larger
#   design/brand/icon-small-512.png   the simple drawing -> every picture of 50 px and smaller
#   design/brand/tray.py              draws the menu-bar / tray glyphs
#
#   bash scripts/make-icons.sh           rebuild src-tauri/icons/, src/assets/brand/ and design/brand/ASSETS.md
#   bash scripts/make-icons.sh --check   change nothing: fail when a picture is not listed in ASSETS.md or a
#                                        retired picture is back; further arguments are bundles to look into
#
# The two icon sources are rendered with Blender from design/brand/icon_blender.py (see design/brand/README.md).
# Rebuilding needs Node (for the Tauri CLI), Python 3 and an ffmpeg: the sidecar in src-tauri/binaries/, or
# $FFMPEG, or one on PATH. The check needs Python 3 only.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
ICONS="src-tauri/icons"
BRAND="design/brand"

if [ "${1:-}" = "--check" ]; then
  shift
  exec python3 "$BRAND/assets.py" check "$@"
fi

FFMPEG="${FFMPEG:-$(ls src-tauri/binaries/ffmpeg-* 2>/dev/null | grep -v "NOTICE\|COPYING" | head -1)}"
[ -n "$FFMPEG" ] || FFMPEG="$(command -v ffmpeg || true)"
[ -n "$FFMPEG" ] || { echo "FAIL: no ffmpeg (run scripts/fetch-ffmpeg.sh or set FFMPEG)" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cp "$BRAND/icon-full-1024.png" "$TMP/full.png"
cp "$BRAND/icon-small-512.png" "$TMP/small.png"

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

# The tray glyphs are drawn, not scaled from the icon.
python3 "$BRAND/tray.py" "$ICONS"

# Inside the app (the About page).
mkdir -p src/assets/brand
for size in 128 256; do
  "$FFMPEG" -y -loglevel error -i "$TMP/full.png" -vf "scale=$size:$size:flags=lanczos" "src/assets/brand/logo-$size.png"
done

python3 "$BRAND/assets.py" write
python3 "$BRAND/assets.py" check
echo "PASS: $ICONS, src/assets/brand and $BRAND/ASSETS.md rebuilt"
