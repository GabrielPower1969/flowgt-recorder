# System audio (macOS)

avfoundation cannot capture system audio, so hearing the other side of a call needs one of three routes (D7).

| | ScreenCaptureKit helper | BlackHole loopback | Off |
|---|---|---|---|
| What it is | `helpers/sck-audio/` Swift CLI writes PCM to a FIFO that ffmpeg reads | Virtual audio driver (`blackhole-2ch`); ffmpeg records it as an input device | Mic only |
| Install | None; uses the Screen Recording permission | `brew install blackhole-2ch`, password, **reboot**, then a Multi-Output Device in Audio MIDI Setup | None |
| You still hear the call | Yes | Yes, only through the Multi-Output Device | Yes |
| Failure modes | Younger code; sync and dropout behaviour unverified; needs macOS 13+ | Wrong output device selected means silence; volume keys do not work on a Multi-Output Device | Other side missing |
| Maturity | Experimental, `--check` and `--self-test` only; never run in a real call | Widely used, plain avfoundation input | Trivial |
| Verified here | Not in a real call (not yet measured) | Not installed on this machine (not yet measured) | Selftest uses a sine mic only |

## Recommendation

**Use BlackHole now; keep the helper experimental.** BlackHole needs no new code and its failure modes are
known; the helper is cheaper for users once it is proven, but nobody has recorded a real call with it. Level D
(my judgement, from the table above).

Decision for Gabriel: install BlackHole or keep waiting for the helper (MANUAL-CHECKLIST item g). Record
the outcome as a new entry in `DECISIONS.md`.

## How to test the helper

1. Run `helpers/sck-audio` build, then its `--check` (exit 2 means permission missing).
2. In Settings choose System audio, Helper. Record 2 minutes of a real Zoom/Teams/Meet call.
3. Play the file: is the far side complete, in sync with video, with no gaps?
