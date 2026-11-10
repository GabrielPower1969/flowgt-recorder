# Task cards

One card per piece of work, small enough for one session. Format: `TEMPLATE.md`. Rules: `AGENTS.md` section 4.

| ID | Title | Owner | Depends on | State |
|---|---|---|---|---|
| W-1 | Build and test on Windows | Windows | W-2 | review |
| W-2 | Fetch LGPL ffmpeg for Windows | Windows | — | review |
| W-3 | First real recording on Windows | Windows | W-2 | review |
| W-4 | Fix what the first Windows recording found | Windows | W-3 | doing |
| W-6 | Spike: native Windows engine | Windows | W-3, S-1 | todo |
| S-1 | Engine conformance test | Mac | — | todo |
| S-2 | Capability flags | Mac | — | todo |

W-5 and W-7 from `docs/COLLABORATION.md` section 4 get their cards after W-3 reports what it found.
Keep this table in step with the cards' front matter.
