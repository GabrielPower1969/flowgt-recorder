# build-store-win.ps1 - build the Microsoft Store variant of the Windows app: an MSIX package.
#
# What is different from `npm run tauri build` (which is unchanged and still makes the NSIS / MSI
# installers):
#   * the output is an MSIX package with package identity (src-tauri/msix/AppxManifest.xml);
#   * ffmpeg.exe / ffprobe.exe must be the self-contained LGPL 2.1 build from
#     scripts/build-ffmpeg-win.sh; this script refuses anything else;
#   * the app is built with `--features msix` and a static C runtime, so the package needs no
#     Visual C++ framework package and holds no path of the build machine;
#   * the ffmpeg licence, its notice and the app licence travel inside the package.
#
# The package is NOT signed. The Microsoft Store signs MSIX packages itself on submission, so an
# upload needs no certificate; what it needs is the identity Partner Center gives the app:
#   -IdentityName          Package/Identity/Name            e.g. 12345Publisher.FlowGTRecorder
#   -Publisher             Package/Identity/Publisher       e.g. CN=01234567-89AB-CDEF-0123-456789ABCDEF
#   -PublisherDisplayName  Package/Properties/PublisherDisplayName
# Without them the script uses placeholder values: good for checking that the package builds, not
# for an upload.
#
#   -SkipBuild   reuse the app already built by an earlier run.
#   -FfmpegDir   take ffmpeg / ffprobe, the notice and the licence text from this folder instead of
#                src-tauri/binaries (see FFMPEG_OUT_DIR in scripts/build-ffmpeg-win.sh).
#
# On a machine where Windows refuses to start an unsigned program (Smart App Control), ffmpeg.exe
# cannot be asked for its version. The script then reads the configure line out of the file itself
# and says so in package-check.txt; the checks on that line are the same.
#
# Run from anywhere:  powershell -ExecutionPolicy Bypass -File scripts/build-store-win.ps1
# Works in Windows PowerShell 5.1 and PowerShell 7. Needs Node, Rust (MSVC), the Windows SDK
# (makeappx, makepri) and the Visual Studio build tools (dumpbin). Prints a final PASS / FAIL line.
# Output: src-tauri/target/store-win/  (layout/, the .msix, package-check.txt)
param(
  [string]$IdentityName = 'FlowGT.Recorder',
  [string]$Publisher = 'CN=FlowGT-Local-Test',
  [string]$PublisherDisplayName = 'FlowGT',
  [string]$DisplayName = 'FlowGT Recorder',
  [switch]$SkipBuild,
  [string]$FfmpegDir = ''
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$Triple = 'x86_64-pc-windows-msvc'
$Root = Split-Path -Parent $PSScriptRoot
$Bin = if ($FfmpegDir) { (Resolve-Path $FfmpegDir).Path } else { Join-Path $Root 'src-tauri/binaries' }
$Out = Join-Path $Root 'src-tauri/target/store-win'
$Layout = Join-Path $Out 'layout'
$CargoTarget = Join-Path $Out 'cargo'
$Report = Join-Path $Out 'package-check.txt'

function Fail([string]$msg) { Write-Host "FAIL: $msg"; exit 1 }
function Note([string]$msg) { Write-Host $msg; Add-Content -Path $Report -Value $msg -Encoding UTF8 }

function Find-SdkTool([string]$name) {
  $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
  $hit = Get-ChildItem -Path $kits -Filter $name -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName -Descending | Select-Object -First 1
  if (-not $hit) { Fail "$name was not found in the Windows SDK ($kits)" }
  return $hit.FullName
}

function Find-Dumpbin {
  $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
  if (-not (Test-Path $vswhere)) { Fail 'vswhere.exe was not found (install the Visual Studio build tools)' }
  $vs = & $vswhere -latest -products * -property installationPath
  $hit = Get-ChildItem -Path (Join-Path $vs 'VC/Tools/MSVC') -Filter dumpbin.exe -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match 'Hostx64\\x64' } | Select-Object -First 1
  if (-not $hit) { Fail 'dumpbin.exe was not found in the Visual Studio build tools' }
  return $hit.FullName
}

