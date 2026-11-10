# CLAUDE.md — FlowGT Recorder

Map for LLM agents and new developers. The user manual is `README.md`; the "why" and the timeline live
in the Hours vault (`hours/30-项目/项目-screen-recorder`), reached through `HOURS.md`. Current stage and
next steps: `00-START-HERE.md`. Architecture and decisions: `docs/ARCHITECTURE.md`, `docs/DECISIONS.md`
— read both before changing anything; they are the source of truth and this file only points at them.

Agents start at `AGENTS.md` (reading order, map by task, task cards in `docs/tasks/`). Target shape for all
platforms: `docs/PLATFORM-ARCHITECTURE.md`. Two-person rules: `docs/COLLABORATION.md`. License: MIT
(`LICENSE`). These three are proposals of 2026-11-10 until Gabriel accepts them.

## Commands

```bash
npm install                                   # once; also installs the Tauri CLI
bash scripts/fetch-ffmpeg.sh                  # LGPL ffmpeg + ffprobe → src-tauri/binaries/ (sidecars)
bash scripts/build-mac.sh                     # helper + release .app (reads .env.google for sign-in)

npm run tauri dev                             # app with hot reload (Vite :1420 + cargo run)   ⚠ TCC prompts
npm run dev                                   # frontend alone in a browser, against the ipc mock
npm run tauri build                           # release bundle (unsigned)

npm test                                      # vitest run: unit + component tests
npm run test:watch                            # vitest in watch mode
npm run test:ui                               # vitest with coverage → coverage/
npm run check                                 # svelte-check
npm run core:test                             # cargo test --manifest-path src-tauri/Cargo.toml --workspace
cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check

bash scripts/integration.sh                   # real ffmpeg, synthetic inputs → MP4 → ffprobe + pixel check
bash scripts/bench.sh                         # encoder speed, CPU, MB/h on synthetic input
bash scripts/bench.sh --app --seconds 60      # the same while the app records for real    ⚠ needs a human
./src-tauri/target/debug/flowgt-recorder --selftest 5   # 5-second end-to-end self test     ⚠ needs permissions
```

## Directory map

| Path | Owns | Notes |
|---|---|---|
| `src-tauri/crates/core/src/types.rs` | **the data contract** | mirrored 1:1 by `src/lib/types.ts`; camelCase on the wire |
| `src-tauri/crates/core/src/*.rs` | pure logic: `budget` `settings` `filename` `disk` `progress` `recovery` `devices` `encoders` `pip` `command` | no I/O; takes strings/paths, returns commands/plans; `fixtures/` holds real `ffmpeg` output |
| `src-tauri/src/` | the Tauri app crate: `commands.rs` (IPC), `recorder.rs` (process supervisor), `drive.rs` (Google sign-in + upload), `sidecar.rs`, `permissions.rs`, `tray.rs`, `shortcuts.rs`, `disk.rs` | all I/O lives here |
| `src-tauri/tauri.conf.json` | windows, CSP, tray, `externalBin`, bundle | `connect-src` allows only `ipc:` — keep it that way |
| `src-tauri/capabilities/default.json` | Tauri permissions for the `main` and `floating` windows | add a line when a new plugin command is called |
| `src-tauri/Info.plist` · `Entitlements.plist` | macOS usage strings and entitlements | TCC prompt texts |
| `src/lib/types.ts` | `Commands` and `Events` enumerations | the frontend never calls `invoke` directly |
| `src/lib/ipc.ts` | typed wrapper + **mock** implementation | mock is what vitest and `npm run dev` use |
| `src/lib/i18n/` | `zh` and `en` string tables | every visible string has a key in both |
| `src/lib/stores/` · `src/lib/components/` · `src/routes/` | state, reusable pieces, one route per screen | screens: Main (home + `ScreenPicker`), Settings, Summary, Floating (D18) |
| `design/tokens.css` · `docs/DESIGN.md` | design tokens and the "Studio" look | tokens are used verbatim (D13) |
| `helpers/sck-audio/` | Swift ScreenCaptureKit audio helper → PCM FIFO | experimental in v0.1 (D7) |
| `scripts/` · `tests/` · `.github/workflows/ci.yml` | integration, bench, fetch-ffmpeg, CI | outputs in `tests/out/`, `bench-out/` (git-ignored) |
| `chrome-extension/` | the FlowGT Recorder Chrome extension (MV3, tab + mic audio) and its yt-dlp native host | own `package.json`, tests and `docs/`; independent of the desktop app (D31) |
| `docs/` | see `README.md` → Documentation | `docs/reports/NN-…` numbered, newest = latest |

