# Pictures

Every picture in the repository, where it comes from and where it is used. This file is written by
`python3 design/brand/assets.py write` (`scripts/make-icons.sh` runs it); do not edit it by hand.
To change a picture, change its source in this folder and run `bash scripts/make-icons.sh`.

| File | Size | Made from | Used for |
|---|---|---|---|
| `chrome-extension/extension/icons/128.png` | 128 × 128 | the extension's own drawing | Chrome extension. NOT the app icon yet: the folder is frozen until Gabriel says otherwise |
| `chrome-extension/extension/icons/16.png` | 16 × 16 | the extension's own drawing | Chrome extension. NOT the app icon yet: the folder is frozen until Gabriel says otherwise |
| `chrome-extension/extension/icons/32.png` | 32 × 32 | the extension's own drawing | Chrome extension. NOT the app icon yet: the folder is frozen until Gabriel says otherwise |
| `chrome-extension/extension/icons/48.png` | 48 × 48 | the extension's own drawing | Chrome extension. NOT the app icon yet: the folder is frozen until Gabriel says otherwise |
| `chrome-extension/extension/icons/logo.svg` | vector | the extension's own drawing | Chrome extension. NOT the app icon yet: the folder is frozen until Gabriel says otherwise |
| `design/brand/icon-full-1024.png` | 1024 × 1024 | Blender: icon_blender.py | source of every picture of 64 px and larger |
| `design/brand/icon-small-512.png` | 512 × 512 | Blender: icon_blender.py (small) | source of every picture of 50 px and smaller |
| `src-tauri/icons/128x128.png` | 128 × 128 | icon-full-1024.png | Tauri bundle; Linux; the window icon |
| `src-tauri/icons/128x128@2x.png` | 256 × 256 | icon-full-1024.png | Tauri bundle; Linux; the window icon |
| `src-tauri/icons/32x32.png` | 32 × 32 | icon-small-512.png | Tauri bundle and Windows Store tiles, small sizes |
| `src-tauri/icons/64x64.png` | 64 × 64 | icon-full-1024.png | Tauri bundle; Linux; the window icon |
| `src-tauri/icons/Square107x107Logo.png` | 107 × 107 | icon-full-1024.png | Windows Store tiles |
| `src-tauri/icons/Square142x142Logo.png` | 142 × 142 | icon-full-1024.png | Windows Store tiles |
| `src-tauri/icons/Square150x150Logo.png` | 150 × 150 | icon-full-1024.png | Windows Store tiles |
| `src-tauri/icons/Square284x284Logo.png` | 284 × 284 | icon-full-1024.png | Windows Store tiles |
| `src-tauri/icons/Square30x30Logo.png` | 30 × 30 | icon-small-512.png | Tauri bundle and Windows Store tiles, small sizes |
| `src-tauri/icons/Square310x310Logo.png` | 310 × 310 | icon-full-1024.png | Windows Store tiles |
| `src-tauri/icons/Square44x44Logo.png` | 44 × 44 | icon-small-512.png | Tauri bundle and Windows Store tiles, small sizes |
| `src-tauri/icons/Square71x71Logo.png` | 71 × 71 | icon-full-1024.png | Windows Store tiles |
| `src-tauri/icons/Square89x89Logo.png` | 89 × 89 | icon-full-1024.png | Windows Store tiles |
| `src-tauri/icons/StoreLogo.png` | 50 × 50 | icon-small-512.png | Tauri bundle and Windows Store tiles, small sizes |
| `src-tauri/icons/icon.icns` | 16 to 1024 px | icon-full-1024.png and icon-small-512.png, plate at 86 % | macOS: Finder, Dock, System Settings, Mac App Store bundle |
| `src-tauri/icons/icon.ico` | 16, 24, 32, 48, 64, 256 px | icon-full-1024.png and icon-small-512.png | Windows: Explorer, taskbar, Alt-Tab, installer |
| `src-tauri/icons/icon.png` | 512 × 512 | icon-full-1024.png | Tauri bundle; Linux; the window icon |
| `src-tauri/icons/tray-dark-16.png` | 16 × 16 | tray.py | Windows tray at 100 to 200 % scaling; not wired in yet |
| `src-tauri/icons/tray-dark-20.png` | 20 × 20 | tray.py | Windows tray at 100 to 200 % scaling; not wired in yet |
| `src-tauri/icons/tray-dark-24.png` | 24 × 24 | tray.py | Windows tray at 100 to 200 % scaling; not wired in yet |
| `src-tauri/icons/tray-dark-32.png` | 32 × 32 | tray.py | Windows tray at 100 to 200 % scaling; not wired in yet |
| `src-tauri/icons/tray-light-16.png` | 16 × 16 | tray.py | Windows tray at 100 to 200 % scaling; not wired in yet |
| `src-tauri/icons/tray-light-20.png` | 20 × 20 | tray.py | Windows tray at 100 to 200 % scaling; not wired in yet |
| `src-tauri/icons/tray-light-24.png` | 24 × 24 | tray.py | Windows tray at 100 to 200 % scaling; not wired in yet |
| `src-tauri/icons/tray-light-32.png` | 32 × 32 | tray.py | Windows tray at 100 to 200 % scaling; not wired in yet |
| `src-tauri/icons/tray-rec-16.png` | 16 × 16 | tray.py | Windows tray at 100 to 200 % scaling; not wired in yet |
| `src-tauri/icons/tray-rec-20.png` | 20 × 20 | tray.py | Windows tray at 100 to 200 % scaling; not wired in yet |
| `src-tauri/icons/tray-rec-24.png` | 24 × 24 | tray.py | Windows tray at 100 to 200 % scaling; not wired in yet |
| `src-tauri/icons/tray-rec-32.png` | 32 × 32 | tray.py | Windows tray at 100 to 200 % scaling; not wired in yet |
| `src-tauri/icons/tray-rec.png` | 22 × 22 | tray.py | menu bar / tray while recording |
| `src-tauri/icons/tray-rec@2x.png` | 44 × 44 | tray.py | menu bar / tray while recording |
| `src-tauri/icons/tray.png` | 22 × 22 | tray.py | macOS menu bar (template image) and the tray while idle |
| `src-tauri/icons/tray@2x.png` | 44 × 44 | tray.py | macOS menu bar (template image) and the tray while idle |
| `src/assets/brand/logo-128.png` | 128 × 128 | icon-full-1024.png | inside the app: the About page |
| `src/assets/brand/logo-256.png` | 256 × 256 | icon-full-1024.png | inside the app: the About page |

## Retired pictures

`retired.sha256` holds the fingerprints of pictures that must not come back: the first icon (a blue
square with a white dot) and the first tray glyph (a dot in a ring). `assets.py check` fails when a
tracked file, or a file inside a bundle given to it, has one of them.
