# sck-record

Native macOS recorder for FlowGT Recorder (decision D27). One process records one segment:

- **screen** — ScreenCaptureKit, a display (`--display`), a window (`--window`) or an app (`--app`),
  scaled to `--size`, cursor optional, the app's own windows excluded by `--exclude-pid`;
- **system audio** — ScreenCaptureKit (`--system-audio`);
- **microphone** — AVCaptureSession (`--mic`, `--mic-device <name>`);
- **camera** — AVCaptureSession, drawn as a square picture-in-picture by CoreImage (`--camera`,
  `--camera-device <name>`, `--pip x,y,size,shape,mirror`);
- **file** — AVAssetWriter, fragmented MP4, HEVC or H.264 through VideoToolbox, AAC per track.
  Tracks: video, then system, then mic (whichever are on).

Frames go in on a clock at the output rate: each tick takes the latest screen frame and the latest
camera frame, so neither can stall the other and the audio never waits. The mic is written raw; the
app levels and mixes with ffmpeg when the recording is finished.

## Protocol

stdout, one JSON object per line:

```
{"event":"started","width":1920,"height":1080,"audioTracks":["system","mic"]}
{"event":"progress","seconds":1.5,"bytes":123456,"frames":45,"dropped":0}
{"event":"warning","code":"warn.cameraLost","detail":"..."}
{"event":"stopped","seconds":12.3,"bytes":..., "frames":..., "dropped":0}
{"event":"error","code":"permission|sourceNotFound|captureFailed|outputFailed|unsupportedOs","detail":"..."}
```

`--preview <file.jpg> [--preview-fps 8]` keeps a JPEG (480 px wide) of the latest output frame at that path, camera
square included, replaced atomically (`.tmp` + rename), so the app can show what is being recorded.

stdin: `stop` ends the recording; stdin closing (the app quit) does too; so do SIGINT/SIGTERM and
`--duration`. Diagnostics go to stderr as `sck-record: …` lines.

`--list` prints displays (id, index, pixel size, scale), windows (id, title, app, bundle id, pid, point and pixel
size), apps (bundle id, name, pid, and a 32 px PNG `icon` in base64 for apps that own a listed window), cameras and
microphones as JSON. `--check` prints `ok`, `permission-denied` or `unsupported-os`.

`--snapshot (--window <id> | --display <id>) --out <file.jpg> [--max-width 480]` writes one JPEG of a window or
display for the picker (SCScreenshotManager, macOS 14+). Run snapshots one at a time: concurrent clients stall the
capture daemon; the helper gives up after 4 s.

Exit codes: 0 ok · 1 usage · 2 permission · 3 unsupported OS · 4 source not found · 5 capture failed · 6 output failed.

## Build and test

```
swift build -c release      # .build/release/sck-record
swift test                  # SckRecordCore only; no capture framework is touched
```

`scripts/build-helper.sh` at the repository root builds it and copies it to `src-tauri/binaries/`.
