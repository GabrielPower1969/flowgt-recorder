# Windows verification checklist

First run on real hardware: **2026-11-11**, by the Windows owner's agent (Claude Fable 5.2) driving the dev
build (`npm run tauri dev`) through the WebView2 debugging port. A box is ticked only where a command or a
screenshot backs it; everything else carries the reason. Results and numbers: `docs/tasks/W-3.md`,
`docs/tasks/W-4.md`, `docs/BENCH.md` → Windows, report `docs/reports/02-Windows通宵开发-2026-11-11.md`.

Machine under test: Windows 11 Pro build 26300, GPU Intel integrated (Core Ultra 5 225, driver 32.0.101.9033),
3840×2160 at 150 % scaling, ffmpeg n9.0.1-11-ge47273f4d9 (BtbN LGPL static), date 2026-11-11.

## 1. Install

- [x] Windows 10 22H2 or 11; WebView2 runtime present (built into 11). WebView2 157.0.4341.42.
- [x] Node 20+, Rust stable (MSVC), Visual Studio Build Tools with the C++ workload. Node 26.13.0, rustc 1.99.1,
      Build Tools 2022 17.14 (MSVC 14.44).
- [x] LGPL ffmpeg and ffprobe built for Windows, copied to `src-tauri/binaries/` with the target triple
      suffix: `pwsh scripts/fetch-ffmpeg.ps1`. **Required, not optional**: the app crate does not compile
      without them.
- [x] `ffmpeg -hide_banner -encoders` lists `h264_qsv` and `hevc_qsv`; both encoded a real 1080p clip. The same
      list also names `*_nvenc` and `*_amf`, which cannot open on this machine: the list is not proof.

## 2. Start the app

- [x] `npm install` then `npm run tauri dev` opens the window. One warning in the log, expected:
      `encoder hevc_nvenc failed its test frame, trying the next one`.
- [x] Settings, Privacy: camera and microphone were already allowed for desktop apps (read from the registry,
      not changed).
- [ ] **Defect, open:** WebView2 shows its own prompts, "http://localhost:1420 wants to use your cameras" and
      then "…microphones". A person has to click Allow once (Gabriel did for the camera). A release build will
      show its own origin there. See W-4, item 5.

## 3. Device listing

- [x] `ffmpeg -list_devices true -f dshow -i dummy` shows the camera and the three microphones.
- [x] The app's device pickers show the same four names. No non-ASCII device name exists on this machine:
      **garbling is not tested**.
- [x] System audio: no "Stereo Mix" and no `virtual-audio-capturer` here, so the "Computer audio" picker is
      disabled and shows "Don't record". System audio cannot be recorded on this machine until W-5.

## 4. Record 2 minutes

- [x] Meeting name `win-verify`, camera on, mic on, one pause of about 10 s. Done twice: before and after the
      W-4 fixes.
- [x] The floating bar shows the time and Pause / Stop. **It shows no size.** Tray icon:
      **not checked** (no way to read the tray from the test driver).
- [x] Stop; the Summary screen appears; the session folder under `.flowgt-recorder/` is removed. The empty
      `.flowgt-recorder` folder itself stays and is visible in Explorer (a leading dot hides nothing on Windows).

## 5. Verify with ffprobe

After the W-4 fixes (`win-verify-2`, screen + camera + microphone, one pause):

```
> ffprobe -v error -show_entries format=duration,size,bit_rate:stream=codec_name,width,height,avg_frame_rate -of default=nw=1 "2026-11-11 0145 win-verify-2.mp4"
codec_name=hevc
width=1920
height=1080
avg_frame_rate=74972160/2514427
codec_name=aac
avg_frame_rate=0/0
duration=163.799674
size=18510859
bit_rate=904073
```

- [x] Duration 163.8 s (the driver was slow to stop; 120 s was the aim). 18,510,859 bytes = 388 MiB per hour,
      inside the 400 budget. 1 video +
      1 audio stream. Video 163.7 s and audio 163.7 s, no gap in the audio timeline.
- [x] Encoder `hevc_qsv`, 29.82 fps over the file. A full decode reports no error. **Not played by a person.**
- [x] CPU and memory during the first recording, 5 s samples: ffmpeg 14 % of one core (1.4 % of the machine),
      925 MB; the app process 0.8 % and 48 MB; its WebView2 processes 14 % and 836 MB. The 925 MB was the
      camera defect below; not measured again after the fix.

Defects found here and fixed in W-4: the app froze when the picker opened; the camera was dropped; with the
camera on, audio came out 9.5 % shorter than video; the recording bar was in the video.

## 6. Report

`docs/reports/02-Windows通宵开发-2026-11-11.md`; numbers in `docs/BENCH.md` → Windows.

## Device defaults on Windows (D19)

dshow cannot say which device is the system default, so the app takes the first camera and microphone it lists
and labels it "Same as system". On this machine the first microphone listed is also the Windows default (Fifine), so the
label was right, by coincidence of order. Computer audio needs
a loopback device ("Stereo Mix" or `virtual-audio-capturer`): there is no ScreenCaptureKit, and a WASAPI
loopback helper is W-5.
