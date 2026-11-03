# Testing

How FlowGT Recorder is tested, layer by layer: the command, what a green run **proves**, what it
**cannot** prove, and where the results go. Written 2026-11-04 during the first build; the scripts
named here are owned by the `scripts` agent (`docs/ARCHITECTURE.md` → module ownership) and this page
describes their contract. If a script and this page disagree, fix one of them in the same commit.

## The pyramid

```
                 ┌──────────────┐
                 │   manual     │  docs/MANUAL-CHECKLIST.md — TCC, real devices, Windows, feel
                 ├──────────────┤
                 │  soak 30 min │  optional, before a release
                 ├──────────────┤
                 │  crash       │  kill -9 → recover
                 ├──────────────┤
                 │  performance │  scripts/bench.sh
                 ├──────────────┤
                 │  integration │  scripts/integration.sh — real ffmpeg, synthetic inputs
                 ├──────────────┤
                 │  component   │  Testing Library + jsdom, through the ipc mock
                 ├──────────────┤
                 │  unit        │  cargo test (recorder-core) · vitest (src/lib)
                 └──────────────┘
          cheap, fast, runs anywhere ──────────────► slow, needs hardware and a human
```

| Layer | Command | Proves | Cannot prove | Results go to |
|---|---|---|---|---|
| **Unit · Rust core** | `npm run core:test` (= `cargo test --manifest-path src-tauri/Cargo.toml --workspace`) | budget arithmetic (`400 → 889 kbps`), the exact ffmpeg argv for every platform and option, parsers against real captured output in `crates/core/fixtures/`, encoder fallback chain, PiP geometry and filter strings, file-name rendering and sanitising, disk arithmetic, manifest round-trips and `recovery::decide`, settings defaults / validation / migration | that ffmpeg accepts the argv, that a device exists, anything about the OS, timing, permissions | terminal; CI job `core` |
| **Unit · TypeScript** | `npm test` (= `vitest run`) | stores, formatting helpers, i18n key parity between `zh` and `en`, the `ipc.ts` mock honouring the `Commands`/`Events` types | real IPC, Tauri window behaviour | terminal; `npm run test:ui` writes `coverage/` |
| **Component** | `npm test` (Testing Library, `jsdom`, `src/test-setup.ts`) | each screen renders from mock state, buttons call the right command, keyboard paths (Tab order, Esc, Enter/Space), ARIA roles and labels, both languages, both themes | WKWebView / WebView2 rendering differences, real fonts and layout metrics, transparency of the floating window | same |
| **Types & lint** | `npm run check` · `cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --all-targets -- -D warnings` · `cargo fmt --all -- --check` | the two halves of the contract agree with the code that uses them; no lint debt | semantics | terminal; CI |
| **Integration** | `bash scripts/integration.sh` | the real ffmpeg on this machine accepts what `command.rs` builds: `testsrc2` + `sine` lavfi inputs → fragmented MP4 segment(s) → concat → `ffprobe` reports the expected codec, size, duration, track count → a pixel check finds the PiP block where `pip::layout` put it → a `kill -9` mid-segment still leaves a playable file that `recovery::decide` turns into a concat | real capture devices, TCC, the hardware encoder on another machine, Windows (no ddagrab/dshow on a Mac — Windows argv is unit-tested only) | `tests/out/` (git-ignored) + a printed summary; CI job on macOS runner if ffmpeg is installed |
| **Performance** | `bash scripts/bench.sh` · `bash scripts/bench.sh --app --seconds 60` | encoder speed ≥ 1.0× real time, CPU % of ffmpeg (and of the app with `--app`), MB/h on static vs. motion inputs, estimate vs. actual size | subjective smoothness, thermal behaviour over an hour | `bench-out/` (git-ignored); numbers are copied into `docs/BENCH.md` |
| **Crash** | integration script's kill case + `docs/MANUAL-CHECKLIST.md` (d) | a `kill -9` of ffmpeg or of the app leaves segments + manifest that recover into a playable file; the in-app Settings → Recover flow works | — | report under `docs/reports/` |
| **Soak** (optional) | a 30-minute real recording, then `ffprobe` + Activity Monitor | no memory growth, no A/V drift, size on budget over a long run | — | `docs/BENCH.md` soak row |
| **Manual** | `docs/MANUAL-CHECKLIST.md` | permissions, real devices, unplugging a camera, Windows hardware, the feel of countdown / floating bar / summary | — | `docs/reports/NN-…` |

## The rule: all green before commit

```bash
npm run check && npm test && npm run core:test
```

must pass before every commit. Add

- `cargo clippy … -D warnings` for any Rust change;
- `bash scripts/integration.sh` for any change to `command.rs`, `pip.rs`, `encoders.rs` or
  `recorder.rs` (the argv must still be accepted by a real ffmpeg);
- `bash scripts/bench.sh` for any change to `budget.rs` or the default profile.

A failing layer is never skipped by editing the test; fix the code or record the decision in
`docs/DECISIONS.md`.

## What the integration script does (contract)

```
lavfi testsrc2=size=1920x1080:rate=30  ─┐
lavfi color=0x4040ff (stands in for the camera) ─┼─► recording_command(plan) ─► seg-000.mp4 (fragmented)
lavfi sine=frequency=440               ─┘                                     │
                                                                              ├─► kill -9 after N s → seg-000.mp4 still probes
                                                                              ├─► second segment → concat_command → out.mp4
                                                                              └─► probe_command → MediaInfo:
                                                                                    codec hevc|h264, 1920×1080, 30 fps,
                                                                                    audio aac, 1 or 2 tracks,
                                                                                    duration ≈ N s, bytes ≈ budget × N
                                                                              pixel check: sample the PiP rectangle,
                                                                              expect the "camera" colour
```

`duration_limit_s` on the plan is what makes this deterministic (`-t`). The script uses whichever
`ffmpeg` the app would use (sidecar in `src-tauri/binaries/` if present, else `PATH`), and prints which.

## CI

`.github/workflows/ci.yml` (owned by `scripts`) is expected to run: `cargo fmt --check`, `cargo clippy`,
`cargo test --workspace` (ubuntu + macos + windows runners — this is where the Windows **unit** tests
actually execute on Windows), `npm run check`, `npm test`, and the integration script on the macOS
runner. CI never has a camera, a display with TCC, or a hardware encoder you can rely on, so it stops at
the integration layer.

## Where numbers may be quoted from

Only from a run you can point at: a `docs/reports/` report, `docs/BENCH.md`, a CI log, or a pasted
terminal block. Everything else is written as "expected" (derived from the budget) or "not yet
measured". On 2026-11-04 the expected values are: 889 kbps total, 793 kbps video at 96 kbps audio,
≈ 13.3 MB for a two-minute recording at the cap; the measured values are the integrator's to report.
