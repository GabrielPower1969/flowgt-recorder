# Benchmarks

Source levels: **A** = command output captured by the author on the stated date; **B** = official page,
summarised; **C** = third party; **D** = inference. Every row below is level A unless stated.

> **These numbers come from pipeline mode (lavfi synthetic inputs), not from real screen capture.**
> `testsrc2` is a synthetic pattern, a webcam is faked with a red `color` source, the mic is a sine tone.
> They prove the encoder, the PiP graph, the segment/concat path and the bitrate budget work. They say
> nothing about avfoundation capture cost, real-content bitrate or Retina scaling.

Machine: Apple M1 Pro, macOS 27.0.1, ffmpeg 7.1.1 (Homebrew, GPL build), Rust 1.99.1, Node 23.9. Date: 2026-11-04.

## Test suites

| Check | Result | Level / date |
|---|---|---|
| `cargo test --workspace` (src-tauri) | recorder-core 112 passed, flowgt-recorder 19 passed, 0 failed | A · 2026-11-04 |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean | A · 2026-11-04 |

## End-to-end self test

`./src-tauri/target/debug/flowgt-recorder --selftest 5 --pause-at 2 --resume-at 4`
(lavfi `testsrc2` 1920x1080@30 + red camera PiP + sine mic, `hevc_videotoolbox`; two segments, concat).

| Metric | Value | Level / date |
|---|---|---|
| File size | 688,844 bytes | A · 2026-11-04 |
| Duration | 6121 ms (5 s requested plus ffmpeg startup and segment seam) | A · 2026-11-04 |
| Video | 1920x1080, 29.9 fps, hevc | A · 2026-11-04 |
| Audio | aac, 1 track | A · 2026-11-04 |
| Measured bitrate | 900.25 kbps | A · 2026-11-04 |
| Budget (400 MB/h) | total 889 / video 793 / maxrate 1190 / bufsize 1586 kbps | A · 2026-11-04 |
| Over budget | ratio 0.0128 (1.3 %) | A · 2026-11-04 |
| Wall clock | 9.9 s for the 5 s run | A · 2026-11-04 |
| CPU | user 2.8 s for the whole process tree including ffmpeg | A · 2026-11-04 |

Caveat (D): `testsrc2` is easy to encode and the budget is a ceiling, so 900 kbps here does not predict MB/h
on real screen content.

## PiP mask micro-benchmark (decision D15b)

300 frames, 264 px PiP over 1080p lavfi, output `-f null`.

| Variant | User CPU | Level / date |
|---|---|---|
| No PiP | 0.16 s | A · 2026-11-04 |
| Square PiP | 0.51 s | A · 2026-11-04 |
| Per-frame `geq` alpha mask | 11.47 s | A · 2026-11-04 |
| One-frame `geq`, looped, `alphamerge` (chosen) | 0.53 s | A · 2026-11-04 |

## Not yet measured

- Real screen capture CPU and RSS (needs a human: `bash scripts/bench.sh --app --seconds 60`).
- MB/h on real content (static slides vs scrolling video vs a call with faces).
- Anything on Windows (nothing has run on Windows; see `WINDOWS-VERIFY.md`).
- SSIM / PSNR against a lossless reference.
- Crash-recovery test (`kill -9` while recording); the script is not written yet.
- Retina scaling cost, multi-display, 4K sources.
- Battery and thermal behaviour over a one-hour recording.

## Real capture through the app (2026-11-04, source A: command output)

Apple M1 Pro, macOS 27.0.1. The app was started from a shell that already holds the screen-recording
permission, so the camera was not allowed and **no PiP was in these runs**. Screen 3840×2160 → 1920×1080,
real microphone + real system audio, hevc_videotoolbox.

| Measure | Result |
|---|---|
| Frames, 32.9 s recording | 985 of 986, 30 in every second, largest gap 0.055 s |
| Size on real desktop content (mostly static) | 648 kbps = 292 MB/h, budget 400 |
| CPU, app + ffmpeg + helper, 11 samples at 2.5 s | 12–26 % of one core |
| Memory, same | 308–326 MB, above the 300 MB target |
| Pause, resume, stop, concat into one playable file | works (earlier 38 s run with one pause) |
| Peak audio level, mic + system mixed | −14 dB |

## Rate ceiling on VideoToolbox (D20)

| Real capture, 8 s | Frames / 240 | Largest gap | kbps |
|---|---|---|---|
| `-maxrate` 1.5×, `-bufsize` 2× (old) | 225 | 0.333 s | 553 |
| `-maxrate` 4×, `-bufsize` 4× (chosen) | 240 | 0.033 s | 605 |
| no ceiling | 239 | 0.033 s | 722 |

| High motion, lavfi `testsrc2` + noise, 10 s | Frames / 300 | MB/h |
|---|---|---|
| 1.5× | 300 | 357 |
| 4× (chosen) | 300 | 361 |
| no ceiling | 300 | 399 |

## Other real checks

- System audio through the helper + FIFO + ffmpeg, Glass.aiff at volume 0.3: max_volume −24.4 dB; silence still delivers samples.
- Drive resumable upload to real Google Drive with a token from another OAuth client: 721 210 bytes stored, same size locally.

## Still not measured

Camera + PiP on real capture (needs the camera permission on the Desktop app) · Google sign-in (needs an OAuth client) · a full hour · Windows.

