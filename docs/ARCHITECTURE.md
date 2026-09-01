# FlowGT Recorder — 架构设计

> 读者：维护者与代码审查者。用户文档见根目录 README.md。
> 最后核实：2026-09-02（v0.2.0，E2E 25/25 通过）

## 1. 设计目标与硬约束

| 目标 | 落地 |
|---|---|
| 录"对方声 + 我的声"混成一轨 | tabCapture（标签页音频）+ getUserMedia（麦克风）→ Web Audio 混音 |
| **音频永不出本机** | 唯一出口是 `chrome.downloads` 写本地；无任何上传代码路径 |
| 录音失败面前保守 | 麦克风拿不到 → 降级只录标签页（文件名带 `_nomic`），而不是整场失败 |
| 产物人人能播 | AAC/.mp4 优先（QuickTime/Windows/微信双击即播），老 Chrome 回退 Opus/.webm |
| 测试覆盖生产代码本身 | 混音/编码/落盘核心抽成 `recorder-core.js`，生产与测试共用同一份 |

MV3 强加的结构性约束（架构因此长这样）：

- **Service worker 没有 DOM/AudioContext** → 采集必须放 offscreen document。
- **`tabCapture.getMediaStreamId` 要求真实用户手势**（点扩展图标/快捷键），
  无法在纯自动化里合成 → 测试用 getDisplayMedia + `--auto-select-tab-capture-source-by-title`
  走同一 recorder-core 管线（见 §4）。
- **tabCapture 会静音原标签页** → 录音时必须把采集到的声音回放到扬声器
  （`playback: true`），否则开会听不到对方。

## 2. 组件与职责

```
popup.js ──(user gesture)──► background.js ──streamId──► offscreen.js ──► recorder-core.js
   │                            │  service worker            │  DOM 环境          │
   │ UI 状态/计时器              │  getMediaStreamId          │  tab gUM + mic gUM │ 混音(AudioContext)
   │ storage.session 读取        │  offscreen 生命周期         │  消息薄封装         │ MediaRecorder 编码
   │ 麦克风权限检查/引导          │  chrome.downloads 落盘      │                    │ 轨道结束监视→自动收尾
   │ flowgt.co.nz 登录态展示     │  REC 角标 + 状态机          │                    │ 文件名构造(纯函数)
permission.html/js: 一次性麦克风授权页（offscreen 无法弹权限框，必须借普通扩展页）
```

**单一职责**：只有 `recorder-core.js` 碰音频；只有 `background.js` 碰状态与下载；
只有 popup/permission 有 UI。测试入口 `test-harness.*` 是 offscreen.js 的平行替身
（采集来源不同，之后的一切相同），打包上架时剔除。

## 3. 消息协议与状态

所有消息 `{target: 'background'|'offscreen', type, ...}`，经 `chrome.runtime.sendMessage` 广播路由。

| type | 方向 | 语义 |
|---|---|---|
| `ping` | bg → off | offscreen 就绪握手（createDocument resolve ≠ listener 就绪，轮询到 pong 为止） |
| `start` / `stop` | bg → off | 开始/停止采集；start 回 `{ok, micOk}` |
| `save-recording` | off → bg | blob URL + 文件名，bg 用 `chrome.downloads` 落盘 |
| `saved` | bg → off | 下载完成，释放 blob URL（之后才能关 offscreen，否则 URL 失效） |
| `recording-finished` | off → bg | 录音收尾（含标签页被关的自动收尾），清状态 |
| `start-recording` / `stop-recording` | popup → bg | 用户操作入口 |
| `get-account` | popup → bg | flowgt.co.nz `/api/session` 登录态（60s 缓存；host_permissions 下带 fg_sess cookie） |

状态唯一真源是 `chrome.storage.session`（浏览器重启即清，符合"录音是短命事务"）：
`rec = {recording, startTime, tabId, label, micOk}`；错误写 `lastError = {msg, at, type}`。
双重启动守卫、失败清理、REC 角标全部挂在这个状态上。

## 4. 测试策略（三层）

| 层 | 文件 | 覆盖 | 运行 |
|---|---|---|---|
| 单元 | `test/unit.mjs` | 纯逻辑：文件名构造（非法字符/截断/`_nomic`/中文）、格式选择回退 | 裸 Node，毫秒级 |
| E2E | `test/e2e.mjs` | 真 Chrome 全管线 25 项：录→停→落盘、ffmpeg 验 codec/时长/音量、**纯麦克风混音**、标签页关闭自动收尾、双启守卫、错误路径、账号消息 | headless，~1 分钟 |
| 真手势 | `test/manual-tabcapture-check.mjs` | tabCapture 真实唤起握手（E2E 无法合成的那一环） | 需人在场或系统空闲 |

E2E 的关键技巧（都是踩坑换来的，别改回去）：

- 采集源用 `getDisplayMedia` + `--auto-select-tab-capture-source-by-title=FlowGT test tone`，
  绕过手势要求但走同一 core；`--allowlisted-extension-id` 在 Chrome 152 只放行 API、设备层照拒。
- `--use-fake-ui-for-media-stream` 与 tabCapture 消费不兼容（NotFound），只配 gdm/mic 用。
- 下载目录写 profile `Preferences` 的 `download.default_directory`；
  CDP `setDownloadBehavior` 会把文件改成 GUID 名。
- Chrome 137+ 正式版删了 `--load-extension` → 必须用 `.browsers/` 里 pin 的 Chrome for Testing。
- 同一页面第二次 getDisplayMedia 会挂住 → 每个用例开新 harness 页。
- gdm 测试路径下轨道不发 `ended` → T2 合成派发；core 因此同时轮询 `readyState` 兜底。
- **T5（纯麦克风）是"人声没录进去"的哨兵**：标签页静音、假麦克风发声，文件仍非静音
  才算麦克风混音路是通的。

## 5. 编码格式决策（2026-09-02）

`pickFormat()` 运行时探测：`audio/mp4;codecs=mp4a.40.2`（AAC）→ `audio/mp4` → `audio/webm;codecs=opus`。

- 为什么不固定 Opus：音质效率虽最高，但 .webm 在 QuickTime/微信/多数播放器打不开，
  用户"想听都没有播放器"。AAC 128k 对语音是透明音质，兼容性拉满。
- 为什么扩展名是 `.mp4` 不是 `.m4a`：`chrome.downloads` 按 blob MIME（audio/mp4）
  强制改写扩展名，写 `.m4a` 会被浏览器改成 `.mp4`——不与浏览器较劲。
- 10 秒 timeslice 分片（fMP4），进程崩溃最多丢最后 10 秒。

## 6. 隐私模型

- 录音数据流：标签页/麦克风 → AudioContext（内存）→ MediaRecorder → blob →
  `chrome.downloads` → 本地磁盘。**没有网络出口。**
- 唯一网络请求：`GET https://flowgt.co.nz/api/session`（只读登录态、显示会员身份，
  不携带也不回传任何录音相关数据）。host_permissions 只声明这一个域。
- 权限最小集：`tabCapture, offscreen, downloads, storage`；无 `<all_urls>`，无 content script。

## 7. 已知边界与二期方向

- 桌面版 Zoom/Teams 抓不到（扩展只及浏览器）→ 二期系统级录音兜底。
- 二期全自动：Native Messaging host 直写 `flowgt-media/`，录完自动 whisper + 分析。
- 上架 Chrome Web Store 时剔除 `extension/test-harness.*`。
