#!/usr/bin/env bash
# install-mac.sh: copy the app that was built last into /Applications, so it can be opened from
# Launchpad, Spotlight and the Dock like any other app.
#
# Usage (from anywhere):  bash scripts/install-mac.sh [mas|mac]
#   mas  the Mac App Store variant from scripts/build-mas.sh (default when it exists)
#   mac  the plain build from scripts/build-mac.sh
# Nothing is built here. A running copy is asked to quit first. No sudo: if /Applications is not
# writable for this user the script says so and stops.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MAS="$ROOT/src-tauri/target/mas/FlowGT Recorder.app"
MAC="$ROOT/src-tauri/target/release/bundle/macos/FlowGT Recorder.app"
case "${1:-}" in
  mas) SRC="$MAS" ;;
  mac) SRC="$MAC" ;;
  "")  if [ -d "$MAS" ]; then SRC="$MAS"; else SRC="$MAC"; fi ;;
  *)   echo "usage: install-mac.sh [mas|mac]" >&2; exit 1 ;;
esac
[ -d "$SRC" ] || { echo "FAIL: nothing built at $SRC. Run scripts/build-mas.sh or scripts/build-mac.sh first." >&2; exit 1; }
DEST="/Applications/FlowGT Recorder.app"
[ -w /Applications ] || { echo "FAIL: /Applications is not writable for this user. Drag the app there in Finder instead:" >&2; echo "  $SRC" >&2; exit 1; }

osascript -e 'tell application id "nz.co.flowgt.recorder" to quit' >/dev/null 2>&1 || true
sleep 1
rm -rf "$DEST"
ditto "$SRC" "$DEST"
codesign --verify --deep --strict "$DEST"
# Tell the system about this copy, so Finder, the Dock and System Settings show its icon.
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$DEST" >/dev/null 2>&1 || true
echo "Installed: $DEST"
echo "Open it:   open \"$DEST\""
[ -n "${FLOWGT_NO_REVEAL:-}" ] || open -R "$DEST" 2>/dev/null || true
