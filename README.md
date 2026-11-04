# FlowGT Recorder

A small, local-only screen recorder for meetings and demos: **screen + camera picture-in-picture +
microphone (and, experimentally, system audio)**, encoded in hardware to **about 400 MB per hour of
HEVC**. Built with Tauri 2, a pure Rust core and Svelte 5, with ffmpeg as a sidecar. macOS and Windows.
Nothing is uploaded, ever — there is no network code in the app at all (see `docs/PRIVACY.md`).

> Status 2026-11-04: **v0.1, first build.** macOS is built and being tested on this machine; Windows
> exists as code and unit tests only. The honest status table is further down.

## The picture

```
┌──────────────────────────── FlowGT Recorder (Tauri 2) ────────────────────────────┐
│  Webview (Svelte 5 + TS)                 │  Rust app crate (src-tauri/src)         │
│  main window ─ home + screen picker /    │  commands.rs   IPC surface (types.ts)   │
│                settings / summary        │  recorder.rs   process supervisor:      │
│  floating window ─ countdown / rec bar   │                segments, pause, stop,   │
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
│  macOS: avfoundation screen+camera+mic · ScreenCaptureKit audio helper (Swift)    │
│  Windows: ddagrab (D3D11) → hw encoder · dshow camera/mic · loopback device       │
└────────────────────────────────────────────────────────────────────────────────────┘
```

One recording segment, inside ffmpeg:

```
screen ──► scale=W:-2 (+crop) ──┐
camera ──► crop=ih:ih → hflip? → scale=S:S → shape mask ─► overlay=x:y ─► HEVC/H.264 hw encoder
mic ─────┐                                                              -b:v -maxrate -bufsize
system ──┴► amix (mixed) | two tracks (separate) ────────────────────► AAC 64–128 kbps
                                                                        ─► fragmented MP4 segment
```

Pause closes the current segment and resume opens a new one; stop concatenates the segments with
`-c copy` (under two seconds). Because every segment is a fragmented MP4 and a manifest is rewritten
on every transition, a crash or `kill -9` loses at most the last fragment, and the next start offers to
finish the file. Details: `docs/ARCHITECTURE.md`, decisions D4 and D5 in `docs/DECISIONS.md`.

## Features

- **Screen + camera PiP + audio in one file.** Whole display or a region; the camera sits in a corner.
- **PiP controls.** Four corners, drag offset, size 10–50 % of the output height, square / rounded /
  circle, mirror on or off, live preview before recording.
- **Pause / resume** without a second ffmpeg process (segment boundary, D4).
- **Crash safety.** Fragmented MP4 segments + `manifest.json`; recovery on next start (D4).
- **Global shortcuts** `⌘⇧R` start / stop and `⌘⇧P` pause / resume on macOS (`Ctrl⇧R` / `Ctrl⇧P` on
  Windows), editable in Settings.
- **Tray icon** with start / pause / stop / show / quit.
- **3-2-1 countdown** (length configurable, 0 disables it).
- **Floating bar** while recording: elapsed time, bytes written vs. estimate, camera state, mic level,
  pause / stop. The main window hides itself during the recording (D11).
- **Size budget** instead of a quality slider: tell it 400 MB per hour and the bitrate follows (D5).
- **Compress to budget** on the summary page if a recording came out larger than planned.
- **Chinese and English** UI (auto-detected, switchable), **light and dark** (follows the system).
- **Keyboard access.** Every control is reachable with Tab, focus is visible, dialogs close on Esc.
- **File names you can read:** `{date} {time} {name}` → `2026-11-04 1430 Weekly sync.mp4` (D10).
- **Low-disk warning** before and during a recording; refuses to start under ten minutes of space.

## The size budget

```
┌─────────────────────────────────────────────────────────────────────────┐
│  400 MB per hour  ⇒  400 MB × 8 ÷ 3600 s ≈ 0.89 Mbps  =  889 kbps total │
│                                                                         │
│  audio   96 kbps (one AAC track)                                        │
│  video  889 − 96 = 793 kbps      -maxrate 1.5 × 793 ≈ 1190 kbps         │
│                                  -bufsize 2.0 × 793 ≈ 1586 kbps         │
│                                                                         │
│  Static slides cost far less than this under VBR; the cap keeps a       │
│  minute of fast motion from blowing the hour. (recorder-core::budget)   │
└─────────────────────────────────────────────────────────────────────────┘
```

