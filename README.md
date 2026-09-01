# FlowGT Recorder

**EN** · Record browser-based meetings locally — tab audio + your microphone, mixed
into one track and saved on your machine. Nothing is ever uploaded. Built to feed
the FlowGT interview-analysis pipeline (Claude Code + `interview-analysis` skill).

**中文** · 录制**浏览器里的网络会议**：对方声音（标签页音频）+ 你的麦克风，混成
一轨保存在本地 `~/Downloads/flowgt-recordings/`。**音频永不上传任何服务器。**
产出直接对接 FlowGT 面试分析流水线（Claude Code + `interview-analysis` skill）。

Works on **macOS and Windows** (any OS that runs Chrome). ·
**macOS / Windows** 双平台可用（Chrome 在哪它在哪）。

---

## Install (developer mode) · 安装（开发者模式）

1. Open `chrome://extensions`
2. Toggle **Developer mode** (top right) · 打开右上角**开发者模式**
3. **Load unpacked** → select the `extension/` folder · **加载已解压的扩展程序** → 选 `extension/` 目录
4. Pin the FlowGT icon to the toolbar · 把 FlowGT 图标固定到工具栏

> Chrome Web Store release is planned — a one-time US$5 developer registration,
> then installs are one click. · 后续可上架 Chrome 商店（一次性 $5 注册），用户一键安装。

## First run · 首次使用

Click the icon → if you see *"Microphone not enabled"* → **Enable microphone →**
→ click **Allow**. One-time only.
点图标 → 出现"Microphone not enabled"就点 **Enable microphone** → 浏览器弹窗选**允许**。只需一次。

## Daily use · 日常使用

| Step | EN | 中文 |
|---|---|---|
| 1 | Join the meeting **in a browser tab** (Google Meet, Zoom Web, Teams Web) | 在**浏览器标签页**里入会 |
| 2 | Switch to the meeting tab, click the FlowGT icon (or press `⌘⇧9` / `Ctrl+Shift+9`) | 切到会议标签页，点图标或按快捷键 |
| 3 | Optional label, e.g. `Iris_Datacom_R2` | 备注可选，会拼进文件名 |
| 4 | **Start recording** — icon shows a red `REC` badge | 点开始，图标出现红色 REC 角标 |
| 5 | **Stop & save** → `~/Downloads/flowgt-recordings/2026-09-01_1430_Iris_Datacom_R2.webm` | 停止即落盘 |

- Closing the meeting tab mid-call auto-saves the recording. · 会议页被关掉也会自动收尾保存，不丢。
- You keep hearing the meeting while recording (playback is routed through). · 录音期间正常听到对方。
- If the mic isn't granted, recording still works — tab audio only, with a warning shown. · 麦克风未授权时降级为只录对方，界面有提示。

## Analyze · 分析（Claude Code 一句话）

> 分析 ~/Downloads/flowgt-recordings/ 里最新这场面试，成员 Iris，公司 Datacom，第 2 轮

Per the global CLAUDE.md rules this runs the full chain:
`make-srt.sh` (local mlx-whisper) → hallucination check → `interview-analysis` skill
→ HTML report → archive. webm/opus is consumed directly by ffmpeg/whisper;
convert with `ffmpeg -i in.webm out.mp3` when the archive needs mp3.

## Boundaries · 已知边界

| Scenario · 场景 | Works? | Notes · 说明 |
|---|---|---|
| Meet / Zoom Web / Teams Web (browser tab) | ✅ | The extension's home turf · 主战场 |
| Zoom / Teams **desktop apps** | ❌ | Chrome extensions cannot capture outside the browser. Ask for a browser join link ("Join from your browser"), or wait for the Phase-2 system-level recorder. · 扩展抓不到浏览器外的声音；让对方发浏览器入会链接，或等二期系统级方案 |
| Mobile | ❌ | Out of scope · 不在范围 |

## Privacy & consent · 隐私与合规

Recordings never leave your machine. Always tell participants you are recording.
NZ law permits recording conversations you take part in (one-party consent), but
disclosure is the professional default — and required in many other jurisdictions.
录音不出本机。录音前请告知参与者；新西兰允许录制自己参与的对话，但主动告知永远是更稳妥的做法。

FlowGT account sign-in (optional, shown in the popup footer) only reads your
existing `flowgt.co.nz` session to display membership status — no recording data
is ever sent. · 底部的 FlowGT 登录态只读取你在官网的现有会话用于显示会员状态，不回传任何录音数据。

## Development · 开发

```
extension/            ← load this in Chrome
├── manifest.json       MV3, tabCapture + offscreen + downloads + storage
├── background.js       service worker: streamId, download, state, /api/session
├── recorder-core.js    mixing + MediaRecorder + save (shared prod/test)
├── offscreen.js/html   production capture entry (tabCapture + mic)
├── popup.*             FlowGT-branded control panel
├── permission.*        one-time mic grant page
├── test-harness.*      test-only getDisplayMedia entry (excluded from store build)
├── icons/              official FlowGT mark (brand/png)
└── fonts/              Space Grotesk subset (wordmark, OFL license)
test/
├── e2e.mjs                      full pipeline E2E (14 checks, headless)
├── screenshot-ui.mjs            UI state screenshots
├── manual-tabcapture-check.mjs  tabCapture handshake check (needs real invocation)
└── tone.html                    440Hz test source
```

Run tests · 跑测试:

```bash
node test/e2e.mjs
```

Requires the pinned Chrome for Testing under `.browsers/` (Chrome 137+ retail
builds removed `--load-extension`): · 需要 `.browsers/` 里的 Chrome for Testing：

```bash
npx @puppeteer/browsers install chrome@stable --path ./.browsers
```

## Roadmap · 路线图

- **Phase 2 — full automation**: Native Messaging host writes straight into
  `flowgt-media/`, auto-triggers whisper + analysis. · 二期：Native 主机直写媒体库，录完自动转写分析。
- System-level capture fallback for desktop Zoom/Teams. · 系统级录音兜底桌面客户端。
- Chrome Web Store listing + FlowGT account-gated premium features. · 上架商店，账号体系解锁高级功能。
