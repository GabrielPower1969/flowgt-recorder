# Decisions

Format: `D<n> · <date> · <decision>` — why — consequences. Newest at the bottom. Nothing here is
deleted; a reversed decision gets a new entry that points back.

D1 · 2026-11-04 · **Tauri 2 + Rust core + Svelte 5 (Vite, TS)**. Smallest footprint of the
cross-platform shells (system webview, no bundled Chromium), Rust for process supervision, Svelte 5 for a
60 fps UI without a virtual DOM. Consequence: WKWebView on macOS, WebView2 on Windows; CSS must work on both.

D2 · 2026-11-04 · **ffmpeg/ffprobe as a sidecar, not linked.** Capture + encode + mux in one battle-tested
binary; swapping to a native ScreenCaptureKit path later only changes the *input* side. Dev uses the PATH
ffmpeg (Homebrew 7.1.1, a GPL build with libx264). Release bundles an **LGPL** build under
`src-tauri/binaries/` (no libx264; fallback chain ends in libopenh264/mpeg4) and ships the LGPL notice.
See `docs/LICENSES.md`.

D3 · 2026-11-04 · **Pure core crate separate from the Tauri crate.** All logic that can be tested
without a GUI, a camera or ffmpeg lives in `recorder-core` and runs under `cargo test` on any OS — this
is how the Windows command paths get tested on a Mac.

D4 · 2026-11-04 · **Pause = segment boundary; stop = concat; crash recovery = concat leftovers.** ffmpeg
has no pause; SIGSTOP breaks avfoundation timing. Segments with fragmented MP4 give pause, resume and
kill -9 safety with one mechanism. Cost: a sub-second gap at each pause (acceptable for meetings).

D5 · 2026-11-04 · **Bitrate budget, not CRF.** 400 MB/h ⇒ 889 kbps total; video gets the remainder after
audio; `-maxrate 1.5×`, `-bufsize 2×`. Hardware encoders honour `-b:v`; CRF would make the hour unbounded.

D6 · 2026-11-04 · **Camera preview and mic meter through the webview (`getUserMedia`)**, not a second
ffmpeg. Zero extra processes; the preview is torn down when recording starts (the camera then belongs
to ffmpeg); the mic analyser keeps running (mic is shareable, < 1 % CPU) to drive the floating meter.

D7 · 2026-11-04 · **System audio on macOS: ScreenCaptureKit helper (Swift CLI → PCM FIFO) as the
primary path, loopback device (BlackHole) as fallback, off as last resort.** avfoundation cannot capture
system audio. The helper is in `helpers/sck-audio/`; v0.1 marks it *experimental* and lets the user
pick a loopback device instead. Final choice is a listed next step in `00-START-HERE.md`.

D8 · 2026-11-04 · **Windows: ddagrab + dshow + loopback device.** ffmpeg has no WASAPI loopback input;
"Stereo Mix" or `virtual-audio-capturer` are offered, a native helper is future work. Windows code is
covered by unit tests with fixture output only; **not executed on Windows hardware** in this build.

D9 · 2026-11-04 · **Window capture deferred.** avfoundation captures displays only. The UI shows the
option disabled with "native path coming"; `CaptureTarget::Window` exists in the contract so the
ScreenCaptureKit/WGC path can land without an IPC change.

D10 · 2026-11-04 · **File name template `{date} {time} {name}`** → `2026-11-04 1430 会议名.mp4`, as asked.
Illegal characters are stripped, collisions get ` (2)`.

D11 · 2026-11-04 · **Floating window hosts both the countdown and the recording bar.** One extra
always-on-top, transparent, frameless Tauri window labelled `floating`; the main window hides during
recording and comes back for the summary.

D12 · 2026-11-04 · **Settings persisted as JSON via tauri-plugin-store** (`settings.json` in app data),
versioned (`version: 1`), migrated by `settings::from_json`. Unknown fields are dropped, missing ones
defaulted, so old files never block startup.

D13 · 2026-11-04 · **Design first in Claude Design mode**, tokens exported to `design/tokens.css` and used
verbatim. Look named "Studio": graphite/off-white grounds, one indigo accent, red reserved for REC.

D14 · 2026-11-04 · **No GitHub remote yet.** Local commits only; creating the remote is a human decision
listed in `00-START-HERE.md`.

D15 · 2026-11-04 · **Takeover decisions in core.** (a) Manifest file is `<work_dir>/manifest.json`
(constant `MANIFEST_FILE`), matching ARCHITECTURE.md; `MANIFEST_SUFFIX` stays for tooling. (b) The
rounded/circle PiP mask is drawn by `geq` on ONE frame and looped, then merged with `alphamerge`;
per-frame `geq` measured 11.5 s CPU vs 0.5 s for 300 frames (ffmpeg 7.1.1, M1 Pro, lavfi, 2026-11-04).
(c) Windows `ddagrab` frames always go through `hwdownload,format=bgra` (CPU scale/overlay) in v0.1;
a GPU-resident NVENC path is future work. (d) macOS default devices use avfoundation's `default`
name; Windows requires an explicit dshow name, which the app crate resolves from the device list.
(e) Synthetic `lavfi` inputs run in real time only via `command::realtime_lavfi` (adds `-re`), so
`recording_command` stays a pure mapping. (f) `amix` uses `normalize=0` (needs ffmpeg ≥ 7.0; the
bundled LGPL build must be ≥ 7.0).