## Invariants

1. **`types.rs` and `types.ts` change together, in one commit.** Same names, same variants, camelCase
   on the wire. A field added on one side without the other is a bug even if everything compiles.
2. **`recorder-core` does no I/O.** No `std::fs`, `std::process`, no network, no OS queries. It takes
   strings (ffmpeg output), paths and plain data and returns commands, plans and parsed data, so it is
   tested with `cargo test` on any OS — this is how Windows code paths are tested on a Mac (D3).
3. **Every segment is a fragmented MP4** (`-movflags +frag_keyframe+empty_moov+default_base_moof`), and
   the session manifest is rewritten on every state transition. Never "optimise" this away (D4).
4. **Pause is a segment boundary**, stop is a concat with `-c copy`. No SIGSTOP, no second ffmpeg (D4).
5. **No absolute paths in any document or script.** Paths are relative to the repository, or to the
   workspace root in the form `4-business/flowgt/screen-recorder` (the Hours vault rule).
6. **Code, comments, commit messages and file names in the code tree are English.** Chinese belongs in
   `HOURS.md`, `00-START-HERE.md`, `docs/MANUAL-CHECKLIST.md`, `docs/UX.md`, `docs/reports/` and the
   `zh` string table.
7. **UI strings only through i18n.** No literal visible text in `.svelte` files; both `zh` and `en`
   tables carry every key (a test checks parity).
8. **Design tokens only from `design/tokens.css`.** No hex colours, font names or radii typed into
   components.
9. **Commit format** `type(scope): summary`, body optional, and the trailer
   `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` whenever an agent wrote it. Types:
   `feat` `fix` `docs` `test` `chore` `refactor` `design`; scopes: `core` `app` `ui` `docs` `scripts`
   `helper` `design`.
10. **Never push to `main`, force-push, or create another remote without Gabriel's explicit OK** (D31). The
    remote is `github.com/GabrielPower1969/flowgt-recorder` (public). Agents may push their own
    `win/…`, `mac/…`, `shared/…` branches and open draft pull requests; merging is a human decision.
11. **Every "verified" or "tested" claim in a document is backed by command output** quoted or linked
    (a report under `docs/reports/`, a CI run, a pasted terminal block). Otherwise write "expected" or
    "not yet measured".
12. **Nothing leaves the machine unless the user signed in to Google (D19).** Then recordings go to their
    own Drive, nowhere else. No telemetry, no crash reporter, no update check, no fonts from a CDN. The CSP in `tauri.conf.json` enforces `connect-src ipc: http://ipc.localhost`; a change to that
    line is a product decision, not a convenience.
13. **Do not commit `src-tauri/binaries/*`** (tens of MB per binary); they are fetched by the script.

## How to verify a change

| You changed… | Run at least… | And read… |
|---|---|---|
| `crates/core/**` | `npm run core:test`, clippy | `docs/TESTING.md` → unit layer |
| `command.rs`, `pip.rs`, `encoders.rs` | the above **plus** `bash scripts/integration.sh` (real ffmpeg must accept the argv) | `tests/out/` summary |
| `budget.rs` or default settings | the above plus `bash scripts/bench.sh` | `docs/BENCH.md` |
| `types.rs` / `types.ts` | `npm run core:test && npm run check && npm test` (the mock in `ipc.ts` must still type-check) | — |
| `src/**` | `npm run check && npm test` | component tests next to the file |
| `src-tauri/src/**` | `cargo clippy …`, then `npm run tauri dev` **with the human present** and the relevant item in `docs/MANUAL-CHECKLIST.md` | — |
| `tauri.conf.json` CSP / capabilities | `npm run tauri dev`, then check the webview console for CSP violations | `docs/PRIVACY.md` |
| docs only | re-read for absolute paths (`grep -rn "/Users/" docs README.md CLAUDE.md`) | — |

