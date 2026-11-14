# Microsoft Store variant (Windows)

`scripts/build-store-win.ps1` builds a second flavour of the Windows app: an MSIX package with package
identity, made to the Microsoft Store rules. The ordinary build (`npm run tauri build`, NSIS and MSI
installers) is unchanged. Both come from the same source; the differences are one cargo feature
(`msix`), the build flags, the package manifest (`src-tauri/msix/AppxManifest.xml`) and the ffmpeg
that goes inside.

Evidence for every "measured" below, and the sentences quoted from Microsoft's documentation:
`docs/reports/07-Windows-上架版重构-2026-11-12.md`.

## What is different

| Area | Ordinary build | Store variant |
|---|---|---|
| Output | NSIS `.exe` and `.msi` installers | one `.msix` (about 31 MB), not signed: the Store signs it on submission |
| Identity | none | package identity; `commands::package_identity()` returns the package full name |
| ffmpeg / ffprobe | whatever is in `src-tauri/binaries/` (in development a third-party LGPL 3 build, 114 MB each, with network protocols) | the build from `scripts/build-ffmpeg-win.sh`: ffmpeg.org source, LGPL 2.1, no network protocols, 30 MB each; the only external library is Intel's VPL dispatcher (MIT, static). The script refuses anything else |
| Video encoders | whatever that ffmpeg has (`hevc_qsv`, `hevc_nvenc`, …) | Intel QSV through VPL (`hevc_qsv`, `h264_qsv`), then Windows' own through Media Foundation: `hevc_mf`, `h264_mf` (hardware), then `h264_mf` in software, then `mpeg4` |
| DLLs the app imports | Windows system DLLs only (measured) | the same, and the script fails on anything else; `+crt-static` keeps the Visual C++ runtime out, so the package declares no framework dependency |
| Build-machine paths in the binary | 711 (measured) | 0: `--remap-path-prefix`, no `CARGO_MANIFEST_DIR` lookup; the script checks every file |
| WebView2 | the installer downloads the bootstrapper | none shipped: the manifest asks for Windows 11, where the runtime is part of the system |
| Licences | not bundled | `licenses/` in the package: ffmpeg notice (source URLs, hashes, configure line), LGPL 2.1 text, the VPL MIT licence, the app's MIT licence |
| Uninstall | the installer's uninstaller | Windows removes the package and its redirected app data. Recordings stay (see below) |

Runtime code asks `commands::package_identity()`; the only compile-time difference is
`sidecar::source_dir` (`feature = "msix"`, same switch as `mas`). Windows keeps its see-through windows:
they use public APIs there, so `windows::SEE_THROUGH` stays tied to `mas` alone.

Three changes made for this variant apply to every Windows build, because they were bugs or habits a
store package should not have:

- **Media Foundation encoders get NV12 and a delayed `moov`.** The hardware encoders turned away planar
  4:2:0 on the first frame, and a segment written with `empty_moov` could not be opened afterwards.
  `encoders::input_pix_fmt` and `encoders::segment_movflags` hold the two rules.
- **A software H.264 step** (`h264_mf` with hardware off) sits in the Windows chain before `mpeg4`.
- **No system utilities are started.** The session folder is hidden through `SetFileAttributesW`
  instead of `attrib.exe`; the privacy settings page opens through the shell instead of `explorer.exe`.

## Capabilities and why each is there

| Capability | Reason |
|---|---|
| `runFullTrust` (restricted) | required for a packaged desktop app at medium integrity. The recorder is a Win32 program that starts `ffmpeg.exe` from its own package and reads the screen through DXGI desktop duplication. The Store asks for a justification at submission |
| `webcam` | camera picture-in-picture and self-view |
| `microphone` | the narration track |