Default profile (as designed; `settings::default_settings` is the source of truth — the integrator's
report `docs/reports/01-首版验收报告-2026-11-04.html` carries the measured numbers):

| Setting | Default | Why |
|---|---|---|
| Resolution | up to 1080p (`maxWidth` 1920; a 2880-wide Retina display is scaled to 1920, height follows, even) | HEVC at 793 kbps is comfortable at 1080p for screen content |
| Frame rate | 30 fps (24 / 60 selectable) | Meetings and slides; 60 only for demos |
| Video codec | HEVC, hardware (`hevc_videotoolbox` on macOS; NVENC / QSV / AMF / MF on Windows), falling back to H.264 hardware → `libopenh264` → `mpeg4` | Hardware honours `-b:v`; software x264 is GPL and not shipped |
| Audio | AAC 96 kbps per track, mic + system **mixed** into one track (**separate** = two tracks for later transcription) | 64 / 96 / 128 selectable |
| Container | MP4, fragmented during capture, `+faststart` after concat | Crash safety |
| Target | 400 MB per hour | The product promise |

## Requirements

| | macOS | Windows |
|---|---|---|
| OS | 13 Ventura or newer (`minimumSystemVersion` 13.0), Apple Silicon or Intel | 10 / 11 with **WebView2** (the installer downloads the bootstrapper if missing) |
| Encoder | VideoToolbox (built in) | NVIDIA / Intel / AMD hardware encoder or Media Foundation; software fallback otherwise |
| ffmpeg | Bundled **LGPL** build of ffmpeg + ffprobe as a Tauri sidecar in release; in development the ffmpeg on your `PATH` (Homebrew) is used | same; `src-tauri/binaries/ffmpeg-<target-triple>.exe` |
| Permissions | Screen Recording, Camera, Microphone (see below) | Camera and Microphone privacy toggles; screen capture is not gated |

Developer toolchain (what this repository was built with on 2026-11-04): Rust ≥ 1.80 (`rust-version`
in `src-tauri/Cargo.toml`; built here with 1.99.1), Node 23 / npm 11, Tauri CLI 2.12, Xcode command-line
tools on macOS. Windows builds additionally need the Visual Studio C++ build tools.

## Quick start

```bash
npm install                      # frontend deps + Tauri CLI
bash scripts/fetch-ffmpeg.sh     # LGPL ffmpeg/ffprobe → src-tauri/binaries/<name>-<target-triple>
npm run tauri dev                # Vite on :1420 + cargo run; first start triggers the permission prompts
```

The first run opens the onboarding wizard: ffmpeg check → Screen Recording → Camera → Microphone →
output folder. On macOS the Screen Recording toggle only takes effect after the app is restarted; the
wizard says so. The human-only steps (clicking system prompts, a real two-minute recording) are listed
in `docs/MANUAL-CHECKLIST.md`.

Other commands:

```bash
npm run tauri build              # .app / .dmg on macOS, .msi / .exe on Windows (unsigned for now)
npm run check                    # svelte-check against tsconfig.json
./src-tauri/target/debug/flowgt-recorder --selftest 5   # 5-second end-to-end self test, prints the probe
```

## Testing

```bash
npm test                         # Vitest: unit + component tests (jsdom, Testing Library)
npm run core:test                # cargo test --workspace: the pure core, runs on any OS
bash scripts/integration.sh      # real ffmpeg: lavfi sources → fragmented MP4 → concat → ffprobe + pixel check
bash scripts/bench.sh            # CPU / MB-per-hour / encoder speed; --app --seconds 60 measures a live recording
```

What each layer proves and what it cannot is in `docs/TESTING.md`. Rule: `npm run check && npm test &&
npm run core:test` green before every commit.

## Permissions

The app records your screen, camera and microphone, so the operating system asks you first. It asks
once per permission; the wizard shows the state of each (granted / denied / not determined) and a button
that opens the right System Settings pane.

**macOS** — three TCC prompts, with these texts (from `src-tauri/Info.plist`):

