# Architecture

```
┌──────────────────────────── FlowGT Recorder (Tauri 2) ────────────────────────────┐
│  Webview (Svelte 5 + TS)                 │  Rust app crate (src-tauri/src)         │
│  main window ─ onboarding / main /       │  commands.rs   IPC surface (types.ts)   │
│                settings / summary        │  recorder.rs   process supervisor:      │
│  floating window ─ countdown / rec panel │                segments, pause, stop,   │
│  camera preview + mic meter via          │                progress events          │
│  getUserMedia (stopped while recording   │  sidecar.rs    find ffmpeg/ffprobe      │
│  except the mic analyser)                │  permissions.rs TCC / Windows privacy   │
│  design/tokens.css used verbatim         │  tray.rs shortcuts.rs disk.rs           │
├──────────────────────────────────────────┴─────────────────────────────────────────┤
│  recorder-core (src-tauri/crates/core) — PURE, no I/O, tested on any OS           │
│  types · budget · settings · filename · disk · progress · recovery · devices ·     │
│  encoders · pip · command                                                          │
├────────────────────────────────────────────────────────────────────────────────────┤
│  ffmpeg / ffprobe sidecar (LGPL build in release; PATH ffmpeg in dev)             │
│  macOS: sck-record helper (SCK display or one window + camera + mic + system       │
│         audio → MP4; --list / --snapshot feed the picker), D27 · D28               │
│  Windows: ddagrab (D3D11) → hw encoder · dshow camera/mic · loopback device       │
└────────────────────────────────────────────────────────────────────────────────────┘
```

## Recording pipeline (one segment)

```
screen ──► scale=W:-2 (+crop) ──┐
camera ──► crop=ih:ih → hflip? → scale=S:S → shape mask ─► overlay=x:y ─► HEVC/H.264 hw encoder
mic ─────┐                                                              -b:v -maxrate -bufsize
system ──┴► amix (mixed) | two tracks (separate) ────────────────────► AAC 64–128 kbps
                                                                        ─► fragmented MP4 segment
```

* **Pause = segment boundary.** Pause closes the current ffmpeg (graceful `q`), resume starts a
  new segment. Stop concatenates all segments with the concat demuxer (`-c copy`, < 2 s).
* **Crash safety.** Every segment is written with `-movflags +frag_keyframe+empty_moov+default_base_moof`,
  and a `manifest.json` next to the segments is updated on every transition. On startup the app
  scans `<output>/.flowgt-recorder/*/manifest.json`; `recovery::decide` turns leftovers into a concat.
* **Budget.** `budget::budget_for(400, 96, 1)` → total 889 kbps, video ≈ 793 kbps, maxrate 1.5×,
  bufsize 2×. Static frames cost almost nothing under VBR; the cap keeps motion from blowing the hour.

## Module ownership during the initial build (2026-11-04)

| Owner | Files |
|---|---|
| core-A | `crates/core/src/{budget,settings,filename,disk,progress,recovery}.rs` + fixtures |
| core-B | `crates/core/src/{devices,encoders,pip,command}.rs` + fixtures |
| app | `src-tauri/src/**` (except placeholder), `src-tauri/icons/`, `Cargo.toml` deps |
| ui | `src/**` (Svelte app, i18n, stores, ipc + mock), `src/**/*.test.ts` |
| scripts | `scripts/**`, `tests/**`, `.github/workflows/ci.yml` |
| helper | `helpers/sck-record/**` (Swift: native recorder, D27) · `helpers/sck-audio/**` (Swift ScreenCaptureKit audio → PCM, ffmpeg fallback path) |
| docs | `README.md`, `CLAUDE.md`, `HOURS.md`, `00-START-HERE.md`, `docs/*.md` |
| design | `design/**`, `docs/DESIGN.md` |

`types.rs` / `types.ts` are the contract: change both in the same commit.

## IPC

Commands and events are enumerated in `src/lib/types.ts` (`Commands`, `Events`). The frontend never
calls `invoke` directly; it goes through `src/lib/ipc.ts`, which has a mock implementation used by
`vitest` and by `npm run dev` in a plain browser.

## Windows

Screen: `ddagrab` as a lavfi source (`-init_hw_device d3d11va -filter_complex "ddagrab=output_idx=N:framerate=F[...]"`),
frames stay on the GPU for NVENC; for QSV/AMF/MF they go through `hwdownload,format=bgra`. Fallback `gdigrab`.
Camera/mic: `dshow`. System audio: a loopback device ("Stereo Mix" or `virtual-audio-capturer`); a WASAPI
loopback helper is future work. **Nothing Windows-side has been executed on real hardware yet** — see
`docs/WINDOWS-VERIFY.md`.

**Start during the countdown (2026-11-20).** The first segment's ffmpeg is started when the countdown
starts (`Recorder::arm`), with every input stamped by the wall clock (`-use_wallclock_as_timestamps 1`,
`-copyts`), and given the wall-clock end of the countdown as its gate (`command::recording_command_gated`).
With QSV or Media Foundation the encoder runs from the start; a keyframe is forced at the gate and the
`noise` bitstream filter drops every packet before it, so the encoder is warm when the countdown ends.
Other encoders are cut before encoding (`trim`/`atrim`). ffmpeg prints `Press [q] to stop` at `info`
once its inputs are open: that is the "ready" of an armed segment. The countdown and the recording bar
are excluded from capture (content protection), so nothing waits for windows to disappear.
Later segments (resume, a restart) and a start without a countdown use the plain command.
Stop: ffmpeg reads stdin every 0.1 s (`-stats_period`), and the segment counts as done at its
`progress=end` block; the camera and microphone then close in the background.
Measurements: `docs/reports/12-Windows-开始与保存提速-2026-11-20.md`.
