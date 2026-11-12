# Integration test: synthetic lavfi inputs -> app-style argv -> MP4 -> ffprobe assertions.
# Run from the repo root: pwsh scripts/integration.ps1 [-Seconds 6]
# Uses the sidecars from scripts/fetch-ffmpeg.ps1 when they are there (the binaries the app runs),
# otherwise ffmpeg / ffprobe on PATH. First run on Windows: 2026-11-11 (docs/tasks/W-2.md).
param([int]$Seconds = 6)
$ErrorActionPreference = 'Stop'
function Find-Tool([string]$name) {
  $sidecar = "src-tauri/binaries/$name-x86_64-pc-windows-msvc.exe"
  if (Test-Path $sidecar) { return $sidecar }
  if (Get-Command $name -ErrorAction SilentlyContinue) { return $name }
  Write-Host "FAIL: $name not found; run scripts/fetch-ffmpeg.ps1 first"; exit 1
}
$ff = Find-Tool 'ffmpeg'
$fp = Find-Tool 'ffprobe'
Write-Host "ffmpeg=$ff ffprobe=$fp"
New-Item -ItemType Directory -Force -Path tests/out | Out-Null
$out = 'tests/out/integration-win.mp4'
$have = (& $ff -hide_banner -encoders) -join "`n"
$extra = @{
  hevc_nvenc = '-preset p4 -tune ll -rc vbr -tag:v hvc1'; hevc_qsv = '-preset veryfast -tag:v hvc1'
  hevc_amf = '-usage lowlatency -tag:v hvc1'; hevc_mf = '-hw_encoding 1 -tag:v hvc1'
  h264_nvenc = '-preset p4 -tune ll -rc vbr'; h264_qsv = '-preset veryfast'
  h264_amf = '-usage lowlatency'; h264_mf = '-hw_encoding 1'; libx264 = '-preset veryfast -tune zerolatency' }
$chain = 'hevc_nvenc','hevc_qsv','hevc_amf','hevc_mf','h264_nvenc','h264_qsv','h264_amf','h264_mf','libx264'
$fc = "[0:v]scale=1920:1080:flags=bicubic[scr];[1:v]crop=ih:ih,hflip,scale=236:236:flags=bicubic,format=rgba[pip_c];nullsrc=s=236x236:r=30,format=gray,geq=lum='clip((29-hypot(max(abs(X-118+0.5)-89,0),max(abs(Y-118+0.5)-89,0)))*255+127.5,0,255)',trim=end_frame=1,loop=loop=-1:size=1:start=0,setpts=N/(30*TB)[pip_m];[pip_c][pip_m]alphamerge[pip];[scr][pip]overlay=1660:820:format=auto[vout];[2:a]aresample=48000:async=1[aout]"
$ok = $false; $used = ''
foreach ($enc in $chain) {
  if ($have -notmatch "\s$enc\s") { continue }
  $a = @('-hide_banner','-loglevel','warning','-f','lavfi','-i','testsrc2=size=1920x1080:rate=30','-f','lavfi','-i','testsrc=size=640x480:rate=30','-f','lavfi','-i','sine=frequency=440:sample_rate=48000',
    '-filter_complex',$fc,'-map','[vout]','-map','[aout]','-c:v',$enc) + ($extra[$enc] -split ' ') +
    @('-b:v','793k','-maxrate','1190k','-bufsize','1586k','-g','60','-pix_fmt','yuv420p','-r','30','-fps_mode','cfr','-c:a','aac','-b:a','96k','-ar','48000','-ac','2','-t',"$Seconds",'-y',$out)
  & $ff @a
  if ($LASTEXITCODE -eq 0 -and (Test-Path $out)) { $ok = $true; $used = $enc; break }
  Write-Host "encoder $enc failed, trying next"
}
if (-not $ok) { Write-Host 'FAIL: no encoder produced output'; exit 1 }
$j = & $fp -v error -show_entries stream=codec_type,codec_name,width,height,r_frame_rate -show_entries format=duration -of json $out | ConvertFrom-Json
$v = $j.streams | Where-Object codec_type -eq 'video' | Select-Object -First 1
$au = $j.streams | Where-Object codec_type -eq 'audio' | Select-Object -First 1
$dur = [double]$j.format.duration
$fails = @()
if ($v.width -ne 1920 -or $v.height -ne 1080) { $fails += "size $($v.width)x$($v.height)" }
if (-not $au) { $fails += 'no audio stream' }
if ([math]::Abs($dur - $Seconds) -gt 1.0) { $fails += "duration $dur" }
if ($v.r_frame_rate -ne '30/1') { $fails += "fps $($v.r_frame_rate)" }
$kbps = (Get-Item $out).Length * 8 / $dur / 1000
Write-Host "encoder=$used duration=$dur kbps=$([math]::Round($kbps)) video=$($v.codec_name) audio=$($au.codec_name)"
if ($kbps -gt 1100) { $fails += "bitrate $kbps kbps over 889 budget by >20%" }
if ($fails.Count) { Write-Host "FAIL: $($fails -join '; ')"; exit 1 }
Write-Host 'PASS'
