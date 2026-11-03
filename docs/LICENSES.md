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

## Patents

HEVC and AAC are patent-encumbered formats. Encoding goes through the operating system's hardware
encoders; this is not legal advice. Check the position before any commercial distribution. Level D.

## Release checklist

- [ ] LGPL ffmpeg and ffprobe in `src-tauri/binaries/`, SHA256 recorded
- [ ] `ffmpeg -L` output saved in the release notes; configure line clean
- [ ] Source tarball for that exact build archived, location written here
- [ ] `THIRD-PARTY-NOTICES` generated and bundled
- [ ] LGPL text and ffmpeg notice shown in the About screen
