#!/usr/bin/env bash
# build-mas.sh: build the Mac App Store variant of the macOS app.
#
# What is different from build-mac.sh:
#   * the app and every helper run inside the App Sandbox (src-tauri/entitlements/);
#   * no private macOS calls (`--features mas`, `macOSPrivateApi: false`);
#   * ffmpeg / ffprobe must be the self-contained LGPL build from build-ffmpeg-mac.sh;
#   * Hardened Runtime is on for every executable.
#
# Without any variable set, the bundle is signed with the local self-signed identity
# (setup-signing.sh). That bundle runs on this Mac only and is for testing the sandbox.
# For a store upload set:
#   MAS_APP_IDENTITY          e.g. "Apple Distribution: Your Name (TEAMID)"
#   MAS_TEAM_ID               the ten-character team id
#   MAS_PROVISIONING_PROFILE  path to the Mac App Store provisioning profile
#   MAS_INSTALLER_IDENTITY    e.g. "3rd Party Mac Developer Installer: Your Name (TEAMID)"
# Output: src-tauri/target/mas/FlowGT Recorder.app (and FlowGT Recorder.pkg with an installer identity).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BUNDLE_ID="nz.co.flowgt.recorder"
case "$(uname -m)" in
  arm64)  TRIPLE="aarch64-apple-darwin" ;;
  x86_64) TRIPLE="x86_64-apple-darwin" ;;
esac

# 1. The sidecars must be shippable before anything is built around them.
for tool in ffmpeg ffprobe; do
  bin="src-tauri/binaries/$tool-$TRIPLE"
  [ -x "$bin" ] || { echo "FAIL: $bin is missing. Run scripts/build-ffmpeg-mac.sh" >&2; exit 1; }
  if otool -L "$bin" | tail -n +2 | grep -v "^\s*/usr/lib/\|^\s*/System/Library/" | grep -q .; then
    echo "FAIL: $bin links libraries outside the system. Run scripts/build-ffmpeg-mac.sh" >&2; exit 1
  fi
done
if "src-tauri/binaries/ffmpeg-$TRIPLE" -hide_banner -version | grep -q -- "--enable-gpl\|--enable-nonfree"; then
  echo "FAIL: the ffmpeg sidecar is a GPL build. Run scripts/build-ffmpeg-mac.sh" >&2; exit 1
fi

if [ -f .env.google ]; then
  set -a; . ./.env.google; set +a
  echo "Google sign-in: enabled"
else
  echo "Google sign-in: not configured (.env.google is missing)"
fi

bash scripts/build-helper.sh

# 2. Build. The Tauri CLI edits the `tauri` features in Cargo.toml to match the config it is
# given; put the file back afterwards so the working tree stays clean.
cp src-tauri/Cargo.toml src-tauri/target/Cargo.toml.before-mas
restore() { cp src-tauri/target/Cargo.toml.before-mas src-tauri/Cargo.toml; }
trap restore EXIT
# Keep this machine's folder names (home folder, user name) out of the shipped binary.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=/build"
npm run tauri build -- --bundles app --features mas --config src-tauri/tauri.mas.conf.json
restore

OUT="src-tauri/target/mas"
APP="$OUT/FlowGT Recorder.app"
rm -rf "$OUT"; mkdir -p "$OUT"
cp -R "src-tauri/target/release/bundle/macos/FlowGT Recorder.app" "$APP"

