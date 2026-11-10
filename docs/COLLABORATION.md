# Collaboration: two people, two platforms

Written 2026-11-10. Status: **proposal, waiting for Gabriel's OK**. Nothing here is in force until the
GitHub remote exists (invariant 10 in `CLAUDE.md`, D14).

Two developers work at the same time: the **Mac owner** (Gabriel, with Claude) and the **Windows owner**
(new developer). This file says who owns which files, how changes meet, and what the Windows owner does first.

## 1. Who owns what

"Owns" means: you change it freely on your own branch, and you review the other person's changes to it.

| Area | Paths | Owner |
|---|---|---|
| macOS capture | `helpers/sck-record/`, `helpers/sck-audio/`, `src-tauri/Info.plist`, `src-tauri/Entitlements.plist`, `scripts/build-mac.sh`, `scripts/build-helper.sh`, `scripts/setup-signing.sh` | Mac |
| macOS code paths | `#[cfg(target_os = "macos")]` blocks in `src-tauri/src/` | Mac |
| Windows capture | the `ddagrab` / `dshow` / loopback paths, any future `helpers/win-*/` | Windows |
| Windows code paths | `#[cfg(windows)]` / `#[cfg(target_os = "windows")]` blocks in `src-tauri/src/` | Windows |
| Windows tooling | `scripts/*.ps1`, `docs/WINDOWS-VERIFY.md`, the Windows installer settings in `tauri.conf.json` → `bundle.windows` | Windows |
| **Shared contract** | `src-tauri/crates/core/src/types.rs` + `src/lib/types.ts`, `settings.rs`, the helper event protocol in `src-tauri/src/` (`native` module) | both: see section 3 |
| Shared product | `src/**` (UI), `src/lib/i18n/`, `design/`, `src-tauri/crates/core/` (other files), `recorder.rs`, `commands.rs`, `tauri.conf.json` (rest), `capabilities/`, `.github/workflows/ci.yml` | both: see section 3 |
| Product decisions | `docs/DECISIONS.md` (D-numbers), `docs/PRIVACY.md`, CSP line, `LICENSE` | Gabriel only |

## 2. Branches and merging

- `main` is always green and always releasable on macOS. Nobody commits to `main` directly.
- Branch names: `win/<topic>` for the Windows owner, `mac/<topic>` for the Mac owner, `shared/<topic>` for
  changes to shared files.
- One pull request per topic. Keep it small: aim for under 400 changed lines.
- CI runs on `macos-14` and `windows-latest` for every pull request (`.github/workflows/ci.yml`; it has
  never run yet). Both must pass before merge.
- Merge by squash. The author merges after one approval from the other person.
- Rebase your branch on `main` at least twice a week, so the two platforms do not drift apart.

## 3. Rules for shared files

1. **Contract first.** A change to `types.rs` / `types.ts`, to settings, or to the helper event protocol goes
   in its own `shared/…` pull request, merged **before** the platform work that needs it. Invariant 1 still
   holds: both type files change in one commit.
2. **No platform leaks into shared code.** `recorder-core` stays free of I/O (invariant 2). A UI difference
   between platforms is driven by a capability flag from the backend, not by `if windows` in a component.
3. **A feature on one platform must not break the other.** If Windows cannot do it yet, the backend reports
   the capability as missing and the UI hides the control.
4. **Decision numbers.** The Mac owner keeps `D30, D31, …`. The Windows owner records decisions as
   `W1, W2, …` in the same `docs/DECISIONS.md`, under a `## Windows` heading. This avoids two people taking
   the same number.
5. **Reports.** Windows reports are `docs/reports/NN-Windows…`; take the next free number at merge time.

## 4. What the Windows owner does first

Order matters; each item ends in a pull request or a report.

| # | Task | Done when |
|---|---|---|
| W-1 | Clone, install the toolchain (`docs/WINDOWS-VERIFY.md` section 1), run `npm run check`, `npm test`, `npm run core:test`, clippy | all four are green on Windows; fixes merged |
| W-2 | A script that fetches LGPL `ffmpeg` and `ffprobe` for Windows into `src-tauri/binaries/` (the PowerShell twin of `scripts/fetch-ffmpeg.sh`) | `scripts/integration.ps1` passes with the fetched binaries |
| W-3 | Run `docs/WINDOWS-VERIFY.md` sections 2–5 on real hardware | report `docs/reports/NN-Windows实测-<date>.html`, numbers in `docs/BENCH.md` |
| W-4 | Fix what W-3 found: device names (non-ASCII), default devices, pause/resume, cleanup | the checklist is fully ticked |
| W-5 | System audio on Windows: decide between a loopback device and a small WASAPI loopback helper | decision `W1` written; system audio lands in a recording |
| W-6 | Parity with macOS D27–D29 (one clock for all tracks, record one window, live preview) | decision `W2` on the approach; see the note below |
| W-7 | Installer and code signing for Windows | an installer a tester can run |

Cards for these tasks live in `docs/tasks/`; how an agent picks one up is in `AGENTS.md`.

Note on W-6 (a suggestion to evaluate, not a decision): on macOS the app talks to `sck-record` through
command-line arguments and JSON events on stdout. A Windows helper built on Windows.Graphics.Capture that
speaks the same protocol could reuse the supervisor in `recorder.rs`. The Windows owner should read
`helpers/sck-record/` and the `native` module first, then propose.

## 5. What must not be shared or changed

- `.env.google` and `.signing/` never leave Gabriel's machine and are git-ignored. The Windows owner gets
  their own test OAuth client if sign-in must be tested on Windows.
- The privacy invariants (12 in `CLAUDE.md`) apply to every platform: no telemetry, no update check, no new
  network call, no wider CSP.
- No binaries and no recordings in git (invariant 13).

## 6. Keeping each other informed

- Every pull request description says: what changed, how it was verified (command output, invariant 11),
  what was **not** verified.
- `00-START-HERE.md` keeps one status line per platform. Each person updates their line when a pull request
  merges.
- Questions about shared files go in the pull request or a GitHub issue, so the answer stays with the code.

## 7. Before any of this can start (Gabriel only)

1. Create the private GitHub repository and say its name. Then the existing history is pushed.
2. Invite the Windows owner, and turn on branch protection for `main` (pull request + passing CI required).
3. Tell the Windows owner the project is MIT (`LICENSE`): what they contribute is published under MIT too.
4. Decide whether the Windows owner tests Google sign-in (needs their own OAuth client) or skips it.
