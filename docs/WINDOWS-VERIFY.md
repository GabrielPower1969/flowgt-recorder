# Windows verification checklist

**Nothing in this repository has ever run on Windows.** The Windows command paths (`ddagrab`, `dshow`,
loopback device) are covered by unit tests with fixture output on macOS only (D3, D8). Every box below is
unchecked until a person ticks it on real hardware.

Machine under test: Windows version ____ , GPU ____ , ffmpeg build ____ , date ____

## 1. Install

- [ ] Windows 10 22H2 or 11; WebView2 runtime present (built into 11).
- [ ] Node 20+, Rust stable (MSVC), Visual Studio Build Tools with the C++ workload.
- [ ] LGPL ffmpeg and ffprobe built for Windows, copied to `src-tauri/binaries/` with the target triple
      suffix (for example `ffmpeg-x86_64-pc-windows-msvc.exe`), or on PATH for dev.
- [ ] `ffmpeg -hide_banner -encoders` lists `h264_nvenc`, `h264_qsv` or `h264_amf`, or `hevc_*` equivalents.

## 2. Start the app

- [ ] `npm install` then `npm run tauri dev` opens the window; note any red console output.
- [ ] Settings, Privacy: allow camera and microphone for desktop apps.

## 3. Device listing

- [ ] `ffmpeg -list_devices true -f dshow -i dummy` shows your camera and microphone.
- [ ] The app's device pickers show the same names (not garbled; non-ASCII names are the risk).
- [ ] If system audio is wanted: "Stereo Mix" or `virtual-audio-capturer` appears (see D8).

## 4. Record 2 minutes

- [ ] Meeting name `win-verify`, camera on, mic on, pause once for 10 s.
- [ ] The floating bar shows time and size; the tray icon changes.
- [ ] Stop; the Summary screen appears; the work folder `.flowgt-recorder/<session>/` is cleaned up.

## 5. Verify with ffprobe

```bash
ffprobe -v error -show_entries format=duration,size,bit_rate:stream=codec_name,width,height,avg_frame_rate -of default=nw=1 "win-verify.mp4"
```

- [ ] Duration about 120 s; size at most about 14 MB; 1 video + 1 audio stream.
- [ ] Encoder and frame rate recorded; dropped frames noted; play the file end to end.
- [ ] Task Manager CPU and memory of the app and ffmpeg during the recording: ____ / ____

## 6. Report

Write results to `docs/reports/02-Windows实测-<date>.html` and update `BENCH.md`.