# In Contents/MacOS, macOS treats each tool as the app itself and shows a Dock icon for it.
mkdir -p "$APP/Contents/Helpers"
for tool in "$APP"/Contents/MacOS/*; do
  [ "$(basename "$tool")" = "flowgt-recorder" ] && continue
  mv "$tool" "$APP/Contents/Helpers/"
done

# The ffmpeg licence and source notice ship inside the bundle.
mkdir -p "$APP/Contents/Resources/licenses"
for f in ffmpeg-NOTICE.txt ffmpeg-COPYING.LGPLv2.1.txt; do
  [ -f "src-tauri/binaries/$f" ] || { echo "FAIL: src-tauri/binaries/$f is missing. Run scripts/build-ffmpeg-mac.sh" >&2; exit 1; }
  cp "src-tauri/binaries/$f" "$APP/Contents/Resources/licenses/"
done

# 3. Signing identity and entitlements.
APP_ENT="$OUT/app.entitlements"
cp src-tauri/entitlements/mas.plist "$APP_ENT"
SIGN=(codesign --force --options runtime)
if [ -n "${MAS_APP_IDENTITY:-}" ]; then
  : "${MAS_TEAM_ID:?set MAS_TEAM_ID together with MAS_APP_IDENTITY}"
  /usr/libexec/PlistBuddy -c "Add :com.apple.application-identifier string $MAS_TEAM_ID.$BUNDLE_ID" "$APP_ENT"
  /usr/libexec/PlistBuddy -c "Add :com.apple.developer.team-identifier string $MAS_TEAM_ID" "$APP_ENT"
  if [ -n "${MAS_PROVISIONING_PROFILE:-}" ]; then
    cp "$MAS_PROVISIONING_PROFILE" "$APP/Contents/embedded.provisionprofile"
  else
    echo "WARNING: no MAS_PROVISIONING_PROFILE; the store will reject this bundle" >&2
  fi
  SIGN+=(--timestamp -s "$MAS_APP_IDENTITY")
  echo "Signing with: $MAS_APP_IDENTITY"
else
  bash scripts/setup-signing.sh
  KC="$ROOT/.signing/flowgt-dev.keychain-db"
  security unlock-keychain -p "$(cat .signing/keychain-password)" "$KC"
  SIGN+=(--keychain "$KC" -s "$(cat .signing/identity)")
  echo "Signing with the local test identity (this bundle cannot be uploaded)"
fi

# Helpers first, the app last: the app's signature covers the helpers'.
for tool in "$APP"/Contents/Helpers/*; do
  "${SIGN[@]}" --entitlements src-tauri/entitlements/mas-inherit.plist \
    -i "$BUNDLE_ID.$(basename "$tool")" "$tool"
done
"${SIGN[@]}" --entitlements "$APP_ENT" -i "$BUNDLE_ID" "$APP"

# 4. Checks: the signature holds, and every executable is sandboxed.
codesign --verify --deep --strict "$APP"
for exe in "$APP/Contents/MacOS/flowgt-recorder" "$APP"/Contents/Helpers/*; do
  if ! codesign -d --entitlements - --xml "$exe" 2>/dev/null | grep -q "com.apple.security.app-sandbox"; then
    echo "FAIL: $exe is not sandboxed" >&2; exit 1
  fi
done
if strings "$APP/Contents/MacOS/flowgt-recorder" "$APP"/Contents/Helpers/ffmpeg "$APP"/Contents/Helpers/ffprobe | grep -q "$HOME"; then
  echo "FAIL: the bundle contains this machine's home folder path" >&2; exit 1
fi
# No retired picture (the first icon, the first tray glyph) may be in the bundle.
bash scripts/make-icons.sh --check "$APP"
echo "Signed: $(codesign -d -r- "$APP" 2>&1 | tail -1)"
echo "Built: $APP"
# Show it in Finder, so nobody has to look for it under target/. FLOWGT_NO_REVEAL=1 skips this.
[ -n "${FLOWGT_NO_REVEAL:-}" ] || open -R "$APP" 2>/dev/null || true
echo "To put it in /Applications: bash scripts/install-mac.sh"

if [ -n "${MAS_INSTALLER_IDENTITY:-}" ]; then
  PKG="$OUT/FlowGT Recorder.pkg"
  productbuild --component "$APP" /Applications --sign "$MAS_INSTALLER_IDENTITY" "$PKG"
  echo "Package: $PKG (upload it with Transporter or xcrun altool)"
else
  echo "No MAS_INSTALLER_IDENTITY: no .pkg was made."
fi
