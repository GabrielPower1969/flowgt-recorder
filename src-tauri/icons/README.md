# src-tauri/icons/

The app icon is the front panel of a small reel-to-reel recorder: a brushed aluminium plate, two reels
(the left one full, the right one nearly empty), and three keys with the red record dot on the middle one.

## Files

| File | What it is |
|---|---|
| `icon-src.png` | the full drawing, 1024 px: tape path, read-out window, screws. Source of every picture of 64 px and larger |
| `icon-src-small.png` | the simple drawing, 512 px: plate, two larger reels, three larger keys. Source of every picture of 50 px and smaller, where the full drawing turns to mush and the red dot disappears |
| `icon.ico` | Windows: 16, 24, 32, 48 px from the simple drawing; 64 and 256 px from the full one |
| `icon.icns` | macOS: 16 and 32 px from the simple drawing, the rest from the full one. The plate is scaled to 86 % of the canvas, as the macOS icon grid expects |
| `32x32.png` … `icon.png`, `Square*Logo.png`, `StoreLogo.png` | generated, same rule |
| `tray*.png` | the tray glyphs: separate drawings, not generated from the app icon |

## Rebuilding

```bash
bash scripts/make-icons.sh          # icon-src*.png -> everything else in this folder
```

To change the drawing itself, render new sources with [Blender](https://www.blender.org) (5.x) and then
run the script above:

```bash
blender -b -P scripts/icons/icon_blender.py -- <out dir> 1024 256 light          # -> icon-light-1024.png        = icon-src.png
blender -b -P scripts/icons/icon_blender.py -- <out dir> 1024 256 light small    # -> icon-light-small-1024.png, scaled to 512 = icon-src-small.png
```

The script also takes `dark` for a graphite plate; that variant is not used.

## Origin and licence

Everything is modelled by `scripts/icons/icon_blender.py` from primitives and procedural materials
(the brushed metal is a noise texture). No downloaded models, textures, HDRIs or fonts. The pictures are
this project's own work under the repository licence. Blender is GPL software; that licence covers the
program, not the pictures rendered with it.

## Checked

- Windows 11: see the pull request that added these files.
- macOS: **not yet looked at.** The `.icns` has the same chunk types as the one the Tauri CLI writes, but
  nobody has seen it in the Dock or in Finder yet.
