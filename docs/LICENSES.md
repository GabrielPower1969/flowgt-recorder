# Licenses

## This app

FlowGT Recorder is released under the **MIT License** (see `LICENSE`). Dependencies (Tauri, Svelte, Rust crates,
npm packages) keep their own licenses; before the first release, generate the full list with
`cargo about` / `license-checker` and ship it as `THIRD-PARTY-NOTICES`. Not done yet.

## ffmpeg

The app runs ffmpeg and ffprobe as **separate sidecar processes** (D2); it never links them.

- **Release builds must bundle an LGPL build of ffmpeg.** Confirm with `ffmpeg -L` (must print the GNU
  Lesser General Public License) and check the `configuration:` line: no `--enable-gpl`, no `--enable-nonfree`,
  no `--enable-libx264`, no `--enable-libx265`.
- **The Homebrew ffmpeg used in development is a GPL build (libx264 enabled). It must never ship.**
  The dev fallback is acceptable only because nothing is distributed.
- LGPL obligations when distributing the binary: ship the LGPL text and a notice naming ffmpeg and its
  version; make the exact corresponding source available (source tarball plus the configure line) or give a
  written offer; do not forbid replacing the ffmpeg binary (the sidecar layout already allows it).
- The decoder/encoder set we rely on is LGPL-clean: `hevc_videotoolbox`, `h264_videotoolbox`, native `aac`
  (Windows adds hardware encoders through their vendor runtimes, not through GPL code).
- Legal reference: https://ffmpeg.org/legal.html (level B until read in full and quoted; not yet done).

### Windows development build (W-2, 2026-11-11)

`scripts/fetch-ffmpeg.ps1` downloads one pinned build, checks its SHA-256, checks the licence and copies
`ffmpeg` and `ffprobe` into `src-tauri/binaries/`. Run on Windows 11 on 2026-11-11 (output in `docs/tasks/W-2.md`).

| | |
|---|---|
| Build | BtbN FFmpeg-Builds, LGPL static variant, 9.0 release branch: `ffmpeg-n9.0.1-11-ge47273f4d9-win64-lgpl-9.0.zip` (autobuild 2026-08-31-13-27) |
| Archive SHA-256 | `2484854ad6988d34560f4e6ea7a6ecb9dde0af7c229d2591815d056b04ec4f56` |
| `ffmpeg.exe` SHA-256 | `63a0b3c76a245bc0d986853612d9ec43a2a2d1f1c7a3fa40ee459c248075b3a6` |
| `ffprobe.exe` SHA-256 | `1ce64d9fdbfce857de2dd1f157c37eaa61c7501a356273dbcbe8b1674aef5879` |
| `ffmpeg -L` | "GNU Lesser General Public License … either version 3 of the License, or (at your option) any later version" |
| Configure line | no `--enable-gpl`, no `--enable-nonfree`, `--disable-libx264 --disable-libx265`; it does have `--enable-version3` |

- **This is LGPL version 3, not 2.1** (`--enable-version3`). It is fine for development and testing. Which
  licence variant a Windows release ships is not decided; that is Gabriel's call before W-7.
- The build has no `libx264`. Software H.264 falls back to `libopenh264`; on the test machine the app uses
  the Intel hardware encoders (`hevc_qsv`, `h264_qsv`).
- The release checklist below is still open for Windows: the source tarball for this exact build is not
  archived, and upstream removes old autobuild releases, so the pinned URL will stop working one day.

## Patents

HEVC and AAC are patent-encumbered formats. Encoding goes through the operating system's hardware
encoders; this is not legal advice. Check the position before any commercial distribution. Level D.

## Release checklist

- [ ] LGPL ffmpeg and ffprobe in `src-tauri/binaries/`, SHA256 recorded
- [ ] `ffmpeg -L` output saved in the release notes; configure line clean
- [ ] Source tarball for that exact build archived, location written here
- [ ] `THIRD-PARTY-NOTICES` generated and bundled
- [ ] LGPL text and ffmpeg notice shown in the About screen
