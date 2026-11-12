#!/usr/bin/env bash
# build-mac.sh: build the macOS app. Google sign-in works only when .env.google exists
# (git-ignored; see docs/MANUAL-CHECKLIST.md): FLOWGT_GOOGLE_CLIENT_ID=… FLOWGT_GOOGLE_CLIENT_SECRET=…
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
if [ -f .env.google ]; then
  set -a; . ./.env.google; set +a
  echo "Google sign-in: enabled"
else
  echo "Google sign-in: not configured (.env.google is missing)"
fi
bash scripts/build-helper.sh
npm run tauri build -- --bundles app
APP="src-tauri/target/release/bundle/macos/FlowGT Recorder.app"
# A stable signature keeps macOS privacy grants across rebuilds; see setup-signing.sh.
bash scripts/setup-signing.sh
KC="$ROOT/.signing/flowgt-dev.keychain-db"
security unlock-keychain -p "$(cat .signing/keychain-password)" "$KC"
ID="$(cat .signing/identity)"
# In Contents/MacOS, macOS treats each tool as the app itself and shows a Dock icon for it.
mkdir -p "$APP/Contents/Helpers"
for tool in "$APP"/Contents/MacOS/*; do
  [ "$(basename "$tool")" = "flowgt-recorder" ] && continue
  mv "$tool" "$APP/Contents/Helpers/"
done
for tool in "$APP"/Contents/Helpers/*; do
  codesign --force --keychain "$KC" -s "$ID" "$tool"
done
codesign --force --keychain "$KC" -s "$ID" -i nz.co.flowgt.recorder "$APP"
codesign --verify --strict "$APP" && echo "Signed: $(codesign -d -r- "$APP" 2>&1 | tail -1)"
echo "Built: $APP"
# Show it in Finder, so nobody has to look for it under target/. FLOWGT_NO_REVEAL=1 skips this.
[ -n "${FLOWGT_NO_REVEAL:-}" ] || open -R "$APP" 2>/dev/null || true
echo "To put it in /Applications: bash scripts/install-mac.sh mac"