# DLL names a PE file imports.
function Get-Imports([string]$dumpbin, [string]$pe) {
  $names = @()
  foreach ($line in (& $dumpbin /nologo /dependents $pe)) {
    $t = $line.Trim()
    if ($t -match '^[A-Za-z0-9_.\-]+\.dll$') { $names += $t.ToLower() }
  }
  return $names
}

# Every import must be a DLL that Windows itself provides. The Visual C++ runtime is not one of
# them: it is on most machines but not part of the system, and a package would have to declare it.
function Test-Imports([string]$dumpbin, [string]$pe) {
  $bad = @()
  foreach ($dll in (Get-Imports $dumpbin $pe)) {
    if ($dll -like 'api-ms-*' -or $dll -like 'ext-ms-*') { continue }
    if ($dll -like 'vcruntime*' -or $dll -like 'msvcp*' -or $dll -like 'vccorlib*' -or $dll -like 'concrt*' -or $dll -eq 'ucrtbased.dll') { $bad += $dll; continue }
    if (-not (Test-Path (Join-Path $env:windir "System32/$dll"))) { $bad += $dll }
  }
  return $bad
}

# Strings that would give away the machine the package was built on, as bytes in both encodings.
function Find-Leaks([string]$file, [string[]]$needles) {
  $latin = [Text.Encoding]::GetEncoding(28591)
  $text = $latin.GetString([IO.File]::ReadAllBytes($file))
  $found = @()
  foreach ($n in $needles) {
    if (-not $n) { continue }
    $wide = $latin.GetString([Text.Encoding]::Unicode.GetBytes($n))
    foreach ($probe in $n, $wide) {
      if ($text.IndexOf($probe, [StringComparison]::OrdinalIgnoreCase) -ge 0) { $found += $n; break }
    }
  }
  return $found
}

# What an ffmpeg build says about itself: asked of the program, or, when Windows will not start it,
# read from the configure line and version string that every ffmpeg binary carries.
function Get-FfmpegFacts([string]$exe) {
  $facts = @{ ran = $false; version = ''; config = ''; license = ''; protocols = @() }
  try {
    $out = (& $exe -hide_banner -version 2>$null) -join "`n"
    if ($LASTEXITCODE -eq 0 -and $out) {
      $facts.ran = $true
      $facts.version = ($out -split "`n")[0]
      $facts.config = (($out -split "`n" | Where-Object { $_ -like 'configuration:*' }) -join ' ') -replace '^configuration:\s*', ''
      $facts.license = (& $exe -hide_banner -L 2>$null) -join ' '
      if ((Split-Path -Leaf $exe) -like 'ffmpeg*') {
        $facts.protocols = @((& $exe -hide_banner -protocols 2>$null) | Where-Object { $_ -match '^\s+\S+$' } | ForEach-Object { $_.Trim() } | Sort-Object -Unique)
      }
      return $facts
    }
  } catch { }
  $text = [Text.Encoding]::GetEncoding(28591).GetString([IO.File]::ReadAllBytes($exe))
  $m = [regex]::Match($text, '--prefix=[\x20-\x7e]+')
  if (-not $m.Success) { Fail "$exe could not be started and holds no configure line" }
  $facts.config = $m.Value
  $v = [regex]::Match($text, 'FFmpeg version ([0-9][\x21-\x7e]*)')
  $facts.version = if ($v.Success) { "ffmpeg version $($v.Groups[1].Value) (read from the file; Windows did not start it)" } else { 'version unknown (Windows did not start the file)' }
  return $facts
}

