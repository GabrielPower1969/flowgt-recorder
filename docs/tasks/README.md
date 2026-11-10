# Task cards

One card per piece of work, small enough for one session. Format: `TEMPLATE.md`. Rules: `AGENTS.md` section 4.

| ID | Title | Owner | Depends on | State |
|---|---|---|---|---|
| W-1 | Build and test on Windows | Windows | — | todo |
| W-2 | Fetch LGPL ffmpeg for Windows | Windows | W-1 | todo |
| W-3 | First real recording on Windows | Windows | W-2 | todo |
| W-6 | Spike: native Windows engine | Windows | W-3, S-1 | todo |
| S-1 | Engine conformance test | Mac | — | todo |
| S-2 | Capability flags | Mac | — | review |

W-4, W-5 and W-7 from `docs/COLLABORATION.md` section 4 get their cards after W-3 reports what it found.
Keep this table in step with the cards' front matter.
