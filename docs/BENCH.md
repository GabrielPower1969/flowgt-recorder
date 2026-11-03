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
