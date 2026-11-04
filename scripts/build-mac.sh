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
echo "Built: src-tauri/target/release/bundle/macos/FlowGT Recorder.app"
