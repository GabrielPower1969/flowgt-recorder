#!/usr/bin/env python3
"""assets.py: the list of every picture in the repository, and the check that keeps it true.

    python3 design/brand/assets.py write     rewrite design/brand/ASSETS.md from what is tracked
    python3 design/brand/assets.py check     fail when a tracked picture is not in the rules below,
                                             or when a retired picture (retired.sha256) is back
    python3 design/brand/assets.py check <bundle or folder> ...
                                             also look for retired pictures inside built bundles

Standard library only. Run from the repository root (scripts/make-icons.sh does).
"""
import hashlib
import re
import struct
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
PICTURE = re.compile(r"\.(png|ico|icns|svg|jpe?g|webp|gif)$", re.I)

FULL = "icon-full-1024.png"
SMALL = "icon-small-512.png"
# (pattern, made from, used for). First match wins. A picture no rule matches fails the check.
RULES = [
    (r"design/brand/icon-full-1024\.png", "Blender: icon_blender.py", "source of every picture of 64 px and larger"),
    (r"design/brand/icon-small-512\.png", "Blender: icon_blender.py (small)", "source of every picture of 50 px and smaller"),
    (r"src-tauri/icons/icon\.icns", f"{FULL} and {SMALL}, plate at 86 %", "macOS: Finder, Dock, System Settings, Mac App Store bundle"),
    (r"src-tauri/icons/icon\.ico", f"{FULL} and {SMALL}", "Windows: Explorer, taskbar, Alt-Tab, installer"),
    (r"src-tauri/icons/(32x32|Square30x30Logo|Square44x44Logo|StoreLogo)\.png", SMALL, "Tauri bundle and Windows Store tiles, small sizes"),
    (r"src-tauri/icons/(64x64|128x128|128x128@2x|icon)\.png", FULL, "Tauri bundle; Linux; the window icon"),
    (r"src-tauri/icons/Square\d+x\d+Logo\.png", FULL, "Windows Store tiles"),
    (r"src-tauri/icons/tray(@2x)?\.png", "tray.py", "macOS menu bar (template image) and the tray while idle"),
    (r"src-tauri/icons/tray-rec(@2x)?\.png", "tray.py", "menu bar / tray while recording"),
    (r"src-tauri/icons/tray-(dark|light|rec)-\d+\.png", "tray.py", "Windows tray at 100 to 200 % scaling; not wired in yet"),
    (r"src/assets/brand/logo-\d+\.png", FULL, "inside the app: the About page"),
    (r"chrome-extension/extension/icons/.*", "the extension's own drawing", "Chrome extension. NOT the app icon yet: the folder is frozen until Gabriel says otherwise"),
]


def tracked() -> list[str]:
    out = subprocess.run(["git", "ls-files", "-co", "--exclude-standard"], cwd=ROOT, capture_output=True, text=True, check=True).stdout
    skip = ("design-shots/", ".browsers/", "host/", "tests/out/", "node_modules/")
    return sorted(f for f in out.splitlines() if PICTURE.search(f) and not f.startswith(skip) and (ROOT / f).exists())


def rule_for(path: str):
    for pattern, source, use in RULES:
        if re.fullmatch(pattern, path):
            return source, use
    return None


def size_of(path: Path) -> str:
    data = path.read_bytes()
    if data[:8] == b"\x89PNG\r\n\x1a\n":
        w, h = struct.unpack(">II", data[16:24])
        return f"{w} × {h}"
    if path.suffix == ".ico":
        count = struct.unpack("<H", data[4:6])[0]
        sizes = sorted({data[6 + 16 * i] or 256 for i in range(count)})
        return ", ".join(str(s) for s in sizes) + " px"
    if path.suffix == ".icns":
        return "16 to 1024 px"
    return "vector" if path.suffix == ".svg" else "?"


def retired() -> dict[str, str]:
    table = {}
    for line in (HERE / "retired.sha256").read_text().splitlines():
        if line.strip():
            digest, _, what = line.partition("  ")
            table[digest] = what
    return table


def write() -> None:
    rows = []
    for f in tracked():
        rule = rule_for(f)
        source, use = rule if rule else ("?", "NOT LISTED: add a rule to assets.py")
        rows.append(f"| `{f}` | {size_of(ROOT / f)} | {source} | {use} |")
    text = f"""# Pictures

Every picture in the repository, where it comes from and where it is used. This file is written by
`python3 design/brand/assets.py write` (`scripts/make-icons.sh` runs it); do not edit it by hand.
To change a picture, change its source in this folder and run `bash scripts/make-icons.sh`.

| File | Size | Made from | Used for |
|---|---|---|---|
{chr(10).join(rows)}

## Retired pictures

`retired.sha256` holds the fingerprints of pictures that must not come back: the first icon (a blue
square with a white dot) and the first tray glyph (a dot in a ring). `assets.py check` fails when a
tracked file, or a file inside a bundle given to it, has one of them.
"""
    (HERE / "ASSETS.md").write_text(text)
    print(f"ASSETS.md: {len(rows)} pictures")


def check(extra: list[str]) -> int:
    problems = []
    old = retired()
    files = tracked()
    listed = (HERE / "ASSETS.md").read_text() if (HERE / "ASSETS.md").exists() else ""
    for f in files:
        if rule_for(f) is None:
            problems.append(f"{f}: no rule in design/brand/assets.py says what this picture is")
        if f"`{f}`" not in listed:
            problems.append(f"{f}: not in design/brand/ASSETS.md (run scripts/make-icons.sh)")
        digest = hashlib.sha256((ROOT / f).read_bytes()).hexdigest()
        if digest in old:
            problems.append(f"{f}: a retired picture is back ({old[digest]})")
    for line in listed.splitlines():
        m = re.match(r"\| `([^`]+)` \|", line)
        if m and m.group(1) not in files:
            problems.append(f"{m.group(1)}: listed in ASSETS.md but not in the repository")
    looked = 0
    for target in extra:
        base = Path(target)
        for p in ([base] if base.is_file() else base.rglob("*")):
            if p.is_file() and PICTURE.search(p.name):
                looked += 1
                digest = hashlib.sha256(p.read_bytes()).hexdigest()
                if digest in old:
                    problems.append(f"{p}: a retired picture ({old[digest]})")
    for p in problems:
        print("FAIL:", p, file=sys.stderr)
    if not problems:
        print(f"PASS: {len(files)} pictures listed, none retired" + (f"; {looked} pictures in {len(extra)} bundle(s) checked" if extra else ""))
    return 1 if problems else 0


if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else ""
    if mode == "write":
        write()
    elif mode == "check":
        sys.exit(check(sys.argv[2:]))
    else:
        print(__doc__)
        sys.exit(2)
