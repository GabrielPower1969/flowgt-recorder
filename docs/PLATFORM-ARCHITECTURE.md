# Platform architecture: one shell, one core, one capture engine per platform

Written 2026-11-10. Status: **proposal (candidate D30), waiting for Gabriel's OK.** `docs/ARCHITECTURE.md`
describes what exists today; this file describes the shape every platform grows into. Where the two
disagree, `ARCHITECTURE.md` is the present and this file is the target.

Evidence labels: **A** = read on the publisher's own site on 2026-11-10, **D** = inference, not yet verified.

## 1. The shape

```
┌────────────────────────────── every platform ──────────────────────────────┐
│  UI            Svelte 5 + TypeScript in the Tauri webview   src/            │
│  App shell     Tauri 2 (Rust): IPC, windows, tray, storage   src-tauri/src/ │
│  Core          recorder-core (Rust, pure, no I/O)            crates/core/   │
├──────────────────────────── the engine contract ───────────────────────────┤
│  start(options) · pause · resume · stop · list sources · snapshot           │
│  events: started · progress · warning · stopped · error                     │
├───────────────┬───────────────┬────────────────────┬───────────────────────┤
│ macOS         │ Windows       │ iOS (future)       │ Android (future)      │
│ sck-record    │ win-record    │ Tauri plugin       │ Tauri plugin          │
│ Swift         │ Rust          │ Swift              │ Kotlin                │
│ child process │ child process │ in-process +       │ in-process +          │
│               │               │ broadcast extension│ foreground service    │
└───────────────┴───────────────┴────────────────────┴───────────────────────┘
```

Three layers are shared by every platform. Only the bottom row is written once per platform.

**The rule that makes this work: the capture engine owns capture, clock, encoding and the segment file.**
The shell never touches frames or samples. It sends options, receives events, and gets a finished
fragmented-MP4 segment per recording span. This is what macOS already does since D27.

## 2. The engine contract

The contract already exists for macOS in `src-tauri/crates/core/src/native.rs`:

| Part | Where it is defined today |
|---|---|
| Options (source, size, fps, codec, bitrates, crop, mic, camera, picture-in-picture, cursor, preview path) | `helper_args` |
| Sources | `NativeSource`: display, window, app |
| Events | `NativeEvent`: `started`, `progress`, `warning`, `stopped`, `error` (one JSON object per line) |
| Source list for the picker | `parse_window_list` / `WindowInfo` |
| Error codes | `error_code` |

Two transports carry the same contract:

- **Process transport (desktop).** The engine is an executable. Options are command-line arguments, events
  are JSON lines on stdout, diagnostics go to stderr, `stop` arrives on stdin. `recorder.rs` supervises it.
- **Plugin transport (mobile).** The operating system does not let an app start child processes, so the
  engine is a Tauri plugin in Swift or Kotlin. The same options travel as a JSON object, the same events as
  Tauri events. `D`: the supervisor in `recorder.rs` needs a small trait (`CaptureEngine`) so both transports
  plug into it; that refactor is not done.

A new engine is correct when it passes the **engine conformance test** (to be written, task S-1 in
`docs/tasks/`): feed it a fixed option set, check the event sequence and probe the resulting file.

## 3. Platform presets

| | macOS (done) | Windows (preset) | iOS (future) | Android (future) |
|---|---|---|---|---|
| Screen / window | ScreenCaptureKit | Windows.Graphics.Capture | ReplayKit broadcast extension | MediaProjection |
| System audio | ScreenCaptureKit | WASAPI loopback (per-process loopback where the OS has it) | from the broadcast extension | AudioPlaybackCapture |
| Mic / camera | AVFoundation | Media Foundation | AVFoundation | CameraX / AudioRecord |
| Encoder + file | AVAssetWriter | Media Foundation sink writer, hardware HEVC/H.264 | AVAssetWriter | MediaCodec + MediaMuxer |
| Engine language | Swift | Rust (`windows` crate) | Swift | Kotlin |
| ffmpeg | join + mix at the end | join + mix at the end | none | none |

