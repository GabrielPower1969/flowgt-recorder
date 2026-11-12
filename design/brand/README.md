# design/brand/

The one place the app's pictures come from. Everything else (`src-tauri/icons/`, `src/assets/brand/`)
is generated from here by `bash scripts/make-icons.sh` and listed in [`ASSETS.md`](ASSETS.md).

| File | What it is |
|---|---|
| `icon-full-1024.png` | the full drawing of the app icon: a brushed aluminium plate, two reels (the left one full), tape path, read-out window, three keys with the red record dot. Source of every picture of 64 px and larger |
| `icon-small-512.png` | the simple drawing: plate, two larger reels, three larger keys. Source of every picture of 50 px and smaller, where the full drawing turns to mush and the red dot disappears |
| `icon_blender.py` | models both drawings from primitives and procedural materials in [Blender](https://www.blender.org) (5.x) |
| `tray.py` | draws the menu-bar / tray glyphs: two reels and the tape running down to the head. Standard library only |
| `assets.py` | writes `ASSETS.md` and checks it: every picture in the repository must be listed, and no retired picture may come back |
| `retired.sha256` | fingerprints of the pictures that are gone for good: the first icon (blue square, white dot) and the first tray glyph (dot in a ring) |
| `ASSETS.md` | generated: every picture, its size, its source, where it is used |

## Changing a picture

```bash
bash scripts/make-icons.sh            # sources -> every generated picture, then the list and the check
bash scripts/make-icons.sh --check    # change nothing; fail on an unlisted or a retired picture
```

To change the icon drawing itself, render new sources with Blender, then run the script:

```bash
blender -b -P design/brand/icon_blender.py -- <out dir> 1024 256 light          # -> icon-light-1024.png        = icon-full-1024.png
blender -b -P design/brand/icon_blender.py -- <out dir> 1024 256 light small    # -> icon-light-small-1024.png, scaled to 512 = icon-small-512.png
```

The script also takes `dark` for a graphite plate; that variant is not used.

To change the tray glyph, edit the numbers at the top of `tray.py` (a 22 x 22 grid).

## Rules

- A picture is never drawn twice by hand: one source, every size generated.
- A size of 50 px or less comes from the simple drawing.
- macOS gets the plate at 86 % of the canvas (the system's icon grid); Windows gets it full size.
- The macOS menu-bar glyph is a template image (black on clear); while recording the same glyph is red.
- `chrome-extension/` still has its own older drawing. That folder is frozen; replacing its icons needs
  the owner's say.

## Origin and licence

Everything is modelled or drawn by the scripts in this folder. No downloaded models, textures, HDRIs or
fonts. The pictures are this project's own work under the repository licence. Blender is GPL software;
that licence covers the program, not the pictures rendered with it.
