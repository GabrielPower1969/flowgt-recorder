# scripts/

All scripts run from the repo root. `.sh` files are bash with `set -euo pipefail`, print a final PASS/FAIL line
and exit non-zero on FAIL. Outputs go to `tests/out/` or `bench-out/` (git-ignored).
Pass criteria for scripts written by other workers are summarised from the task brief; the script header is authoritative.

| Script | Purpose | How to run | Pass criteria | Verified on |
|---|---|---|---|---|
| `fetch-ffmpeg.sh` | Copy the Homebrew ffmpeg + ffprobe into `src-tauri/binaries/` (development only: a GPL build tied to this machine) | `bash scripts/fetch-ffmpeg.sh` | Binaries present and runnable | macOS 2026-11-04 |
| `build-ffmpeg-mac.sh` | Build the ffmpeg + ffprobe a macOS release ships: LGPL, system libraries only, no network protocols. Source 7.1.1 from ffmpeg.org, SHA-256 pinned | `bash scripts/build-ffmpeg-mac.sh` (about ten minutes) | Script refuses a GPL build or a non-system library; prints both SHA-256 | macOS 2026-11-12 |
| `build-mas.sh` | Mac App Store variant: App Sandbox for the app and every helper, Hardened Runtime, no private macOS calls. Local test identity by default; real identities through `MAS_*` variables (script header) | `bash scripts/build-mas.sh` | Signature verifies; every executable carries the sandbox entitlement | macOS 2026-11-12 (local identity only; no store upload) |
| `integration.sh` | Real ffmpeg, synthetic inputs, app argv -> MP4 -> ffprobe + pixel check | `bash scripts/integration.sh` | Final line PASS | macOS 2026-11-04 |
| `crash-test.sh` | Kill the encoder mid-segment and check the fragmented MP4 is still playable | `bash scripts/crash-test.sh` | Final line PASS | macOS 2026-11-04 |
| `size-check.sh` | Output size against the 400 MB/h (889 kbps) budget | `bash scripts/size-check.sh` | Final line PASS (within budget) | macOS 2026-11-04 |
| `quality-check.sh` | Quality/size test of the encode (faster than realtime, no `-re`) | `bash scripts/quality-check.sh` | Final line PASS | macOS 2026-11-04 |
| `bench.sh` | Encoder speed, CPU, MB/h on synthetic input; `--app --seconds 60` benches the live app (needs a human) | `bash scripts/bench.sh` | Final line PASS | macOS 2026-11-04 |
| `bench-startup.sh` | Time from start to first output | `bash scripts/bench-startup.sh` | Final line PASS | macOS 2026-11-04 |
| `bench-stop-latency.sh` | Time from stop request to finalized file | `bash scripts/bench-stop-latency.sh` | Final line PASS | macOS 2026-11-04 |
| `integration.ps1` | PowerShell port of `integration.sh`; Windows encoder chain (hevc_nvenc ... libx264) | `pwsh scripts/integration.ps1 [-Seconds 6]` | 1920x1080, audio present, duration +-1 s, 30 fps, <= 1100 kbps; final line PASS | never |
| `bench.ps1` | PowerShell port of `bench.sh`; per-encoder speed, CPU, peak working set, MB/h | `pwsh scripts/bench.ps1 [-Seconds 20]` | Every encoder that ran: speed >= 1.0x, <= 480 MB/h; final line PASS | never |
| `../.github/workflows/ci.yml` | CI matrix macos-14 + windows-latest (check, test, cargo test, clippy, integration, tauri build --debug) | Runs on GitHub only | All steps green (Windows integration is continue-on-error) | never (no GitHub remote yet, D14; YAML parse only) |
