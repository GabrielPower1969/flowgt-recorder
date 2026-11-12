# Bench: encoder speed, CPU and working set on synthetic input, plus MB/h.
# Run from the repo root: pwsh scripts/bench.ps1 [-Seconds 20]
param([int]$Seconds = 20)
$ErrorActionPreference = 'Stop'
# Uses the sidecar from scripts/fetch-ffmpeg.ps1 when it is there (the binary the app runs), otherwise ffmpeg on PATH.
# First run on Windows: 2026-11-11 (docs/BENCH.md -> Windows).
$sidecar = 'src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe'
$ff = if (Test-Path $sidecar) { $sidecar } elseif (Get-Command ffmpeg -ErrorAction SilentlyContinue) { 'ffmpeg' } else { Write-Host 'FAIL: ffmpeg not found; run scripts/fetch-ffmpeg.ps1 first'; exit 1 }
Write-Host "ffmpeg=$ff"
New-Item -ItemType Directory -Force -Path bench-out | Out-Null
$have = (& $ff -hide_banner -encoders) -join "`n"
$extra = @{
  hevc_nvenc = '-preset p4 -tune ll -rc vbr -tag:v hvc1'; hevc_qsv = '-preset veryfast -tag:v hvc1'
  hevc_amf = '-usage lowlatency -tag:v hvc1'; hevc_mf = '-hw_encoding 1 -tag:v hvc1'
  h264_nvenc = '-preset p4 -tune ll -rc vbr'; h264_qsv = '-preset veryfast'
  h264_amf = '-usage lowlatency'; h264_mf = '-hw_encoding 1'; libx264 = '-preset veryfast -tune zerolatency' }
$chain = 'hevc_nvenc','hevc_qsv','hevc_amf','hevc_mf','h264_nvenc','h264_qsv','h264_amf','h264_mf','libx264'
$rows = @(); $fail = $false
foreach ($enc in $chain) {
  if ($have -notmatch "\s$enc\s") { continue }
  $out = "bench-out/bench-$enc.mp4"
  $a = @('-hide_banner','-loglevel','error','-f','lavfi','-i','testsrc2=size=1920x1080:rate=30','-f','lavfi','-i','sine=frequency=440:sample_rate=48000',
    '-map','0:v','-map','1:a','-c:v',$enc) + ($extra[$enc] -split ' ') +
    @('-b:v','793k','-maxrate','1190k','-bufsize','1586k','-g','60','-pix_fmt','yuv420p','-r','30','-c:a','aac','-b:a','96k','-t',"$Seconds",'-y',$out)
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $p = Start-Process -FilePath $ff -ArgumentList $a -PassThru -NoNewWindow
  $peakWs = 0; $cpu0 = 0
  while (-not $p.HasExited) {
    Start-Sleep -Milliseconds 500
    try { $p.Refresh(); $peakWs = [math]::Max($peakWs, $p.WorkingSet64); $cpu0 = $p.TotalProcessorTime.TotalSeconds } catch {}
  }
  $p.WaitForExit(); $sw.Stop()
  if ($p.ExitCode -ne 0 -or -not (Test-Path $out)) { Write-Host "encoder $enc failed (exit $($p.ExitCode))"; continue }
  $mbh = (Get-Item $out).Length / 1MB * 3600 / $Seconds
  $rows += [pscustomobject]@{ encoder = $enc; wall_s = [math]::Round($sw.Elapsed.TotalSeconds,1)
    speed_x = [math]::Round($Seconds / $sw.Elapsed.TotalSeconds,2); cpu_s = [math]::Round($cpu0,1)
    cpu_pct_of_1core = [math]::Round(100 * $cpu0 / $sw.Elapsed.TotalSeconds); peak_ws_mb = [math]::Round($peakWs/1MB)
    mb_per_hour = [math]::Round($mbh) }
}
$rows | Format-Table -AutoSize | Out-String | Write-Host
$rows | ConvertTo-Json | Set-Content bench-out/bench-win.json
if ($rows.Count -eq 0) { Write-Host 'FAIL: no encoder ran'; exit 1 }
foreach ($r in $rows) { if ($r.speed_x -lt 1.0 -or $r.mb_per_hour -gt 480) { $fail = $true; Write-Host "over budget: $($r.encoder)" } }
if ($fail) { Write-Host 'FAIL'; exit 1 }
Write-Host 'PASS'