Nothing else is declared. A medium-integrity app needs no capability for the network (the optional
upload to the user's own Google Drive) or for the Videos folder, and desktop duplication has none.
`webcam` and `microphone` are not strictly required at medium integrity; Microsoft recommends them so the
app appears in the privacy settings pages from the moment it is installed.

## Where things live under package identity

| What | Path the app asks for | Where it really goes |
|---|---|---|
| Recordings | `Videos\FlowGT Recorder` | the same folder; not redirected |
| Session folders (segments, manifest) | `Videos\FlowGT Recorder\.flowgt-recorder` | the same folder, hidden |
| A save folder picked in Settings | whatever the user picked | the same folder (full trust: no broker, no bookmark) |
| Settings, Google sign-in, WebView2 data | `%APPDATA%\nz.co.flowgt.recorder`, `%LOCALAPPDATA%\nz.co.flowgt.recorder` | new files are redirected to the package's private store and removed on uninstall |
| Temporary files | `%TEMP%` | under `%LOCALAPPDATA%`; whether Windows redirects it was not looked up |

The interface shows only the save folder, which is a real path. This table follows Microsoft's
description of packaged desktop apps; it has not been observed on an installed package (see Known gaps).

## Build

```sh
# once, on Linux, WSL or a Mac with mingw-w64; about ten minutes
bash scripts/build-ffmpeg-win.sh
```

```powershell
# on Windows: Node, Rust (MSVC), Windows SDK, Visual Studio build tools
powershell -ExecutionPolicy Bypass -File scripts/build-store-win.ps1
```

Output in `src-tauri/target/store-win/`: `layout/` (the package contents), the `.msix`, and
`package-check.txt` (what was checked and found).

For an upload, pass the identity Partner Center shows under *Product identity*:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-store-win.ps1 `
  -IdentityName "<Package/Identity/Name>" -Publisher "<Package/Identity/Publisher>" `
  -PublisherDisplayName "<Package/Properties/PublisherDisplayName>"
```

and set `version` in `src-tauri/tauri.conf.json` to 1.0.0 or higher first: the Store does not take a
version that starts with 0.

For a local install without a certificate, turn on Developer Mode in Windows Settings and register the
package contents in place (no elevation needed):

```powershell
Add-AppxPackage -Register src-tauri/target/store-win/layout/AppxManifest.xml
```

The app then runs from `layout/` with package identity; do not delete or rebuild that folder while it is
registered. An unsigned `.msix` is not a route for this app: `Add-AppxPackage -AllowUnsigned` refuses any
package with an executable in it, elevated or not ("an unsigned package cannot include Executable
activations", `0x80073D2B`, seen 2026-11-12).

## Known gaps

- **Installed only by registering the layout** (Developer Mode, `Add-AppxPackage -Register`), never
  from the `.msix` file: that needs a signature (the Store's, on submission). Measured with package
  identity on 2026-11-14 (`docs/reports/11-Windows-通宵测试与优化-2026-11-14.md`): start, the startup
  task, the global shortcuts, scrolling screenshots, recording without sound, settings in the
  package's private `LocalCache`, screenshots and recordings in the real Videos folder.
- **Media Foundation hardware encoding crashes inside the package** (measured, Intel graphics driver
  26.8.7.5603): any `ffmpeg.exe` that lies in the package folder crashes in the driver's
  `mfx_mft_h265ve_64.dll` (0xc0000005) when it opens `hevc_mf` or `h264_mf` in hardware mode. The
  same file outside the package folder, with package identity, works, and so does Intel QSV inside
  the folder; turning off registry and file write virtualization does not help. Since 2026-11-14 our
  ffmpeg carries Intel's VPL dispatcher, and the app picks `hevc_qsv` before `hevc_mf`: inside the
  package it recorded HEVC at 58 % of one core at 1080p30, where software H.264 took about 80 %. On a
  PC without Intel graphics QSV fails its test and the app falls back to Media Foundation; whether
  that path crashes in a package with NVIDIA or AMD drivers is not known (one machine, Intel only).
- **The first microphone or camera use asks** ("Let FlowGT Recorder access your microphone?"). The
  device waits for the answer; after 12 s the start ends with `error.consentPending`, which tells
  the user to answer the question. Not tested past that point: nobody answered on the test machine.
- **Not submitted to Partner Center.** No account was available. The identity values are placeholders.
- **Windows App Certification Kit not run**: it installs the package first.
- **Computer sound is not recorded on Windows**, in this variant or the ordinary one. ffmpeg has no
  WASAPI loopback input; the plan is the native engine (`docs/tasks/W-6.md`).
- **Windows 11 only.** A Windows 10 package would have to bring or require the WebView2 Runtime.
- **x64 only.**
- **Software H.264 overshoots the size budget**; the package falls back to it only on a PC where neither QSV nor Media Foundation hardware encoding works. Hardware HEVC measured 872 kbps against the 889 kbps
  budget; the Media Foundation software encoder measured 735 to 919 kbps on ten seconds of a still
  screen, depending on the rate-control mode, and was not measured on a long or busy recording.
- **Smart App Control.** On a PC with Smart App Control on, an unsigned `ffmpeg.exe` built from source
  may be blocked ("An Application Control policy has blocked this file"): the first `ffmpeg.exe` built
  here was, while the `ffprobe.exe` of the same build and both files of the final build were allowed. Store-signed packages are not affected; an ordinary installer with
  an unsigned ffmpeg is. Sign the tools for any distribution outside the Store.
- The licence texts ship in the package but are not shown in the app.
- `THIRD-PARTY-NOTICES` for the Rust and npm dependencies is still not generated (`docs/LICENSES.md`).
