# `src-tauri/binaries/` — ffmpeg / ffprobe sidecars

Tauri bundles the files listed under `bundle.externalBin` in `tauri.conf.json`
(`binaries/ffmpeg`, `binaries/ffprobe`). Tauri expects one file per target
triple, named `<name>-<target-triple>`:

| Platform              | Files                                                              |
|-----------------------|--------------------------------------------------------------------|
| macOS Apple Silicon   | `ffmpeg-aarch64-apple-darwin`, `ffprobe-aarch64-apple-darwin`      |
| macOS Intel           | `ffmpeg-x86_64-apple-darwin`, `ffprobe-x86_64-apple-darwin`        |
| Windows x64           | `ffmpeg-x86_64-pc-windows-msvc.exe`, `ffprobe-x86_64-pc-windows-msvc.exe` |

Everything in this folder except this README and `.gitignore` is git-ignored.

## Development (this Mac)

```sh
bash scripts/fetch-ffmpeg.sh
```

copies the Homebrew `ffmpeg`/`ffprobe` (7.1.1, **GPL** build with libx264,
dynamically linked against `/opt/homebrew/lib`) into this folder so that
`npx tauri dev` finds the sidecars. These copies are for development only:
they are GPL-licensed and they will not run on a machine without Homebrew.

If the files are missing, the app falls back to `ffmpeg` on `PATH` and reports
`ffmpegBundled: false` in `get_app_info`.

## Release (LGPL requirement — see docs/DECISIONS.md D2 and docs/LICENSES.md)

FlowGT Recorder is MIT-licensed. Shipping a GPL ffmpeg would make the whole
distribution GPL, so release builds MUST bundle an **LGPL** ffmpeg:

**macOS: run `bash scripts/build-ffmpeg-mac.sh`.** It builds ffmpeg 7.1.1 from the ffmpeg.org source
(SHA-256 pinned in the script) as an LGPL build that links system libraries only, checks both points,
and writes `ffmpeg-NOTICE.txt` and `ffmpeg-COPYING.LGPLv2.1.txt` here for the bundle. The steps below
are for the other platforms.

1. Download an LGPL (not GPL) static build for each target, for example from
   a provider that publishes `--enable-shared`-free LGPL builds, such as the
   "lgpl" variants of BtbN's FFmpeg-Builds (`ffmpeg-n7.1-latest-<os>-lgpl-7.1`)
   or an LGPL build you compile yourself (`--disable-gpl --disable-nonfree`,
   no libx264/libx265).
2. Verify the published SHA-256 before use:
   `shasum -a 256 ffmpeg-*.tar.xz` / `certutil -hashfile ... SHA256`.
3. Extract `ffmpeg` and `ffprobe`, rename them to the triple-suffixed names
   above and place them here. On macOS strip the quarantine flag
   (`xattr -d com.apple.quarantine ffmpeg-aarch64-apple-darwin`) and make
   sure they are `chmod +x`.
4. Confirm the licence banner: `./ffmpeg-aarch64-apple-darwin -version` must
   print `configuration: ... --disable-gpl` or simply not list `--enable-gpl`
   / `--enable-libx264`. The app's encoder fallback chain ends in
   `libopenh264` / `mpeg4` for exactly this reason.
5. Ship the LGPL notice and the ffmpeg source offer with the app
   (`docs/LICENSES.md`).

Hardware encoders (`hevc_videotoolbox`, `h264_videotoolbox`, NVENC, QSV, AMF,
MediaFoundation) are all available in LGPL builds, so the 400 MB/hour budget
does not depend on libx264.
