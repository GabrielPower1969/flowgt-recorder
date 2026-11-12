# Mac App Store variant

`scripts/build-mas.sh` builds a second flavour of the macOS app that follows the Mac App Store rules.
The ordinary build (`scripts/build-mac.sh`) is unchanged. Both come from the same source; the
differences are one cargo feature (`mas`), one config overlay (`src-tauri/tauri.mas.conf.json`),
two entitlement files and the signing step.

Evidence for every "verified" below: `docs/reports/06-Mac-上架版重构-2026-11-12.md`.

## What is different

| Area | Ordinary build | Store variant |
|---|---|---|
| App Sandbox | off | on for the app (`src-tauri/entitlements/mas.plist`) and inherited by every helper (`mas-inherit.plist`) |
| Hardened Runtime | off | on for every executable |
| ffmpeg / ffprobe | whatever is in `src-tauri/binaries/` | must be the LGPL, system-libraries-only build from `scripts/build-ffmpeg-mac.sh`; the script refuses anything else |
| See-through windows | Tauri `macOSPrivateApi` | off. The floating bar is an opaque window with corners rounded by AppKit; the screen outline is a native bordered window (no screen number on it) |
| Save folder | `~/Movies/FlowGT Recorder` | the same folder (Movies entitlement). A folder picked in Settings is kept across launches with a security-scoped bookmark (`src-tauri/src/bookmark.rs`) |
| Build-machine paths in the binary | present | removed (`--remap-path-prefix`, no `CARGO_MANIFEST_DIR` lookups); the script checks |
| ffmpeg licence | not bundled | `Contents/Resources/licenses/` holds the notice (source URL, hash, configure line) and the LGPL 2.1 text |

Runtime code asks `commands::is_sandboxed()` (the system sets `APP_SANDBOX_CONTAINER_ID`); compile-time
differences sit behind `feature = "mas"` and are limited to `windows.rs` and `sidecar::source_dir`.

## Entitlements and why each is there

| Entitlement | Reason |
|---|---|
| `app-sandbox` | required by the store |
| `device.camera`, `device.audio-input` | camera picture-in-picture, microphone |
| `assets.movies.read-write` | recordings go to `~/Movies/FlowGT Recorder` |
| `files.user-selected.read-write`, `files.bookmarks.app-scope` | a save folder the user picks, remembered across launches |
| `network.client` | upload to the user's own Google Drive after sign-in (D19) |
| `network.server` | the one-shot loopback listener that receives the Google sign-in redirect |

Screen recording has no entitlement; it is the user's grant in System Settings.

## Build

```sh
bash scripts/build-ffmpeg-mac.sh   # once, about ten minutes
bash scripts/build-mas.sh          # local test identity: runs on this Mac only
```

For an upload, set `MAS_APP_IDENTITY`, `MAS_TEAM_ID`, `MAS_PROVISIONING_PROFILE` and
`MAS_INSTALLER_IDENTITY` (see the script header). Then the script also adds the application and team
identifiers to the entitlements, embeds the profile and writes a signed `.pkg`.

A helper signed with `inherit` runs only when a sandboxed app starts it. Started from a terminal it is
killed at once; test the tools through the app, or use the copies in `src-tauri/binaries/`.

## Known gaps

- Not uploaded to App Store Connect; no Apple identity was available. The signing branch that uses
  `MAS_*` variables has never run.
- Settings and the Google sign-in of an existing non-sandboxed install are not carried into the
  sandbox container; the store variant starts with defaults.
- macOS shows its "bypass the system private window picker" prompt when the app first lists or
  records screens. The app uses ScreenCaptureKit directly instead of the system picker. Whether
  App Review accepts that for this app is not known.
- A save folder picked in Settings: bookmark code is unit-tested; the full path (pick, quit, relaunch,
  record) has not been run in the sandbox.
- "Open System Settings" from the permission notice runs `open`; not exercised in the sandbox.
- Apple Silicon only. ffmpeg and ffprobe add about 40 MB; a trimmed build would be smaller.
- `drawsBackground` still appears as a string in the binary: wry compiles the call in and runs it only
  for a see-through window, which the store variant never creates.