Rule: **all green before commit** — `npm run check && npm test && npm run core:test` (plus clippy for
Rust changes). The integration script is required before committing anything that changes the ffmpeg
argv; the bench before changing the budget defaults.

## Risk tiers for actions

| Tier | Actions | Who |
|---|---|---|
| **0 · free** | read files, `cargo test`, `vitest`, clippy, `svelte-check`, `scripts/integration.sh` and `scripts/bench.sh` without `--app` (synthetic lavfi inputs, no devices, no permissions) | agent, any time |
| **1 · announce first** | `npm run tauri dev`, `--selftest`, `bench.sh --app`: these open windows, register global shortcuts and **trigger macOS TCC prompts** that only a human can click; a recording may start and write files to `~/Movies/FlowGT Recorder` | agent, after saying so and with Gabriel at the keyboard |
| **2 · human only** | clicking any system permission dialog, toggles in System Settings, installing BlackHole (needs a reboot), running anything on a Windows machine, creating the GitHub remote, pushing, deleting recordings, signing / notarising | Gabriel — listed in `docs/MANUAL-CHECKLIST.md` |
| **never** | uploading a recording anywhere but the signed-in user's own Drive, adding other network calls, widening the CSP, committing binaries or recordings, force-pushing | — |

## Recipes

### Add a setting

1. `src-tauri/crates/core/src/types.rs` — add the field to the right struct (`Settings`, `VideoSettings`,
   `AudioSettings`, `CameraSettings`, …). Document range and unit in a doc comment.
2. `src/lib/types.ts` — mirror it, camelCase.
3. `src-tauri/crates/core/src/settings.rs` — give it a value in `default_settings`, a range check in
   `validate`, and make `from_json` fill it when an old file lacks it (bump `SETTINGS_VERSION` only when
   the meaning of an existing field changes).
4. If the setting changes the ffmpeg command: `command.rs` (`make_plan` / `recording_command`), with a
   unit test asserting the new argv fragment.
5. `src/lib/ipc.ts` — the mock's default settings object.
6. `src/routes/Settings.svelte` — the control, bound to the settings store.
7. `src/lib/i18n/zh.ts` and `en.ts` — label and help text.
8. Tests: core round-trip (`default → to_json → from_json == default`, `validate` rejects out of range),
   Vitest for the control. Then `npm run check && npm test && npm run core:test`.

### Add a command

1. `src/lib/types.ts` → `Commands`: `snake_case_name: (args: {…}) => ReturnType`. New payload types go in
   both `types.rs` and `types.ts`.
2. `src-tauri/src/commands.rs` — `#[tauri::command] async fn snake_case_name(…)`, keep logic that can be
   pure in `recorder-core` and call it from here.
3. `src-tauri/src/lib.rs` — add it to `tauri::generate_handler![…]`.
4. `src-tauri/capabilities/default.json` — only if it uses a plugin command that is not yet allowed.
5. `src/lib/ipc.ts` — typed wrapper and a mock branch that returns realistic data.
6. If it emits events: add the event name and payload to `Events` in `types.ts`, emit from Rust with the
   same name, subscribe through `ipc.ts`.
7. Tests: a core test for the pure part, a Vitest test against the mock, and the integration script if
   it spawns ffmpeg.

## Pointers

- `docs/ARCHITECTURE.md` — the diagram, the pipeline, module ownership, IPC, Windows.
- `docs/DECISIONS.md` — D1 stack · D2 sidecar · D3 pure core · D4 segments · D5 budget · D6 webview
  preview · D7 system audio · D8 Windows · D9 window capture deferred · D10 file names · D11 floating
  window · D12 settings store · D13 design tokens · D14 no remote (replaced by D31: one public repository).
- `docs/BENCH.md` — measured numbers; quote them, do not invent new ones.
- `docs/WINDOWS-VERIFY.md` — nothing Windows-side has run on hardware; this is the plan.
- `docs/SYSTEM-AUDIO.md` — helper vs BlackHole.
- `docs/TESTING.md` · `docs/LICENSES.md` · `docs/PRIVACY.md` · `docs/UX.md` · `docs/MANUAL-CHECKLIST.md`.
