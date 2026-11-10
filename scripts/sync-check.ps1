# sync-check.ps1 - measure how far sound and picture are apart in a recording of scripts/sync-signal.ps1.
#
# How to use (needs a person, because it plays sound):
#   1. Start a recording in the app with the microphone on.
#   2. pwsh scripts/sync-signal.ps1            (a white square flashes in the middle of the screen with a beep)
#   3. Stop the recording, then:  pwsh scripts/sync-check.ps1 -File "<the recording>.mp4"
#
# The script finds every flash (jump in brightness in the centre of the picture) and every beep (1 kHz burst),
# pairs them and prints "sound minus picture" in milliseconds. Positive = the sound comes late.
# It ends in PASS when the median is within -Limit .. +Limit (100 ms by default), otherwise FAIL.
# The speakers, the room and the microphone add their own delay (a few tens of milliseconds); read the
# result with that in mind.
#
#   pwsh scripts/sync-check.ps1 -SelfTest      builds a synthetic clip with a known 150 ms delay and checks itself
#
# Works in Windows PowerShell 5.1 and PowerShell 7. Run from the repository root.
param(
  [string]$File = '',
  [int]$Limit = 100,
  [switch]$SelfTest
)
$ErrorActionPreference = 'Stop'
function Find-Tool([string]$name) {
  $sidecar = "src-tauri/binaries/$name-x86_64-pc-windows-msvc.exe"
  if (Test-Path $sidecar) { return $sidecar }
  if (Get-Command $name -ErrorAction SilentlyContinue) { return $name }
  Write-Host "FAIL: $name not found; run scripts/fetch-ffmpeg.ps1 first"; exit 1
}
$ff = Find-Tool 'ffmpeg'

# ffmpeg writes its report to stderr; collect both streams as plain text without tripping -ErrorAction Stop.
function Invoke-Ffmpeg([string[]]$ffArgs) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = (Resolve-Path $ff -ErrorAction SilentlyContinue | Select-Object -First 1)
  if (-not $psi.FileName) { $psi.FileName = $ff }
  $psi.Arguments = ($ffArgs | ForEach-Object { if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ } }) -join ' '
  $psi.UseShellExecute = $false; $psi.RedirectStandardOutput = $true; $psi.RedirectStandardError = $true; $psi.CreateNoWindow = $true
  $p = [System.Diagnostics.Process]::Start($psi)
  $errTask = $p.StandardError.ReadToEndAsync()
  $out = $p.StandardOutput.ReadToEnd()
  $p.WaitForExit()
  return @{ Out = $out; Err = $errTask.Result; Code = $p.ExitCode }
}
function ToNum([string]$s) { return [double]::Parse($s, [Globalization.CultureInfo]::InvariantCulture) }

if ($SelfTest) {
  New-Item -ItemType Directory -Force -Path tests/out | Out-Null
  $File = 'tests/out/sync-selftest.mp4'
  # Flashes at 2, 4, 6, 8 s; beeps 150 ms after each.
  $v = "color=c=black:s=640x360:r=30,drawbox=x=220:y=80:w=200:h=200:color=white:t=fill:enable='gte(t,2)*lt(mod(t,2),0.1)'"
  $a = "aevalsrc='0.5*sin(2*PI*1000*t)*gte(t,2.15)*lt(mod(t-0.15,2),0.1)':s=48000"
  $r = Invoke-Ffmpeg @('-hide_banner', '-loglevel', 'error', '-y', '-f', 'lavfi', '-i', $v, '-f', 'lavfi', '-i', $a, '-t', '10', '-c:v', 'mpeg4', '-q:v', '5', '-c:a', 'aac', $File)
  if ($r.Code -ne 0) { Write-Host "FAIL: cannot build the synthetic clip: $($r.Err)"; exit 1 }
  Write-Host 'self test: synthetic clip with the sound 150 ms after each flash'
}
if (-not $File -or -not (Test-Path $File)) { Write-Host 'FAIL: give a recording with -File, or use -SelfTest'; exit 1 }

# Picture: average brightness of the centre of each frame.
$r = Invoke-Ffmpeg @('-hide_banner', '-loglevel', 'error', '-i', $File, '-an', '-vf', 'crop=iw/8:ih/8,signalstats,metadata=print:key=lavfi.signalstats.YAVG:file=-', '-f', 'null', '-')
$times = New-Object System.Collections.Generic.List[double]; $luma = New-Object System.Collections.Generic.List[double]; $t = 0.0
foreach ($line in ($r.Out -split "`r?`n")) {
  if ($line -match 'pts_time:([0-9.]+)') { $t = ToNum $Matches[1] }
  elseif ($line -match 'YAVG=([0-9.]+)') { $times.Add($t); $luma.Add((ToNum $Matches[1])) }
}
if ($luma.Count -lt 10) { Write-Host 'FAIL: could not read the picture'; exit 1 }
# A flash is the black square turning white: a frame near white right after a frame near black. The desktop
# showing through before and after the signal window is neither, so it is not counted.
$flashes = @()
for ($i = 1; $i -lt $luma.Count; $i++) {
  if ($luma[$i] -ge 180 -and $luma[$i - 1] -le 90) { $flashes += $times[$i] }
}

# Sound: the 1 kHz beep, as the end of each silence in a narrow band around it.
$r = Invoke-Ffmpeg @('-hide_banner', '-nostats', '-i', $File, '-vn', '-af', 'bandpass=f=1000:width_type=h:w=300,silencedetect=noise=-38dB:d=0.3', '-f', 'null', '-')
$beeps = @()
foreach ($line in ($r.Err -split "`r?`n")) { if ($line -match 'silence_end: ([0-9.]+)') { $beeps += (ToNum $Matches[1]) } }

Write-Host ("flashes found: {0}   beeps found: {1}" -f $flashes.Count, $beeps.Count)
if ($flashes.Count -eq 0) { Write-Host 'FAIL: no flash in the picture (was sync-signal.ps1 on the recorded screen?)'; exit 1 }
if ($beeps.Count -eq 0) { Write-Host 'FAIL: no beep in the sound (microphone off, speakers muted, or too quiet)'; exit 1 }
$offsets = @()
foreach ($f in $flashes) {
  $near = $beeps | Sort-Object { [math]::Abs($_ - $f) } | Select-Object -First 1
  if ([math]::Abs($near - $f) -le 1.0) {
    $ms = [int][math]::Round(($near - $f) * 1000)
    $offsets += $ms
    Write-Host ("  flash at {0,7:N3} s   beep at {1,7:N3} s   sound minus picture {2,5} ms" -f $f, $near, $ms)
  }
}
if ($offsets.Count -eq 0) { Write-Host 'FAIL: no beep within one second of any flash'; exit 1 }
$s = $offsets | Sort-Object; $median = $s[[int](($s.Count - 1) / 2)]
Write-Host ("pairs: {0}   median: {1} ms   smallest: {2} ms   largest: {3} ms" -f $offsets.Count, $median, $s[0], $s[-1])
if ($SelfTest) {
  # Frame steps are 33 ms at 30 fps, so the known 150 ms may read as 117..183.
  if ([math]::Abs($median - 150) -le 40) { Write-Host 'PASS (self test: the known 150 ms was measured)'; exit 0 }
  Write-Host 'FAIL (self test: expected about 150 ms)'; exit 1
}
if ([math]::Abs($median) -le $Limit) { Write-Host "PASS (within $Limit ms)"; exit 0 }
Write-Host "FAIL (more than $Limit ms apart)"; exit 1
