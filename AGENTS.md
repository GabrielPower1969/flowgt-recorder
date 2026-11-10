# AGENTS.md — how an agent works in this repository

For any coding agent (Claude Code, Codex, Cursor, Copilot…) and the person driving it. Commands,
invariants and recipes live in `CLAUDE.md`; this file is the entry point and the working agreement.
Written 2026-11-10 as a proposal; in force once Gabriel accepts `docs/COLLABORATION.md`.

## 1. Read in this order, stop when you have enough

| Step | File | You learn | Lines |
|---|---|---|---|
| 1 | `00-START-HERE.md` | the current stage and the next steps | top 30 |
| 2 | your task card in `docs/tasks/` | what to do, which files, how it is checked | all |
| 3 | `CLAUDE.md` | commands, invariants, how to verify a change | sections you need |
| 4 | `docs/PLATFORM-ARCHITECTURE.md` | the layers and the engine contract | sections 1, 2, 7 |
| 5 | `docs/COLLABORATION.md` | who owns which files, branch rules | sections 1–3 |

Read `docs/DECISIONS.md` only for the decision numbers your task card names. Do not read `docs/reports/`,
`docs/HANDOVER-*` or `node_modules/` unless a card sends you there.

## 2. Map by task

| The task is about… | Start in… | Check with… |
|---|---|---|
| bitrate, file names, settings, recovery, ffmpeg arguments | `src-tauri/crates/core/src/` | `npm run core:test` |
| the engine contract (options, events) | `src-tauri/crates/core/src/native.rs` | `npm run core:test` |
| starting, pausing, stopping, supervising processes | `src-tauri/src/recorder.rs` | clippy + manual checklist |
| IPC commands and events | `src-tauri/src/commands.rs`, `src/lib/types.ts`, `src/lib/ipc.ts` | `npm run check && npm test` |
| what the user sees | `src/routes/`, `src/lib/components/`, `src/lib/i18n/` | `npm run check && npm test` |
| macOS capture | `helpers/sck-record/` | its Swift tests + manual checklist |
| Windows capture | Windows blocks in `src-tauri/src/`, `scripts/*.ps1`, later `helpers/win-record/` | `docs/WINDOWS-VERIFY.md` |
| Google sign-in and upload | `src-tauri/src/drive.rs` | manual checklist |
| the Chrome extension | `chrome-extension/` (its own `README.md` and `docs/ARCHITECTURE.md`) | `cd chrome-extension && npm test` |
| look and feel | `design/tokens.css`, `docs/DESIGN.md` | component tests |

## 3. Principles this repository is built on, for agents

1. **One entry point, short files.** Every folder an agent works in can be understood from a file of under
   150 lines. If a document grows past that, split it and link it from here.
2. **Contracts are files, not conversations.** The data contract (`types.rs` + `types.ts`) and the engine
   contract (`native.rs`) are the only ways layers talk. Change the contract first, in its own change.
3. **Pure logic in the core.** If it can be a function from data to data, it goes in `recorder-core` and gets
   a unit test. This is what lets an agent on a Mac change Windows behaviour safely, and the reverse.
4. **Every claim has a command.** "Works" means a command was run and its output is quoted or linked
   (invariant 11). An agent that cannot run the check writes "not verified" and says what is needed.
5. **Small, owned changes.** One task card, one branch, one pull request, files inside your lane
   (`docs/COLLABORATION.md` section 1). Touching another lane means a `shared/…` pull request.
6. **Humans do what only humans can.** Permission dialogs, real-device tests, signing, pushing, product
   decisions. An agent prepares everything up to that point and lists the human steps in one place
   (`docs/MANUAL-CHECKLIST.md`, or the card's "Human steps").
7. **Leave the map current.** A change that moves or adds a module updates `CLAUDE.md`'s directory map and
   this file's section 2 in the same commit.
8. **No hidden state.** Nothing an agent needs lives only in a chat. Decisions go to `docs/DECISIONS.md`,
   findings to the task card, status to `00-START-HERE.md`.

## 4. Task cards

Work is handed out as cards in `docs/tasks/` (`TEMPLATE.md` there). A card is small enough for one session.

- IDs: `W-n` Windows, `M-n` macOS and mobile, `S-n` shared.
- States, kept in the card's front matter: `todo` → `doing` → `review` → `done`.
- An agent starts by setting `doing` and its branch name, and ends by filling "Result" and "Not verified".
- If the card is wrong or too big, the agent stops and edits the card first; it does not improvise scope.

## 5. A session, start to finish

1. Read per section 1. Create the branch named in the card.
2. Make the change inside your lane. Run the checks from `CLAUDE.md` → "How to verify a change".
3. Commit in the repository format (invariant 9). Do not push unless the human driving you says so.
4. Update the card, `00-START-HERE.md` (your platform's status line), and the maps if structure changed.
5. Report: what changed, what was verified with which command, what was not verified, human steps left.

## 6. Hard limits

The risk tiers in `CLAUDE.md` apply to every agent. In short: never push or create remotes on your own,
never widen the CSP or add a network call, never commit binaries, recordings or secrets, never tick a box
in a checklist you did not run on the real thing.
