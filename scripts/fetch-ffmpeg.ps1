# fetch-ffmpeg.ps1 - put LGPL ffmpeg/ffprobe sidecars in src-tauri/binaries/ (Windows x64).
#
# The PowerShell twin of scripts/fetch-ffmpeg.sh. Unlike the macOS script (which copies the GPL
# Homebrew build for development only), this one fetches an LGPL build, because that is what a
# Windows release has to ship (docs/LICENSES.md). The app crate does not compile without these
# files: tauri-build checks that every `bundle.externalBin` entry exists.
#
# Run from anywhere:
#   pwsh scripts/fetch-ffmpeg.ps1                 download the pinned build, verify SHA-256, install
#   pwsh scripts/fetch-ffmpeg.ps1 -FromDir <dir>  use ffmpeg.exe / ffprobe.exe from <dir>, no download
#   pwsh scripts/fetch-ffmpeg.ps1 -Force          replace binaries that are already there
# Works in Windows PowerShell 5.1 and PowerShell 7. Prints a final PASS / FAIL line.
param(
  [string]$FromDir = '',
  [switch]$Force
)
$ErrorActionPreference = 'Stop'

# Pinned build: BtbN FFmpeg-Builds, LGPL static variant, 9.0 release branch, 2026-08-31.
# To move to a newer build, change both lines together and re-run scripts/integration.ps1.
$Url = 'https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-31-13-27/ffmpeg-n9.0.1-11-ge47273f4d9-win64-lgpl-9.0.zip'
$Sha256 = '2484854ad6988d34560f4e6ea7a6ecb9dde0af7c229d2591815d056b04ec4f56'

$Triple = 'x86_64-pc-windows-msvc'
$Tools = 'ffmpeg', 'ffprobe'
$Root = Split-Path -Parent $PSScriptRoot
$Dest = Join-Path $Root 'src-tauri/binaries'

function Fail([string]$msg) { Write-Host "FAIL: $msg"; exit 1 }

# An LGPL build prints the LGPL from -L and has none of these switches in its configure line.
function Test-Lgpl([string]$exe) {
  $license = (& $exe -hide_banner -L) -join "`n"
  if ($license -notmatch 'GNU Lesser General Public License') { return "$exe -L does not print the LGPL" }
  $config = (& $exe -version | Where-Object { $_ -like 'configuration:*' }) -join ' '
  foreach ($flag in '--enable-gpl', '--enable-nonfree', '--enable-libx264', '--enable-libx265') {
    if ($config -match [regex]::Escape($flag)) { return "$exe is configured with $flag" }
  }
  return ''
}

function Show-Installed {
  foreach ($tool in $Tools) {
    $p = Join-Path $Dest "$tool-$Triple.exe"
    $ver = (& $p -version | Select-Object -First 1)
    $hash = (Get-FileHash -Algorithm SHA256 $p).Hash.ToLower()
    Write-Host "src-tauri/binaries/$tool-$Triple.exe"
    Write-Host "  $ver"
    Write-Host "  sha256 $hash"
  }
}

if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { Fail "only x64 Windows is supported, this is $($env:PROCESSOR_ARCHITECTURE)" }
New-Item -ItemType Directory -Force -Path $Dest | Out-Null

$present = @($Tools | Where-Object { Test-Path (Join-Path $Dest "$_-$Triple.exe") })
if ($present.Count -eq $Tools.Count -and -not $Force) {
  foreach ($tool in $Tools) {
    $why = Test-Lgpl (Join-Path $Dest "$tool-$Triple.exe")
    if ($why) { Fail "$why (run again with -Force to replace it)" }
  }
  Write-Host 'already present (use -Force to replace):'
  Show-Installed
  Write-Host 'PASS'
  exit 0
}

$work = Join-Path ([IO.Path]::GetTempPath()) ("flowgt-fetch-ffmpeg-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $work | Out-Null
try {
  if ($FromDir) {
    foreach ($tool in $Tools) {
      $src = Join-Path $FromDir "$tool.exe"
      if (-not (Test-Path $src)) { Fail "missing $tool.exe in the -FromDir folder" }
      Copy-Item -Force $src (Join-Path $work "$tool.exe")
    }
    Write-Host 'using the binaries from -FromDir (no download)'
  } else {
    $zip = Join-Path $work 'ffmpeg.zip'
    Write-Host "downloading $Url"
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $ProgressPreference = 'SilentlyContinue'   # the progress bar makes 5.1 downloads many times slower
    Invoke-WebRequest -Uri $Url -OutFile $zip -UseBasicParsing
    $got = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
    if ($got -ne $Sha256) { Fail "SHA-256 mismatch: expected $Sha256, got $got" }
    Write-Host "sha256 ok ($got)"
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($zip)
    try {
      foreach ($tool in $Tools) {
        $entry = $archive.Entries | Where-Object { $_.FullName -like "*/bin/$tool.exe" } | Select-Object -First 1
        if (-not $entry) { Fail "bin/$tool.exe is not in the archive" }
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $work "$tool.exe"), $true)
      }
    } finally { $archive.Dispose() }
  }

  # Check the licence before anything lands in src-tauri/binaries/.
  foreach ($tool in $Tools) {
    $why = Test-Lgpl (Join-Path $work "$tool.exe")
    if ($why) { Fail ($why -replace [regex]::Escape($work), '<temp>') }
  }
  foreach ($tool in $Tools) {
    Copy-Item -Force (Join-Path $work "$tool.exe") (Join-Path $Dest "$tool-$Triple.exe")
  }
} finally {
  Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}

Write-Host 'installed (LGPL confirmed: -L prints the LGPL, no --enable-gpl / --enable-nonfree / libx264 / libx265):'
Show-Installed
Write-Host 'PASS'
