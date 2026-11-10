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
| 5 | **Stop & save** → `~/Downloads/flowgt-recordings/2026-09-01_1430_Iris_Datacom_R2.mp4` | 停止即落盘 |

- Recordings are **audio-only `.mp4` (AAC 128k)** — double-click plays anywhere
  (QuickTime, Windows, WeChat). Old Chrome (<126) falls back to `.webm` (Opus).
  · 录音是**纯音频 .mp4（AAC）**，任何播放器双击即播；老 Chrome 自动回退 .webm。
- Closing the meeting tab mid-call auto-saves the recording. · 会议页被关掉也会自动收尾保存，不丢。
- You keep hearing the meeting while recording (playback is routed through). · 录音期间正常听到对方。
- If the mic isn't granted, recording still works — tab audio only, with a warning
  shown and a `_nomic` marker in the filename. · 麦克风未授权时降级为只录对方，
  界面有提示，且文件名带 `_nomic` 标记（一眼可见人声没进来）。

## Analyze · 分析（Claude Code 一句话）

> 分析 ~/Downloads/flowgt-recordings/ 里最新这场面试，成员 Iris，公司 Datacom，第 2 轮

Per the global CLAUDE.md rules this runs the full chain:
`make-srt.sh` (local mlx-whisper) → hallucination check → `interview-analysis` skill
→ HTML report → archive. Both .mp4/AAC and .webm/Opus are consumed directly by
ffmpeg/whisper; convert with `ffmpeg -i in.mp4 out.mp3` when the archive needs mp3.

## Media downloader (dev build only) · 视频/音频下载（仅开发版）

On a **YouTube or Bilibili video page**, the popup shows a download card with the
platform's logo — detected automatically, nothing to choose. Pick **Video · best**
(highest-quality video + AAC audio, merged to `.mp4`) or **Audio · m4a**. Files land in
`~/Downloads/flowgt-downloads/`, named `<title> [<id>].<ext>`.
在 **YouTube / B 站视频页**打开插件会自动出现下载卡（自动识别平台并显示 logo，无需选择）。
选 **Video · best**（最高画质 + AAC 音轨合成 mp4）或 **Audio · m4a**，文件存到
`~/Downloads/flowgt-downloads/`。

**One-time setup (macOS) · 一次性安装：** the extension hands the actual download to
[yt-dlp](https://github.com/yt-dlp/yt-dlp) on your machine through a tiny local helper.
插件本身不下载，交给本机 yt-dlp 干活，需要装一次本机助手：

```bash
python3 -m pip install --user "yt-dlp[default]"   # or: brew install yt-dlp
host/install.sh
```

Then reload the extension at `chrome://extensions`. If you loaded the extension from a
different folder, pass its ID: `host/install.sh <extension-id>`.
然后到 `chrome://extensions` 刷新插件。若插件不是从本仓库 `extension/` 加载的，把扩展 ID 作为参数传给脚本。

| Note · 说明 | |
|---|---|
| Bilibili HD | 1080P+ needs your Bilibili login: the helper reads your Chrome cookies (toggle in the card, on by default). macOS may ask once for Keychain access — if denied, it retries without login. · B 站高清需登录，默认读取 Chrome 登录态（可关）；首次可能弹钥匙串授权，拒绝则自动以未登录画质重试 |
| 4K YouTube | Top-quality YouTube video is often AV1/VP9. QuickTime may not play it — use IINA or VLC. Audio is always AAC. · YouTube 最高画质多为 AV1/VP9，QuickTime 可能打不开，用 IINA/VLC；音轨始终是 AAC |
| Scope | Single video only (no playlists). One download at a time; closing Chrome cancels it. · 只下单条，不下播放列表；同时只能一个任务 |
| **Not in store builds** | Chrome Web Store policy does not allow YouTube downloaders, and YouTube's terms prohibit downloading. This feature ships only in this developer build — for personal use; respect each platform's terms and creators' rights. · 商店政策不允许 YouTube 下载器、YouTube 条款禁止下载：本功能只存在于开发版，仅限个人用途，请遵守平台条款与版权 |

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
├── platform.js         YouTube/Bilibili detection (single source, shared with host)
├── popup.*             FlowGT-branded control panel (+ download card)
├── permission.*        one-time mic grant page
├── test-harness.*      test-only getDisplayMedia entry (excluded from store build)
├── icons/              official FlowGT mark (brand/png)
└── fonts/              Space Grotesk subset (wordmark, OFL license)
host/                  ← local download helper (Native Messaging, dev build only)
├── flowgt-host.mjs     stdio host: framing, URL allow-list, runs yt-dlp
├── ytdlp-args.mjs      yt-dlp args + output parsing (pure)
├── ext-id.mjs          unpacked-extension ID from folder path
└── install.sh          one-time macOS install (writes Chrome host manifest)
test/
├── unit.mjs                     pure-logic unit tests (81 checks; plain Node)
├── host.test.mjs                host integration tests (38 checks; stub yt-dlp, no network)
├── e2e.mjs                      full pipeline E2E (48 checks, headless, real Chrome + real Native Messaging)
├── screenshot-ui.mjs            UI state screenshots
├── manual-tabcapture-check.mjs  tabCapture handshake check (needs real invocation)
└── tone.html                    440Hz test source (?silent=1 → mic-only test mode)
```

Architecture & design decisions: see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

Run tests · 跑测试:

```bash
npm test
```

(`npm run test:unit` for the instant Node-only suite, `npm run test:host` for the
download helper, `npm run test:e2e` for the full browser pipeline.)

Requires the pinned Chrome for Testing under `.browsers/` (Chrome 137+ retail
builds removed `--load-extension`): · 需要 `.browsers/` 里的 Chrome for Testing：

```bash
npx @puppeteer/browsers install chrome@stable --path ./.browsers
```

## Roadmap · 路线图

- **Phase 2 — full automation**: the Native Messaging host (shipped in v0.3 for
  downloads) also receives recordings, writes straight into `flowgt-media/`, and
  auto-triggers whisper + analysis. · 二期：复用 v0.3 已落地的 Native 主机，录音直写媒体库并自动转写分析。
- Windows installer for the download helper (`install.ps1`, registry-based host manifest). · 下载助手的 Windows 安装脚本。
- System-level capture fallback for desktop Zoom/Teams. · 系统级录音兜底桌面客户端。
- Chrome Web Store listing + FlowGT account-gated premium features. · 上架商店，账号体系解锁高级功能。
