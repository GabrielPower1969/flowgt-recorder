#!/usr/bin/env bash
# build-ffmpeg-win.sh: build the ffmpeg.exe / ffprobe.exe that a Windows release ships.
#
# The result is an LGPL 2.1 build (no --enable-gpl, no --enable-version3) that imports only DLLs
# that are part of Windows, so it runs on a clean machine and inside an MSIX package. It has no
# network protocols: the app never asks ffmpeg to open a URL.
# Encoders come from Windows itself (Media Foundation: h264_mf, hevc_mf), from Intel's graphics
# driver through Intel VPL (h264_qsv, hevc_qsv), plus ffmpeg's own. The one external library is the
# VPL dispatcher (MIT, linked statically): it only finds and loads the driver's own runtime at run
# time, so on a PC without Intel graphics the QSV encoders simply fail their test and are skipped.
# QSV is there because the driver's Media Foundation encoder crashes when ffmpeg.exe runs from an
# MSIX package folder (docs/WINDOWS-STORE.md, Known gaps).
#
# This is a cross build with mingw-w64. Run it on Linux, in WSL, or on a Mac with mingw-w64:
#   Ubuntu / WSL:  sudo apt-get install gcc g++ gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 make nasm \
#                    xz-utils curl cmake ninja-build pkg-config
#   macOS:         brew install mingw-w64 nasm cmake ninja pkg-config
#
# Usage (from the repo root):  bash scripts/build-ffmpeg-win.sh
# Output: src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe, ffprobe-x86_64-pc-windows-msvc.exe
#         (the name is the triple of the app that runs them, not of the compiler that built them),
#         ffmpeg-COPYING.LGPLv2.1.txt, ffmpeg-NOTICE.txt and libvpl-LICENSE.txt next to them.
# Set FFMPEG_OUT_DIR to put the five files somewhere else, for example when the development
# machine cannot run an unsigned program (Smart App Control) and keeps a signed build in binaries/.
# Work directory: src-tauri/target/ffmpeg-build-win (git-ignored), or $FFMPEG_BUILD_DIR. Under WSL
# point FFMPEG_BUILD_DIR at the Linux file system (for example "$HOME/ffmpeg-build-win"): it is
# several times faster than a Windows drive. Takes about ten minutes on four cores.
set -euo pipefail

VERSION="9.0.2"
SHA256="8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e"
# Intel VPL dispatcher: the GitHub source archive of the release tag.
VPL_VERSION="2.17.0"
VPL_SHA256="4de3e2faf1e8307fb282e4a43f443191810f6a6b0a484fffa7995ba1c814c6ec"
TRIPLE="x86_64-pc-windows-msvc"
CROSS="x86_64-w64-mingw32-"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="${FFMPEG_BUILD_DIR:-$ROOT/src-tauri/target/ffmpeg-build-win}"
DEST="${FFMPEG_OUT_DIR:-$ROOT/src-tauri/binaries}"

# cc is the build machine's own compiler: ffmpeg builds a few small generator programs with it.
for tool in cc "${CROSS}gcc" "${CROSS}g++" "${CROSS}objdump" "${CROSS}strip" make nasm curl tar cmake ninja pkg-config; do
  command -v "$tool" >/dev/null || { echo "FAIL: $tool is not installed (see the header of this script)" >&2; exit 1; }
done
sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
cores() { nproc 2>/dev/null || sysctl -n hw.ncpu; }

mkdir -p "$WORK" "$DEST"
TARBALL="$WORK/ffmpeg-$VERSION.tar.xz"
if [ ! -f "$TARBALL" ]; then
  curl --fail --location --silent --show-error -o "$TARBALL" "https://ffmpeg.org/releases/ffmpeg-$VERSION.tar.xz"
fi
GOT="$(sha256 "$TARBALL")"
if [ "$GOT" != "$SHA256" ]; then
  echo "FAIL: SHA-256 mismatch for ffmpeg-$VERSION.tar.xz" >&2
  echo "  expected $SHA256" >&2
  echo "  got      $GOT" >&2
  exit 1
fi

# Intel VPL dispatcher, a static library in $WORK/vpl (nothing is installed on this machine).
VPL_TARBALL="$WORK/libvpl-$VPL_VERSION.tar.gz"
if [ ! -f "$VPL_TARBALL" ]; then
  curl --fail --location --silent --show-error -o "$VPL_TARBALL" "https://github.com/intel/libvpl/archive/refs/tags/v$VPL_VERSION.tar.gz"
