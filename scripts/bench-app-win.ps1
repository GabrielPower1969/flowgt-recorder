# Start a built FlowGT Recorder with the WebView2 debugging port open, for the start/stop benches.
#
#   powershell -ExecutionPolicy Bypass -File scripts/bench-app-win.ps1 [-Exe <path>] [-Port 9222]
#
# Then, while it runs:
#   node scripts/bench-start-stop.mjs new 5 8 '{"countdownSeconds":3,"audio":{"micEnabled":true},"camera":{"enabled":true}}'
#   node scripts/bench-flows.mjs <ffprobe>
#
# The port listens on 127.0.0.1 only and exists for this process alone (an environment variable of the
# child, not a system setting). Quit the app when the bench is done. The app log goes to
# tests/out/startstop/app.log (git-ignored); `timing ...` lines in it are the app's own measurements.
# Recordings land in the folder set in the app ("Videos\FlowGT Recorder" by default), named bench-* / flow-*.
param(
  [string]$Exe = "src-tauri/target/release/flowgt-recorder.exe",
  [int]$Port = 9222
)
$ErrorActionPreference = "Stop"
if (-not (Test-Path $Exe)) { throw "No app at $Exe. Build it first: npm run tauri build" }
Get-Process flowgt-recorder -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep 1
$out = "tests/out/startstop"
New-Item -ItemType Directory -Force $out | Out-Null
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
$env:RUST_LOG = "info"
Start-Process (Resolve-Path $Exe) -RedirectStandardError "$out/app.log" -RedirectStandardOutput "$out/app.out.log" | Out-Null
for ($i = 0; $i -lt 40; $i++) {
  Start-Sleep -Milliseconds 500
  try {
    $pages = Invoke-RestMethod "http://127.0.0.1:$Port/json" -TimeoutSec 2
    if ($pages | Where-Object { $_.url -like "*#/floating*" }) { "PASS app ready, debugging port $Port"; exit 0 }
  } catch { }
}
"FAIL the floating window did not appear on port $Port within 20 s"
exit 1