function Save-Png([string]$source, [int]$size, [string]$dest) {
  $src = [Drawing.Image]::FromFile($source)
  $bmp = New-Object Drawing.Bitmap $size, $size, ([Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [Drawing.Graphics]::FromImage($bmp)
  $g.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $g.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $g.CompositingQuality = [Drawing.Drawing2D.CompositingQuality]::HighQuality
  $g.DrawImage($src, 0, 0, $size, $size)
  $bmp.Save($dest, [Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose(); $src.Dispose()
}

if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { Fail "only x64 Windows is supported, this is $($env:PROCESSOR_ARCHITECTURE)" }
New-Item -ItemType Directory -Force -Path $Out | Out-Null
Set-Content -Path $Report -Value "FlowGT Recorder, Microsoft Store variant: package check" -Encoding UTF8
$makeappx = Find-SdkTool 'makeappx.exe'
$makepri = Find-SdkTool 'makepri.exe'
$dumpbin = Find-Dumpbin

# 1. The sidecars must be shippable before anything is built around them.
Note ''
Note '== ffmpeg / ffprobe =='
foreach ($tool in 'ffmpeg', 'ffprobe') {
  $exe = Join-Path $Bin "$tool-$Triple.exe"
  if (-not (Test-Path $exe)) { Fail "$tool-$Triple.exe is missing from the ffmpeg folder. Run scripts/build-ffmpeg-win.sh" }
  $facts = Get-FfmpegFacts $exe
  $config = $facts.config
  foreach ($flag in '--enable-gpl', '--enable-nonfree', '--enable-version3') {
    if ($config -match [regex]::Escape($flag)) { Fail "$tool is configured with $flag. Run scripts/build-ffmpeg-win.sh" }
  }
  if ($config -notmatch '--disable-network') { Fail "$tool is built with network protocols. Run scripts/build-ffmpeg-win.sh" }
  if ($config -match '--enable-lib') { Fail "$tool is built with an external library. Run scripts/build-ffmpeg-win.sh" }
  if ($facts.ran -and ($facts.license -notmatch 'GNU Lesser General Public License' -or $facts.license -notmatch 'version 2\.1')) { Fail "$tool -L does not print the LGPL 2.1" }
  $network = @($facts.protocols | Where-Object { $_ -match '^(http|https|tcp|udp|tls|rtp|rtmp.*|ftp|srt|sftp|rist|zmq|gopher.*|mms.*|icecast|httpproxy|udplite|srtp|dtls|ipfs|ipns|prompeg)$' })
  if ($network.Count) { Fail "$tool still has network protocols: $($network -join ', ')" }
  $bad = @(Test-Imports $dumpbin $exe)
  if ($bad.Count) { Fail "$tool imports DLLs that are not part of Windows: $($bad -join ', '). Run scripts/build-ffmpeg-win.sh" }
  Note $facts.version
  Note "  configure: $config"
  if ($facts.ran) { if ($facts.protocols.Count) { Note "  protocols: $($facts.protocols -join ' ')" } }
  else { Note '  NOT RUN: Windows refused to start this file (unsigned; Smart App Control). Licence and protocols are judged from the configure line only.' }
  Note "  imports: $((Get-Imports $dumpbin $exe) -join ' ')"
}
foreach ($f in 'ffmpeg-NOTICE.txt', 'ffmpeg-COPYING.LGPLv2.1.txt') {
  if (-not (Test-Path (Join-Path $Bin $f))) { Fail "$f is missing from the ffmpeg folder. Run scripts/build-ffmpeg-win.sh" }
}

# 2. Build the app. A static C runtime, and this machine's folder names kept out of the binary.
$exeBuilt = Join-Path $CargoTarget 'release/flowgt-recorder.exe'
if (-not $SkipBuild) {
  $manifestBackup = Join-Path $Out 'Cargo.toml.before-store'
  Copy-Item (Join-Path $Root 'src-tauri/Cargo.toml') $manifestBackup -Force
  $oldFlags = $env:RUSTFLAGS; $oldTarget = $env:CARGO_TARGET_DIR
  try {
    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
    $rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
    $env:RUSTFLAGS = "$oldFlags -C target-feature=+crt-static --remap-path-prefix=$Root=C:\build\app --remap-path-prefix=$cargoHome=C:\build\cargo --remap-path-prefix=$rustupHome=C:\build\rustup --remap-path-prefix=$($env:USERPROFILE)=C:\build\home".Trim()
    $env:CARGO_TARGET_DIR = $CargoTarget
    Push-Location $Root
    & npm run tauri build -- --no-bundle --features msix
    $code = $LASTEXITCODE
    Pop-Location
    if ($code -ne 0) { Fail "tauri build ended with exit code $code" }
  } finally {
    $env:RUSTFLAGS = $oldFlags; $env:CARGO_TARGET_DIR = $oldTarget
    Copy-Item $manifestBackup (Join-Path $Root 'src-tauri/Cargo.toml') -Force
  }
}
if (-not (Test-Path $exeBuilt)) { Fail "the app was not built: $exeBuilt is missing" }

# 3. Lay the package out.
if (Test-Path $Layout) { Remove-Item -Recurse -Force $Layout }
New-Item -ItemType Directory -Force -Path (Join-Path $Layout 'Assets'), (Join-Path $Layout 'licenses') | Out-Null
Copy-Item $exeBuilt (Join-Path $Layout 'flowgt-recorder.exe')
foreach ($tool in 'ffmpeg', 'ffprobe') { Copy-Item (Join-Path $Bin "$tool-$Triple.exe") (Join-Path $Layout "$tool.exe") }
Copy-Item (Join-Path $Bin 'ffmpeg-NOTICE.txt') (Join-Path $Layout 'licenses/ffmpeg-NOTICE.txt')
Copy-Item (Join-Path $Bin 'ffmpeg-COPYING.LGPLv2.1.txt') (Join-Path $Layout 'licenses/ffmpeg-COPYING.LGPLv2.1.txt')
Copy-Item (Join-Path $Root 'LICENSE') (Join-Path $Layout 'licenses/FlowGT-Recorder-LICENSE.txt')

# Logos: the simple drawing for 50 px and smaller, the full one above (src-tauri/icons/README.md).
$full = Join-Path $Root 'src-tauri/icons/icon-src.png'
$small = Join-Path $Root 'src-tauri/icons/icon-src-small.png'
if (-not (Test-Path $small)) { $small = $full }
$assets = Join-Path $Layout 'Assets'
foreach ($scale in 100, 200, 400) {
  foreach ($logo in @{ n = 'Square44x44Logo'; s = 44 }, @{ n = 'Square150x150Logo'; s = 150 }, @{ n = 'StoreLogo'; s = 50 }) {
    $px = [int]($logo.s * $scale / 100)
    Save-Png $(if ($px -le 50) { $small } else { $full }) $px (Join-Path $assets "$($logo.n).scale-$scale.png")
  }
}
foreach ($px in 16, 24, 32, 48, 256) {
  Save-Png $(if ($px -le 50) { $small } else { $full }) $px (Join-Path $assets "Square44x44Logo.targetsize-${px}_altform-unplated.png")
  Save-Png $(if ($px -le 50) { $small } else { $full }) $px (Join-Path $assets "Square44x44Logo.targetsize-$px.png")
}

# The manifest. A four-part version whose last part is 0: the Store keeps that part for itself.
$conf = Get-Content (Join-Path $Root 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
$version = "$($conf.version).0"
if ($version -notmatch '^[1-9]\d*\.\d+\.\d+\.0$' -and $version -notmatch '^0\.\d+\.\d+\.0$') { Fail "version $version is not a.b.c.0" }
if ($version -match '^0\.') { Note ''; Note "note: version $version starts with 0. The Store does not accept that; set a version of 1.0.0 or higher in tauri.conf.json before an upload." }
$publisherValue = $Publisher
$xml = Get-Content (Join-Path $Root 'src-tauri/msix/AppxManifest.xml') -Raw
foreach ($pair in @{ k = 'IDENTITY_NAME'; v = $IdentityName }, @{ k = 'VERSION'; v = $version }, @{ k = 'PUBLISHER'; v = $publisherValue },
    @{ k = 'PUBLISHER_DISPLAY_NAME'; v = $PublisherDisplayName }, @{ k = 'DISPLAY_NAME'; v = $DisplayName }) {
  $xml = $xml.Replace("{{$($pair.k)}}", [Security.SecurityElement]::Escape($pair.v))
}
if ($xml -match '\{\{') { Fail 'the manifest template still has an unfilled field' }
[IO.File]::WriteAllText((Join-Path $Layout 'AppxManifest.xml'), $xml, (New-Object Text.UTF8Encoding $false))

# 4. Check what is about to ship.
Note ''
Note '== package contents =='
$appBad = @(Test-Imports $dumpbin (Join-Path $Layout 'flowgt-recorder.exe'))
if ($appBad.Count) { Fail "flowgt-recorder.exe imports DLLs that are not part of Windows: $($appBad -join ', ')" }
Note "flowgt-recorder.exe imports: $((Get-Imports $dumpbin (Join-Path $Layout 'flowgt-recorder.exe')) -join ' ')"
$user = $env:USERNAME
$needles = @($env:USERPROFILE, $env:USERPROFILE.Replace('\', '/'), "\Users\$user\", "/Users/$user/", "/home/", "/mnt/c/", $Root, $Root.Replace('\', '/'))
$leaks = 0
foreach ($f in Get-ChildItem -Path $Layout -Recurse -File) {
  $found = @(Find-Leaks $f.FullName $needles)
  $rel = $f.FullName.Substring($Layout.Length + 1)
  if ($found.Count) { $leaks++; Note "LEAK  $rel  contains: $($found -join ' | ')" }
  else { Note ("ok    {0,-58} {1,11:N0} bytes" -f $rel, $f.Length) }
}
if ($leaks) { Fail "$leaks file(s) contain a path of this machine (see package-check.txt)" }
Note "no file contains a path of the build machine (looked for the home folder, the user folder, the repository folder, /home/ and /mnt/c/)"

# 5. Resource index (lets Windows pick the logo for the scale and size it needs), then the package.
$priConfig = Join-Path $Out 'priconfig.xml'
& $makepri createconfig /cf $priConfig /dq en-US /o | Out-Null
if ($LASTEXITCODE -ne 0) { Fail 'makepri createconfig failed' }
& $makepri new /pr $Layout /cf $priConfig /mn (Join-Path $Layout 'AppxManifest.xml') /of (Join-Path $Layout 'resources.pri') /o | Out-Null
if ($LASTEXITCODE -ne 0) { Fail 'makepri new failed' }

$msix = Join-Path $Out "FlowGT-Recorder_${version}_x64.msix"
$packLog = & $makeappx pack /d $Layout /p $msix /o
if ($LASTEXITCODE -ne 0) { $packLog | Select-Object -Last 15 | ForEach-Object { Write-Host $_ }; Fail 'makeappx pack failed (the manifest did not validate, or a file is missing)' }

Note ''
Note '== result =='
Note "identity: Name=$IdentityName  Publisher=$publisherValue  Version=$version"
Note ("package:  {0}  {1:N0} bytes" -f (Split-Path -Leaf $msix), (Get-Item $msix).Length)
Note "sha256:   $((Get-FileHash -Algorithm SHA256 $msix).Hash.ToLower())"
if ($IdentityName -eq 'FlowGT.Recorder') { Note 'identity is the placeholder: pass -IdentityName, -Publisher and -PublisherDisplayName from Partner Center for an upload' }
Write-Host "PASS: $msix"
