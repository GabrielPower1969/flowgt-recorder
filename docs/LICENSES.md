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
- **macOS (2026-11-12):** `scripts/build-ffmpeg-mac.sh` builds ffmpeg 7.1.1 from
  `https://ffmpeg.org/releases/ffmpeg-7.1.1.tar.xz` (SHA-256
  `733984395e0dbbe5c046abda2dc49a5544e7e0e1e2366bba849222ae9e3a03b1`; the release signature was checked by
  hand against the FFmpeg release key `FCF9 86EA 15E6 E293 A564 4F10 B432 2F04 D676 58D8`). No external
  libraries, no network protocols. `scripts/build-mas.sh` puts the notice (source URL, hash, configure line)
  and the LGPL 2.1 text in `Contents/Resources/licenses/`. Not done: showing them in the About screen.
- Legal reference: https://ffmpeg.org/legal.html (level B until read in full and quoted; not yet done).

## Patents

HEVC and AAC are patent-encumbered formats. Encoding goes through the operating system's hardware
encoders; this is not legal advice. Check the position before any commercial distribution. Level D.

## Release checklist

- [ ] LGPL ffmpeg and ffprobe in `src-tauri/binaries/`, SHA256 recorded
- [ ] `ffmpeg -L` output saved in the release notes; configure line clean
- [ ] Source tarball for that exact build archived, location written here
- [ ] `THIRD-PARTY-NOTICES` generated and bundled
- [ ] LGPL text and ffmpeg notice shown in the About screen
