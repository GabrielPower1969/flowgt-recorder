#!/usr/bin/env bash
# build-ffmpeg-mac.sh: build the ffmpeg / ffprobe that a macOS release ships.
#
# The result is an LGPL build (no --enable-gpl, no external libraries) that links only system
# libraries and frameworks, so it runs on a Mac without Homebrew and inside the App Sandbox.
# It has no network protocols: the app never asks ffmpeg to open a URL.
#
# Usage (from the repo root):  bash scripts/build-ffmpeg-mac.sh
# Output: src-tauri/binaries/ffmpeg-<triple>, src-tauri/binaries/ffprobe-<triple>
# Work directory: src-tauri/target/ffmpeg-build (git-ignored). Takes about ten minutes.
set -euo pipefail

VERSION="7.1.1"
SHA256="733984395e0dbbe5c046abda2dc49a5544e7e0e1e2366bba849222ae9e3a03b1"
MIN_MACOS="13.0"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="${FFMPEG_BUILD_DIR:-$ROOT/src-tauri/target/ffmpeg-build}"
DEST="$ROOT/src-tauri/binaries"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  TRIPLE="aarch64-apple-darwin"; ARCH="arm64" ;;
  Darwin-x86_64) TRIPLE="x86_64-apple-darwin"; ARCH="x86_64" ;;
  *) echo "This script builds on macOS only." >&2; exit 1 ;;
esac

mkdir -p "$WORK" "$DEST"
TARBALL="$WORK/ffmpeg-$VERSION.tar.xz"
if [ ! -f "$TARBALL" ]; then
  curl --fail --location --silent --show-error -o "$TARBALL" "https://ffmpeg.org/releases/ffmpeg-$VERSION.tar.xz"
fi
GOT="$(shasum -a 256 "$TARBALL" | cut -d' ' -f1)"
if [ "$GOT" != "$SHA256" ]; then
  echo "SHA-256 mismatch for $TARBALL" >&2
  echo "  expected $SHA256" >&2
  echo "  got      $GOT" >&2
  exit 1
fi

SRC="$WORK/ffmpeg-$VERSION"
rm -rf "$SRC"
tar -xf "$TARBALL" -C "$WORK"
cd "$SRC"

# Nothing is installed; a fixed --prefix keeps this machine's folder names out of the version
# banner that ships inside the binary.
# --disable-autodetect keeps every optional external library out; the frameworks the recorder
# needs are switched back on by name. zlib is the system's own (/usr/lib/libz); PNG needs it.
./configure \
  --prefix=/usr/local \
  --arch="$ARCH" \
  --cc=clang \
  --enable-static --disable-shared \
  --disable-autodetect \
  --enable-videotoolbox --enable-audiotoolbox \
  --enable-avfoundation --enable-appkit \
  --enable-zlib \
  --disable-network \
  --disable-doc --disable-ffplay --disable-debug \
  --extra-cflags="-mmacosx-version-min=$MIN_MACOS" \
  --extra-ldflags="-mmacosx-version-min=$MIN_MACOS" \
  > "$WORK/configure.log"

make -j"$(sysctl -n hw.ncpu)" > "$WORK/make.log" 2>&1

for tool in ffmpeg ffprobe; do
  out="$DEST/$tool-$TRIPLE"
  rm -f "$out"
  cp "$SRC/$tool" "$out"
  strip -x "$out"
  chmod 755 "$out"
done

# The licence text and the exact configure line travel with the binaries (LGPL section 6).
cp "$SRC/COPYING.LGPLv2.1" "$DEST/ffmpeg-COPYING.LGPLv2.1.txt"
{
  echo "FlowGT Recorder runs ffmpeg and ffprobe $VERSION as separate programs (Contents/Helpers)."
  echo "They are free software under the GNU Lesser General Public License, version 2.1 or later;"
  echo "the licence text is in ffmpeg-COPYING.LGPLv2.1.txt next to this file."
  echo
  echo "Source: https://ffmpeg.org/releases/ffmpeg-$VERSION.tar.xz"
  echo "SHA-256: $SHA256"
  echo "Built unmodified by scripts/build-ffmpeg-mac.sh in the FlowGT Recorder repository with:"
  sed -n 's/^configuration: /  .\/configure /p' <("$DEST/ffmpeg-$TRIPLE" -hide_banner -version)
  echo
  echo "You may replace the two programs in Contents/Helpers with your own build of ffmpeg."
} > "$DEST/ffmpeg-NOTICE.txt"

# Refuse a build that would not be shippable.
FF="$DEST/ffmpeg-$TRIPLE"
if "$FF" -hide_banner -version | grep -q -- "--enable-gpl\|--enable-nonfree"; then
  echo "FAIL: this is not an LGPL build" >&2; exit 1
fi
for tool in ffmpeg ffprobe; do
  if otool -L "$DEST/$tool-$TRIPLE" | tail -n +2 | grep -v "^\s*/usr/lib/\|^\s*/System/Library/" | grep -q .; then
    echo "FAIL: $tool links a library outside the system:" >&2
    otool -L "$DEST/$tool-$TRIPLE" >&2
    exit 1
  fi
done

"$FF" -hide_banner -version | head -2
"$FF" -hide_banner -L | head -3
shasum -a 256 "$DEST/ffmpeg-$TRIPLE" "$DEST/ffprobe-$TRIPLE"
echo "Built: $DEST/ffmpeg-$TRIPLE and ffprobe-$TRIPLE"