D16 · 2026-11-04 · **App crate design (src-tauri/src).** (a) `recorder.rs` is generic over an
`EventSink` so the same supervisor runs under Tauri (`TauriSink`) and headless (`--selftest`, tests).
(b) `start` returns only after the first `-progress` block arrives (≤ 12 s), so an immediate ffmpeg
failure is reported as `error.start` with the stderr tail instead of a silent "recording". (c) An
ffmpeg exit nobody asked for, with a camera in the plan and after the 3-second quick-failure window,
is treated as "camera lost": warn, drop the camera, open the next segment; a clean exit (exit 0, e.g.
an input ended) finishes the session; disk-full output finishes it early. (d) The window dance
(main hidden, floating shown) is driven by phase changes inside `TauriSink::state`, so tray and
shortcut starts behave exactly like the button. (e) The default display id "0" from the core
defaults is not an avfoundation screen index; `build_params` falls back to the first screen when the
stored id is not in the device list. (f) Screen size on macOS is probed with a 1-frame ffmpeg run at
start (needs the screen-recording permission, otherwise `error.permission`); the OS monitor size is
the fallback. (g) Settings use `tauri-plugin-store` (D12) but always pass through `settings::from_json`.

D18 · 2026-11-04 · **The UX follows Google Meet's "share screen" (Gabriel: "too complicated").** Home has a
self-view, a mic toggle, a camera toggle and one button; the button opens a screen picker with thumbnails
(`ScreenPicker.svelte`, modelled on Chrome's "choose what to share", including the "also share audio"
checkbox); while recording, a bar with status, time, pause and stop; afterwards a page with the video and
"show in Finder". Everything else (devices, PiP geometry, bitrate, shortcuts, recovery) moved to Settings.
The three-step permission wizard is gone: camera and mic are asked on load like a meeting page, screen
recording only when the user first wants to record, with a "reopen the app" button because macOS applies
that permission only after a restart (`relaunch_app`). Region capture left the UI (still in the contract).
`Settings.onboardingDone` stays in the schema, unused, so no migration is needed. Consequences in the
backend: `SystemAudioSource::Auto` no longer picks the experimental helper (a helper that never writes
would hang the FIFO and fail the start); a camera or mic without permission is dropped instead of killing
ffmpeg; a start that fails with a camera is retried once without it.

D19 · 2026-11-04 · **Follow the system's devices; signing in saves to Google Drive (Gabriel).** (a) Camera and
mic default to the system's current device (`AVCaptureDevice.default…` on macOS, re-read at each start); a
chosen device is stored by NAME because avfoundation indices shift. Windows has no such query through dshow:
the first device is used, untested. (b) Computer audio defaults to the ScreenCaptureKit helper, which records
the system mix on whatever output is active, so no BlackHole is needed; bundled through
`tauri.macos.conf.json`. This reverses the "Auto never picks the helper" part of D18 and the "experimental"
status in D7, on evidence: real capture through FIFO + ffmpeg, Glass.aiff at volume 0.3, max_volume −24.4 dB,
and silence still delivers samples, so ffmpeg does not stall (2026-11-04). A start that fails is retried
without the helper, then without the camera. (c) Sign-in: OAuth desktop flow with PKCE + loopback in the
user's browser, scope `drive.file` only (non-sensitive, and the app sees only what it created). Signed in =
every finished recording uploads to a folder `FlowGT Recorder` in the Drive home, resumable in 8 MiB chunks;
the local file stays. Upload verified against real Drive with a token from another client (721 210 bytes
stored, same size), into a throwaway folder since trashed. Sign-in itself is NOT verified: it needs an OAuth
client only a human can create (`.env.google`). (d) The refresh token lives in a 0600 file in the app config
directory, not the Keychain: an ad-hoc-signed build would raise a Keychain prompt after every rebuild; revisit
when the app is signed. (e) This is a deliberate change to invariant 12: the app talks to Google only when the
user has signed in, from Rust, so the webview CSP stays as it was.

D20 · 2026-11-04 · **VideoToolbox gets a 4x rate ceiling instead of 1.5x (amends D5).** On real screen capture the
1.5x `-maxrate` made VideoToolbox drop frames after each keyframe: a 0.2–0.33 s stall every 2 s (225/240
frames in 8 s). Tested variants: 4x ceiling 240/240, 605 kbps; no ceiling 239/240, 722 kbps; so 4x keeps a guard
against runaway sizes without stalls. The app, re-measured: 985/986 frames, 30 in every second, 292 MB/h. High
motion (testsrc2 + noise, lavfi): 300/300, 361 MB/h. Other encoders keep 1.5x (untested). `BitrateBudget`
still reports 1.5x, as the budget of record.

