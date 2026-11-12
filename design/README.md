# Design — look "Faceplate"

> Since 2026-11-11 the app wears the "Faceplate" look: the main window is the front panel of a reel-to-reel
> recorder (aluminium plate, two reels, off-white square keys, one red dot). Canvas:
> https://claude.ai/artifact/JSnvN5dF734tuFgCdgMYih · description in `docs/DESIGN.md` · values in `design/tokens.css`.
> The rest of this file describes the previous look, "Studio", and is kept for reference until it is rewritten.

Previous canvas (Claude Design mode, 2026-11-04): https://claude.ai/artifact/S5hmhuFEHGxEU93FfHJtq7
Tokens the app uses verbatim: `design/tokens.css` (imported once in the app shell; nothing else defines colours).

## The look

Calm, premium pro tool. Graphite dark ground `#0F1115` / warm off-white light ground `#F6F5F2`, surfaces one step
lighter or darker, hairline borders. **One** interactive accent (indigo `#4452E6` light / `#7D8BFF` dark). Red
(`#D63C41` / `#E5484D`) belongs only to the REC button, the REC dot and Stop. Amber = warning, green = ok.
Inter for UI copy, JetBrains Mono for every figure that changes (timer, size, bitrate, resolution, paths).
Radii 8 / 12 / 16, pill for chips. No gradients, no emoji, inline stroke SVG icons, 44 px touch targets,
real `<button>` / `<input>` / `<label>` everywhere.

## Artboards (dark row on top, light row below)

| File | Screen | Size | Notes |
|---|---|---|---|
| `Main.dc.html` / `Main-Light.dc.html` | 主窗口 Main | 1280 × 860 page | Source picker (displays, 区域, disabled 窗口), camera + PiP controls with stage preview, mic + level meter, system audio, meeting name, output path, estimate card, disk line, REC button |
| `Onboarding-*.dc.html` | 首次权限向导 | 1280 × 760 page | 3-step stepper (屏幕录制 ✓ / 摄像头 current / 麦克风), privacy-pane illustration, status chip, 去系统设置 / 重新检查 / 跳过, Windows strip |
| `Floating-*.dc.html` | 录制中悬浮条 | 420 × 72 fixed | Pill; drag dots, pulsing REC dot, mono timer, size estimate, mini mic meter, hide-camera / pause / stop |
| `Countdown-*.dc.html` | 倒计时 | 420 × 200 fixed | Ring + mono "3", what will be recorded, Esc to cancel |
| `Summary-*.dc.html` | 结算页 | 1280 × 820 page | 16:9 preview with PiP, filename, detail tiles, over-budget card with "压到 400 MB", 再录一段 |
| `Settings-*.dc.html` | 设置 | 1280 × 900 page | Left nav; 通用 / 视频 / 音频 / 快捷键 / 恢复 / 关于 |

## Token → component map

| Token(s) | Used by |
|---|---|
| `--color-bg`, `--color-surface`, `--color-border` | window background, cards, header, hairlines |
| `--color-surface-2` / `-3` | inputs, selects, segmented controls (unselected), meter track, stage preview |
| `--color-text`, `--color-text-muted`, `--color-text-faint` | copy / labels & hints / captions (faint: ≥ 12 px only on dark) |
| `--color-accent`, `--color-accent-fg`, `--color-accent-hover`, `--color-accent-soft` | primary buttons, selected segment, selected source card, active nav, info banners, PiP outline |
| `--color-record`, `--color-record-hover`, `--color-record-soft` | REC button, REC dot, Stop button, countdown ring — nothing else |
| `--color-warn`, `--color-warn-soft` | over-budget card, "未授权" chip, recovery row, meter hot zone |
| `--color-ok`, `--color-ok-soft` | "就绪" / "已保存" pills, granted step, meter normal zone, disk-space line |
| `--color-floating-bg`, `--shadow-floating` | floating bar and countdown window |
| `--color-focus` | 2 px focus ring on every control |
| `--radius-s/m/l/pill` | 8 small controls · 12 inputs & buttons · 16 cards & REC · pill chips |
| `--font-ui`, `--font-mono`, `--text-*` | all copy / timer, sizes, bitrates, paths |
| `--duration-*`, `--ease` | hover, segment switch, countdown ring |

## Rules

1. Change a token here first; the canvas mirrors it, not the other way round.
2. Never introduce a second accent. Red is semantic (recording), not decorative.
3. Every icon-only button has an `aria-label`; every input has a `<label>`.
4. Pages must still work at phone width (columns wrap, toolbars wrap).