| Prompt | Text shown by macOS | Why the app needs it |
|---|---|---|
| Screen Recording | "FlowGT Recorder records your screen locally. Nothing is uploaded." | ffmpeg's avfoundation input captures the display; without it the capture is black or fails |
| Camera | "FlowGT Recorder shows your camera as a picture-in-picture in your recordings." | the PiP preview (`getUserMedia`) and the ffmpeg camera input |
| Microphone | "FlowGT Recorder records your voice together with the screen." | the level meter and the ffmpeg mic input |
| System audio (optional) | "FlowGT Recorder records the meeting's system audio locally." | only when the ScreenCaptureKit helper is used; covered by the Screen Recording toggle (macOS 15 names the pane "Screen & System Audio Recording") |

Screen Recording is **not** granted by clicking the prompt: macOS sends you to System Settings →
Privacy & Security → Screen Recording, where you switch the app on and relaunch it. In development
(`npm run tauri dev`) the entry may appear under the name of the terminal that launched it (Terminal,
iTerm, Claude, VS Code) rather than "FlowGT Recorder" — grant whichever appears; the checklist explains.

**Windows** — Settings → Privacy & security → Camera / Microphone must allow desktop apps. Screen
capture is not gated (`PermissionState::NotApplicable`).

## Status (honest)

| Area | State on 2026-11-04 | Evidence |
|---|---|---|
| macOS capture (avfoundation screen + camera + mic, VideoToolbox HEVC) | **built here**; tested on this Mac by the integrator | `docs/reports/01-首版验收报告-2026-11-04.html` (integrator) |
| Pause / resume, segments, concat | implemented | `npm run core:test`, `scripts/integration.sh` |
| Crash recovery (`kill -9` → recover on next start) | implemented; integration script covers the kill case, the in-app recovery flow is a manual item | `docs/MANUAL-CHECKLIST.md` (d) |
| Size budget, compress-to-budget | implemented | `docs/BENCH.md` for measured MB/h |
| System audio, macOS | **experimental**: ScreenCaptureKit helper (`helpers/sck-audio`), BlackHole loopback as fallback, off otherwise | `docs/SYSTEM-AUDIO.md`; final choice pending (D7) |
| **Windows** (ddagrab + dshow + loopback device) | **code + unit tests only — never run on Windows hardware** | `docs/WINDOWS-VERIFY.md` is the test plan |
| Window capture (single window) | **not yet**; needs the native ScreenCaptureKit / WGC path (D9) | option shown disabled in the UI |
| Packaging: signing, notarisation, auto-update | not yet | no updater plugin in `src-tauri/Cargo.toml` |
| Release ffmpeg (LGPL, static) | fetched by `scripts/fetch-ffmpeg.sh`; a reproducible build with its source archive is a next step | `docs/LICENSES.md`, `00-START-HERE.md` |
| GitHub remote | none; local commits only (D14) | human decision |

## Project layout

```
screen-recorder/
├── README.md · CLAUDE.md · HOURS.md · 00-START-HERE.md · LICENSE
├── package.json · vite.config.ts · svelte.config.js · tsconfig.json · index.html
├── src/                        Svelte 5 frontend
│   ├── main.ts · App.svelte
│   ├── routes/                 Onboarding · Main · Settings · Summary · Floating (one per screen)
│   └── lib/
│       ├── types.ts            IPC contract — mirrors crates/core/src/types.rs 1:1
│       ├── ipc.ts              typed invoke/listen wrapper + mock for vitest and plain-browser dev
│       ├── i18n/               zh · en string tables (all UI text goes through here)
│       ├── stores/             settings, recorder state, devices
│       └── components/         StagePreview · LevelMeter · ShortcutField · Toast …
├── src-tauri/
│   ├── tauri.conf.json         windows, CSP, tray, sidecars (externalBin), bundle
│   ├── Cargo.toml              app crate + workspace
│   ├── Info.plist · Entitlements.plist · capabilities/default.json
│   ├── binaries/               ffmpeg-<triple>, ffprobe-<triple>  (fetched, not committed)
│   ├── icons/
│   ├── src/                    lib.rs · commands.rs · recorder.rs · sidecar.rs · permissions.rs · tray.rs · shortcuts.rs · disk.rs
│   └── crates/core/            recorder-core: pure logic + fixtures/ from real machines
├── helpers/sck-audio/          Swift ScreenCaptureKit audio → PCM helper (macOS)
├── design/tokens.css           design tokens, used verbatim (D13)
├── scripts/                    fetch-ffmpeg.sh · integration.sh · bench.sh
├── tests/                      integration fixtures and expectations
├── docs/                       see index below; reports/ holds numbered acceptance reports
└── .github/workflows/ci.yml    cargo test + vitest + clippy + svelte-check
```