fi
GOT="$(sha256 "$VPL_TARBALL")"
if [ "$GOT" != "$VPL_SHA256" ]; then
  echo "FAIL: SHA-256 mismatch for libvpl-$VPL_VERSION.tar.gz" >&2
  echo "  expected $VPL_SHA256" >&2
  echo "  got      $GOT" >&2
  exit 1
fi
VPL_SRC="$WORK/libvpl-$VPL_VERSION"
VPL="$WORK/vpl"
rm -rf "$VPL_SRC" "$VPL" "$WORK/vpl-build"
tar -xzf "$VPL_TARBALL" -C "$WORK"
# -ffile-prefix-map keeps this machine's folder names out of the strings the dispatcher logs.
cmake -S "$VPL_SRC" -B "$WORK/vpl-build" -G Ninja \
  -DCMAKE_SYSTEM_NAME=Windows \
  -DCMAKE_C_COMPILER="${CROSS}gcc" -DCMAKE_CXX_COMPILER="${CROSS}g++" -DCMAKE_RC_COMPILER="${CROSS}windres" \
  -DCMAKE_C_FLAGS="-ffile-prefix-map=$WORK=/build" -DCMAKE_CXX_FLAGS="-ffile-prefix-map=$WORK=/build" \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$VPL" \
  -DBUILD_SHARED_LIBS=OFF -DBUILD_TESTS=OFF -DBUILD_EXAMPLES=OFF -DINSTALL_EXAMPLES=OFF \
  > "$WORK/vpl.log" 2>&1 \
  && cmake --build "$WORK/vpl-build" >> "$WORK/vpl.log" 2>&1 \
  && cmake --install "$WORK/vpl-build" >> "$WORK/vpl.log" 2>&1 \
  || { tail -20 "$WORK/vpl.log" >&2; echo "FAIL: libvpl (log: vpl.log in the work directory)" >&2; exit 1; }
# The dispatcher is C++: a C program that links it statically needs the C++ runtime as well.
sed -i.bak 's/^Libs: \(.*\)$/Libs: \1 -lstdc++/' "$VPL/lib/pkgconfig/vpl.pc"
# Only this folder's .pc files, never the build machine's own libraries.
export PKG_CONFIG_LIBDIR="$VPL/lib/pkgconfig"
unset PKG_CONFIG_PATH

SRC="$WORK/ffmpeg-$VERSION"
rm -rf "$SRC"
tar -xf "$TARBALL" -C "$WORK"
cd "$SRC"

# Nothing is installed; a fixed --prefix keeps this machine's folder names out of the version
# banner that ships inside the binary.
# --disable-autodetect keeps every optional library out. What the recorder needs from Windows is
# switched back on by name: Direct3D 11 (the ddagrab screen source), Media Foundation (hardware and
# software H.264 / HEVC encoders, AAC). The DirectShow and GDI devices need no switch. libvpl is
# the one library switched on (Intel QSV encoders; see the top of this script).
# prompeg (error correction for RTP) is not covered by --disable-network; it is switched off by name.
# -static links the compiler's own runtime in, so there is no libgcc or libwinpthread DLL to ship.
./configure \
  --prefix=/usr/local \
  --arch=x86_64 --target-os=mingw32 \
  --enable-cross-compile --cross-prefix="$CROSS" \
  --enable-static --disable-shared \
  --disable-autodetect \
  --enable-d3d11va --enable-dxva2 \
  --enable-mediafoundation \
  --enable-libvpl --pkg-config=pkg-config --pkg-config-flags=--static \
  --enable-w32threads \
  --disable-network --disable-protocol=prompeg \
  --disable-doc --disable-ffplay --disable-debug \
  --extra-ldflags="-static" \
  > "$WORK/configure.log" 2>&1 || { tail -5 "$WORK/configure.log" >&2; echo "FAIL: configure (log: configure.log in the work directory)" >&2; exit 1; }

# The parts the recorder cannot work without. configure drops a part silently when a header is
# missing from the toolchain, so check its result instead of finding out on a user's machine.
for part in ddagrab_filter dshow_indev gdigrab_indev h264_mf_encoder hevc_mf_encoder h264_qsv_encoder hevc_qsv_encoder aac_encoder mov_muxer mp4_muxer; do
  NAME="CONFIG_$(echo "$part" | tr '[:lower:]' '[:upper:]')"
  grep -q "^#define $NAME 1" config_components.h || { echo "FAIL: this toolchain cannot build $part" >&2; exit 1; }
done

make -j"$(cores)" > "$WORK/make.log" 2>&1 || { tail -20 "$WORK/make.log" >&2; echo "FAIL: make (log: make.log in the work directory)" >&2; exit 1; }

