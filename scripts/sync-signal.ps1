# sync-signal.ps1 - flash a white square in the middle of the screen and beep at the same moment.
#
# Record this with the app (microphone on, speakers audible), then run scripts/sync-check.ps1 on the
# recording to read how far sound and picture are apart. PLAYS SOUND through the default output device.
#
#   pwsh scripts/sync-signal.ps1 [-Count 6] [-EveryMs 2000] [-FlashMs 150] [-Mute]
#
# -Mute shows the flashes without the beeps (to check the picture half of the test quietly).
# Works in Windows PowerShell 5.1 and PowerShell 7.
param([int]$Count = 6, [int]$EveryMs = 2000, [int]$FlashMs = 150, [switch]$Mute)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing

# 100 ms of a 1 kHz tone as a WAV in memory: 48 kHz, 16-bit, mono, with 5 ms fades against clicks.
function New-BeepStream {
  $rate = 48000; $n = [int]($rate * 0.1); $ms = New-Object System.IO.MemoryStream; $w = New-Object System.IO.BinaryWriter($ms)
  $w.Write([Text.Encoding]::ASCII.GetBytes('RIFF')); $w.Write([int](36 + $n * 2)); $w.Write([Text.Encoding]::ASCII.GetBytes('WAVEfmt '))
  $w.Write([int]16); $w.Write([int16]1); $w.Write([int16]1); $w.Write([int]$rate); $w.Write([int]($rate * 2)); $w.Write([int16]2); $w.Write([int16]16)
  $w.Write([Text.Encoding]::ASCII.GetBytes('data')); $w.Write([int]($n * 2))
  $fade = [int]($rate * 0.005)
  for ($i = 0; $i -lt $n; $i++) {
    $gain = [math]::Min(1.0, [math]::Min($i, $n - 1 - $i) / $fade)
    $w.Write([int16](0.6 * 32767 * $gain * [math]::Sin(2 * [math]::PI * 1000 * $i / $rate)))
  }
  $w.Flush(); $ms.Position = 0; return $ms
}

$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$form = New-Object System.Windows.Forms.Form
$form.FormBorderStyle = 'None'; $form.TopMost = $true; $form.ShowInTaskbar = $false; $form.StartPosition = 'Manual'
# A quarter of the screen each way, centred: sync-check.ps1 looks at the middle eighth.
$form.Size = New-Object System.Drawing.Size([int]($screen.Width / 4), [int]($screen.Height / 4))
$form.Location = New-Object System.Drawing.Point([int]($screen.X + $screen.Width * 3 / 8), [int]($screen.Y + $screen.Height * 3 / 8))
$form.BackColor = [System.Drawing.Color]::Black
$player = $null
if (-not $Mute) { $player = New-Object System.Media.SoundPlayer (New-BeepStream); $player.Load() }

$form.Show(); $form.Refresh()
# Paint straight onto the window: waiting for the next paint message can swallow a 100 ms flash.
$gfx = $form.CreateGraphics()
function Set-Square([System.Drawing.Color]$c) { $form.BackColor = $c; $gfx.Clear($c); $gfx.Flush(); [System.Windows.Forms.Application]::DoEvents() }
Set-Square ([System.Drawing.Color]::Black)
$clock = [Diagnostics.Stopwatch]::StartNew()
for ($i = 1; $i -le $Count; $i++) {
  while ($clock.ElapsedMilliseconds -lt $i * $EveryMs) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 2 }
  Set-Square ([System.Drawing.Color]::White)
  if ($player) { $player.Play() }
  $until = $clock.ElapsedMilliseconds + $FlashMs
  while ($clock.ElapsedMilliseconds -lt $until) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 2 }
  Set-Square ([System.Drawing.Color]::Black)
  Write-Host "signal $i of $Count"
}
Start-Sleep -Milliseconds 600
$gfx.Dispose(); $form.Close(); $form.Dispose()
if ($player) { $player.Dispose() }
Write-Host 'done'