Evidence for the Windows column:
- Windows.Graphics.Capture "provides APIs to acquire frames from a display or application window" — **A**,
  learn.microsoft.com, Screen capture.
- Per-process loopback: Microsoft's ApplicationLoopback sample captures the audio of one process tree and
  "requires Windows 10 build 20348 or later" — **A**, learn.microsoft.com. Whole-endpoint loopback is older
  and works everywhere.
- Choosing Rust for the Windows engine (one toolchain with the shell, no C++ build) — **D**, to be confirmed
  by a spike (task W-6).

Mobile rows are **D** except: Android needs a `mediaProjection` foreground service and user consent before
each session — **A**, developer.android.com. iOS ReplayKit details were not re-read today.

### Windows path in two steps

1. **Now:** keep the ffmpeg path (`ddagrab` + `dshow`) to get a first real recording and numbers
   (`docs/WINDOWS-VERIFY.md`). It is already written and unit-tested.
2. **Then:** build `helpers/win-record/` to the engine contract. It brings what ffmpeg cannot do well on
   Windows: one clock for all tracks, one-window capture, system audio without a virtual device.

## 4. Why not Qt

Considered on 2026-11-10 because of future iOS and Android versions.

| Question | Answer |
|---|---|
| Does Qt remove the per-platform capture work? | No. Qt's `QScreenCapture` "is not supported on mobile operating systems, except on Android" and is "only supported with the FFmpeg backend" — **A**, doc.qt.io/qt-6/qscreencapture.html. iOS screen capture has to be native in any toolkit. |
| Does it fit an MIT project? | It can, with care. Qt's open-source option is LGPL v3; Qt says static linking may make the application "subject to LGPL" and recommends dynamic linking — **A**, qt.io LGPL obligations page. An MIT app can meet that, but every release then carries LGPL duties for Qt on top of those for ffmpeg. |
| What would switching cost? | A rewrite of the working UI (Svelte), the shell (Tauri/Rust) and their tests — **D**. |
| Does the current stack reach mobile? | Yes. Tauri 2 is "for all major desktop and mobile platforms" with backend logic in "Rust, Swift, and Kotlin" — **A**, v2.tauri.app. |

Decision proposed: **stay on Tauri 2 + Rust core + Svelte; put platform work in engines.** Revisit only if
a Tauri mobile spike fails (task M-1, not scheduled).

## 5. What mobile changes later

Known constraints to respect now, so nothing has to be undone:

1. **No ffmpeg on mobile.** Engines must produce a finished, playable segment on their own. Keep the final
   join simple (`-c copy`) so a native join can replace it.
2. **No floating window, no tray, no global shortcuts on mobile.** Recording controls live in the UI layer
   behind capability flags (see section 6), never assumed.
3. **Storage and permissions differ.** Paths come from the shell, never from the UI or core.
4. **iOS records the screen from a separate extension process with tight memory.** The iOS engine will be
   two pieces: the extension that captures and writes, and the plugin that controls it. **D**.

## 6. Capability flags

The shell reports what the running platform can do; the UI shows or hides controls from that alone.

```
capabilities = { windowCapture, systemAudio, perAppAudio, camera, pictureInPicture,
                 floatingBar, tray, globalShortcuts, pauseResume, livePreview }
```

No `if (platform === ...)` in components. A platform gains a feature by turning a flag on in its engine.
`D`: the flags type is not in `types.rs` / `types.ts` yet (task S-2).

## 7. Where new code goes

| You are adding… | It goes in… |
|---|---|
| a rule, a calculation, a parser, an argument builder | `src-tauri/crates/core/` (pure, tested on any OS) |
| anything that touches the OS, a file, a process | `src-tauri/src/` |
| capture, encoding, device access | the platform's engine under `helpers/` (desktop) or a plugin (mobile) |
| anything the user sees | `src/`, strings in both `i18n` tables |
| a new option or event of the contract | `native.rs` + every engine + the conformance test, in a `shared/…` pull request |