for tool in ffmpeg ffprobe; do
  out="$DEST/$tool-$TRIPLE.exe"
  rm -f "$out"
  cp "$SRC/$tool.exe" "$out"
  "${CROSS}strip" "$out"
done

# Refuse a build that would not be shippable.
CONFIG="$(sed -n 's/^#define FFMPEG_CONFIGURATION "\(.*\)"$/\1/p' config.h)"
case " $CONFIG " in
  *" --enable-gpl "*|*" --enable-nonfree "*|*" --enable-version3 "*) echo "FAIL: this is not an LGPL 2.1 build: $CONFIG" >&2; exit 1 ;;
esac
case " $CONFIG " in
  *" --disable-network "*) ;;
  *) echo "FAIL: network protocols are not switched off" >&2; exit 1 ;;
esac
for flag in $CONFIG; do
  case "$flag" in
    --enable-libvpl) ;;
    --enable-lib*) echo "FAIL: external library $flag is not on the list (libvpl only)" >&2; exit 1 ;;
  esac
done
grep -q '^#define CONFIG_GPL 0' config.h || { echo "FAIL: config.h says GPL" >&2; exit 1; }
grep -q '^#define CONFIG_NETWORK 0' config.h || { echo "FAIL: config.h says network" >&2; exit 1; }

# Every imported DLL must be one that ships with Windows 10 and 11.
SYSTEM_DLLS="advapi32 bcrypt gdi32 kernel32 mf mfplat mfuuid msvcrt ole32 oleaut32 psapi shell32 shlwapi strmiids user32 uuid avicap32 msvfw32 d3d11 dxgi dwmapi ucrtbase"
for tool in ffmpeg ffprobe; do
  for dll in $("${CROSS}objdump" -p "$DEST/$tool-$TRIPLE.exe" | sed -n 's/^\s*DLL Name: \(.*\)$/\1/p' | tr '[:upper:]' '[:lower:]'); do
    base="${dll%.dll}"
    case "$base" in api-ms-win-*) continue ;; esac
    case " $SYSTEM_DLLS " in
      *" $base "*) ;;
      *) echo "FAIL: $tool.exe imports $dll, which is not on the list of Windows system DLLs" >&2; exit 1 ;;
    esac
  done
done

# The licence text and the exact configure line travel with the binaries (LGPL section 6).
cp "$SRC/COPYING.LGPLv2.1" "$DEST/ffmpeg-COPYING.LGPLv2.1.txt"
{
  echo "FlowGT Recorder runs ffmpeg and ffprobe $VERSION as separate programs (ffmpeg.exe and"
  echo "ffprobe.exe next to the app). They are free software under the GNU Lesser General Public"
  echo "License, version 2.1 or later; the licence text is in ffmpeg-COPYING.LGPLv2.1.txt next to"
  echo "this file."
  echo
  echo "Source: https://ffmpeg.org/releases/ffmpeg-$VERSION.tar.xz"
  echo "SHA-256: $SHA256"
  echo "Built unmodified by scripts/build-ffmpeg-win.sh in the FlowGT Recorder repository"
  echo "(https://github.com/GabrielPower1969/flowgt-recorder) with $("${CROSS}gcc" --version | head -1) and:"
  echo "  ./configure $CONFIG"
  echo
  echo "Linked into both programs: the Intel Video Processing Library dispatcher (libvpl) $VPL_VERSION,"
  echo "MIT License, Copyright (c) 2020 Intel Corporation; the licence text is in libvpl-LICENSE.txt."
  echo "Source: https://github.com/intel/libvpl/archive/refs/tags/v$VPL_VERSION.tar.gz"
  echo "SHA-256: $VPL_SHA256"
  echo
  echo "You may replace the two programs with your own build of ffmpeg."
} > "$DEST/ffmpeg-NOTICE.txt"
cp "$VPL_SRC/LICENSE" "$DEST/libvpl-LICENSE.txt"

echo "configuration: $CONFIG"
for tool in ffmpeg ffprobe; do
  echo "$(sha256 "$DEST/$tool-$TRIPLE.exe")  $tool-$TRIPLE.exe  $(wc -c < "$DEST/$tool-$TRIPLE.exe") bytes"
  echo "  imports: $("${CROSS}objdump" -p "$DEST/$tool-$TRIPLE.exe" | sed -n 's/^\s*DLL Name: \(.*\)$/\1/p' | tr '\n' ' ')"
done
echo "PASS: built $DEST/ffmpeg-$TRIPLE.exe and ffprobe-$TRIPLE.exe"
