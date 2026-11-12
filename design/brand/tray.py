#!/usr/bin/env python3
"""tray.py: draw the menu-bar / tray glyphs. Standard library only.

The glyph is the app icon reduced to what survives at 16 px: two reels and the tape running
down from them to the head.

    python3 design/brand/tray.py <out dir>

Writes, into <out dir>:
    tray.png, tray@2x.png            black on clear, 22 pt: a macOS template image (the system tints it)
    tray-rec.png, tray-rec@2x.png    the same glyph in the record red, shown while recording
    tray-dark-{16,20,24,32}.png      white on clear, for a dark Windows taskbar
    tray-light-{16,20,24,32}.png     black on clear, for a light Windows taskbar
    tray-rec-{16,20,24,32}.png       the record red, for either
"""
import struct
import sys
import zlib
from pathlib import Path

GRID = 22.0  # the glyph is designed on a 22 x 22 grid
REELS = ((6.0, 8.0), (16.0, 8.0))
OUTER, INNER, HUB = 4.9, 3.5, 1.2
HEAD = (8.4, 16.0, 13.6, 19.2, 1.0)  # the head block: x0, y0, x1, y1, corner radius
# The tape leaves each reel on its outer side and runs down to the head.
TAPE = (((2.35, 11.3), (8.6, 17.4)), ((19.65, 11.3), (13.4, 17.4)))
TAPE_HALF = 0.62
RECORD_RGB = (226, 69, 45)  # --color-record in design/tokens.css
SS = 6  # samples per pixel edge

BLACK = (0, 0, 0)
WHITE = (255, 255, 255)
RECORD = RECORD_RGB


def near_segment(x: float, y: float, a, b, half: float) -> bool:
    (ax, ay), (bx, by) = a, b
    dx, dy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)))
    return (x - ax - t * dx) ** 2 + (y - ay - t * dy) ** 2 <= half * half


def inside(x: float, y: float) -> bool:
    for cx, cy in REELS:
        d2 = (x - cx) ** 2 + (y - cy) ** 2
        if d2 <= HUB * HUB or INNER * INNER <= d2 <= OUTER * OUTER:
            return True
    x0, y0, x1, y1, r = HEAD
    if x0 <= x <= x1 and y0 <= y <= y1:
        nx = min(max(x, x0 + r), x1 - r)
        ny = min(max(y, y0 + r), y1 - r)
        if (x - nx) ** 2 + (y - ny) ** 2 <= r * r:
            return True
    return any(near_segment(x, y, a, b, TAPE_HALF) for a, b in TAPE)


def render(size: int, colour) -> bytes:
    scale = GRID / size
    rows = []
    for py in range(size):
        row = bytearray([0])  # filter type 0
        for px in range(size):
            hits = 0
            for sy in range(SS):
                for sx in range(SS):
                    if inside((px + (sx + 0.5) / SS) * scale, (py + (sy + 0.5) / SS) * scale):
                        hits += 1
            row += bytes((*colour, round(255 * hits / (SS * SS))))
        rows.append(bytes(row))

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(b"".join(rows), 9))
        + chunk(b"IEND", b"")
    )


def main() -> None:
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    files = {
        "tray.png": (22, BLACK),
        "tray@2x.png": (44, BLACK),
        "tray-rec.png": (22, RECORD),
        "tray-rec@2x.png": (44, RECORD),
    }
    for size in (16, 20, 24, 32):
        files[f"tray-dark-{size}.png"] = (size, WHITE)
        files[f"tray-light-{size}.png"] = (size, BLACK)
        files[f"tray-rec-{size}.png"] = (size, RECORD)
    for name, (size, colour) in files.items():
        (out / name).write_bytes(render(size, colour))
    print(f"{len(files)} tray glyphs -> {out}")


if __name__ == "__main__":
    main()