## Documentation

| File | What it is |
|---|---|
| `00-START-HERE.md` | Where the project is, what comes next (Chinese) |
| `docs/ARCHITECTURE.md` | Modules, pipeline, IPC, Windows path — the source of truth |
| `docs/DECISIONS.md` | D1–D14 and why |
| `docs/DESIGN.md` · `design/tokens.css` | The "Studio" look and its tokens |
| `docs/UX.md` | User journey, UX checklist per screen, shortcuts, accessibility (Chinese) |
| `docs/TESTING.md` | The test pyramid: what each layer proves |
| `docs/BENCH.md` | Measured CPU and MB/h |
| `docs/WINDOWS-VERIFY.md` | What to run on a real Windows machine |
| `docs/SYSTEM-AUDIO.md` | ScreenCaptureKit helper vs BlackHole |
| `docs/MANUAL-CHECKLIST.md` | Everything only a human can do (Chinese) |
| `docs/PRIVACY.md` | Local-only guarantee, permissions, where files live (bilingual) |
| `docs/LICENSES.md` | MIT for the app, LGPL obligations for the bundled ffmpeg, third-party notices |
| `docs/reports/` | Numbered reports, newest number = latest |

## License

The application is **MIT** (`LICENSE`, © 2026 Gabriel Chen / FlowGT). Release builds bundle **ffmpeg
and ffprobe under the LGPL v2.1 or later**; the notice, the exact source and configure line that
correspond to the shipped binaries, and the reasons the Homebrew (GPL) build must never ship are in
`docs/LICENSES.md`. Tauri is MIT / Apache-2.0, Svelte is MIT, the Inter and JetBrains Mono fonts are
under the SIL Open Font License 1.1.

---

## 中文速览

**这是什么。** 一个只在本机工作的录屏软件：屏幕 ＋ 摄像头小窗（PiP）＋ 麦克风（系统声音为实验功能），
硬件 HEVC 编码，一小时约 400 MB。Tauri 2 ＋ Rust 核心 ＋ Svelte 5，ffmpeg 作为旁车进程。macOS 与
Windows。**没有任何网络代码，什么都不上传。**

**为什么是 400 MB。** 400 MB × 8 ÷ 3600 s ≈ 0.89 Mbps，总码率 889 kbps；音频 96 kbps，视频拿剩下的
793 kbps，`-maxrate` 1.5 倍、`-bufsize` 2 倍。静态画面在 VBR 下几乎不花钱，上限只防止一分钟剧烈运动把
一小时撑爆。这个数字是产品承诺，不是画质滑块（决定 D5）。

**怎么跑。**

```bash
npm install
bash scripts/fetch-ffmpeg.sh     # 下载 LGPL 版 ffmpeg/ffprobe 到 src-tauri/binaries/
npm run tauri dev                # 第一次启动会弹三个系统权限对话框
```

快捷键：`⌘⇧R` 开始／停止，`⌘⇧P` 暂停／继续。暂停＝切一段，停止＝拼接，`kill -9` 也只丢最后一个片段。

**状态表（2026-11-04）。**

| 项 | 状态 |
|---|---|
| macOS 录制 | 已构建，本机验收中（见 `docs/reports/01-首版验收报告-2026-11-04.html`） |
| Windows | 只有代码和单元测试，**没在真机跑过** |
| 系统声音 | 实验性 ScreenCaptureKit 助手，备选 BlackHole |
| 窗口捕获 | 还没有（D9） |
| 签名／公证／自动更新 | 还没有 |

**下一步。** 见 `00-START-HERE.md`：① 本机点权限、录 2 分钟真会议 → ② Windows 真机 → ③ 原生
ScreenCaptureKit 视频路径 → ④ 系统声音方案定案 → ⑤ 是否建 GitHub 远程 → ⑥ LGPL ffmpeg 可复现构建 →
⑦ 窗口捕获。人工才能做的事都在 `docs/MANUAL-CHECKLIST.md`。
