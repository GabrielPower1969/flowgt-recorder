# 下一个会话的开场 prompt（给便宜模型，2026-11-04 11:57 写）

> 用法：新开一个 Claude Code 会话（建议 Sonnet 或 Opus，effort high，工作目录就是本仓库），把下面整段贴进去。

---

你接手 **FlowGT Recorder**：一个 Tauri 2 跨平台录屏软件（macOS + Windows），用途是 Gabriel 参加别人的会议/培训时自己留一份录像：录屏 + 自己摄像头的方形画中画（默认右下角，可拖动/改大小/隐藏/镜像/圆角或圆形）+ 麦克风 + 会议系统声音，一小时约 400 MB、1080p30、HEVC 硬件编码。上一轮由多个并行子代理建仓并开工，被中途暂停；现在由你**顺序**把它做完。

## 先读（按顺序，读完再动手）
1. `docs/HANDOVER-2026-11-04.md` —— 现状表和剩余步骤（权威）。
2. `CLAUDE.md`、`docs/ARCHITECTURE.md`、`docs/DECISIONS.md`（D1–D14 是已定的技术决定，偏离要在 DECISIONS.md 追加一条写理由）。
3. 合约：`src-tauri/crates/core/src/types.rs` 与 `src/lib/types.ts`（1:1 对应，改一处必须同一 commit 改另一处；`Commands`/`Events` 接口就是前后端的全部 IPC）。
4. `design/tokens.css`、`docs/DESIGN.md`（界面只能用这些 CSS 变量；设计画布 https://claude.ai/artifact/S5hmhuFEHGxEU93FfHJtq7）。

## 硬规矩
- 代码、注释、commit 全英文；commit 格式 `type(scope): summary`，正文写 Why，结尾一行 `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`。
- **只本地 commit，不建 GitHub 远程，不 push。**
- **不要真跑屏幕录制**（会弹 macOS 权限框，只有 Gabriel 能点）。端到端一律用 ffmpeg 的 lavfi 合成输入（`testsrc2`、`color`、`sine`）。
- 不要一步一问：自己决定并写进 `docs/DECISIONS.md`；只有人能做的事追加到 `docs/MANUAL-CHECKLIST.md`。
- 每个「已验证」都要有命令输出支撑；没跑过就写没跑过。Windows 在本机无法实测，只能靶 mock/fixture 的单元测试覆盖，报告里明说。
- 每完成一步就 `git add -A && git commit`，commit 前 `npm test` 和 `cargo test`（在 `src-tauri/`）必须全绿。
- 回复 Gabriel 用中文；他和家人是新手读者，计划和报告要图形化（表格、流程图、时间线）。

## 第 0 步：接手检查
```
git status            # HEAD 之后出现的文件是被切断的子代理留下的 WIP，逐个审一遍再提交
cd src-tauri && cargo test -p recorder-core; cd ..
npm install && npm test
```
把能过的先 commit 成 `wip(...)`。

## 第 1 步：填完 core 的 `todo!()`（最先，其余都依赖它）
目录 `src-tauri/crates/core/src/`。每个文件顶部注释和函数 doc 写了要做什么，**签名不改**。要点：
- `progress.rs`：增量解析 `-progress pipe:1` 流（frame/fps/bitrate/total_size/out_time_us/drop_frames/dup_frames/speed/progress=continue|end），支持分块、CRLF、`N/A`；`classify_stderr_line` 把摄像头掉线/丢帧/磁盘满映射到 i18n 码。
- `recovery.rs`：manifest 生命周期；`seg-000.mp4` 三位编号；`decide()`：Complete→Discard，有字节的段→Concat；`concat_list()` 用 ffmpeg concat demuxer 格式并转义单引号。
- `devices.rs`：解析 avfoundation（本机可真跑 `ffmpeg -hide_banner -f avfoundation -list_devices true -i "" 2>&1` 取 fixture）和 dshow（手写 fixture，首行 `# hand-written fixture`）。
- `encoders.rs`：解析 `ffmpeg -encoders`；回退链 macOS `hevc_videotoolbox → h264_videotoolbox → libx264 → libopenh264 → mpeg4`，Windows `hevc_nvenc → hevc_qsv → hevc_amf → hevc_mf → h264_* → libx264 → …`；HEVC 加 `-tag:v hvc1`。
- `pip.rs`：方形 `crop=ih:ih` → 可选 `hflip` → `scale=S:S` → `format=rgba` + `geq` 圆角/圆形 alpha → `overlay=x:y`。
- `command.rs`：`RecordingPlan → argv`：每个输入前 `-thread_queue_size 1024`；视频 `-c:v <enc> <extra> -b:v V -maxrate 1.5V -bufsize 2V -g fps*2 -pix_fmt yuv420p -fps_mode cfr`；音频 `aac -b:a 96k -ar 48000 -ac 2`（Mixed 用 `amix`，Separate 两轨）；`-movflags +frag_keyframe+empty_moov+default_base_moof -frag_duration 2000000 -progress pipe:1 -nostats -y`；还有 concat/remux/compress/probe/quality/list 命令和 ffprobe JSON 解析。
- 测试：每模块单测；`command.rs` 加一个 lavfi 真跑 3 秒的冒烟测试（ffmpeg 不存在时跳过）。关键数字：`budget_for(400,96,1)` → total 889 / video 793 kbps。
- 验收：`cargo test -p recorder-core` 全绿，`cargo clippy -p recorder-core` 无警告。

## 第 2 步：Tauri app crate（`src-tauri/src/`）
- `commands.rs`：实现 `types.ts` 里 `Commands` 的每一个命令（snake_case 名字，camelCase JSON），错误返回 `{code, detail}`，code 用 i18n 键（`error.noFfmpeg` 等）。
- `recorder.rs`：tokio 子进程监管 ffmpeg；stdout 走 `ProgressParser` 每秒最多 2 次发 `recorder://progress`；stderr 分类发 `recorder://warning`；**暂停 = 关掉当前段（stdin 写 `q`），继续 = 开新段，停止 = concat 全部段**（一段就 remux）；manifest 每次状态变化原子写（临时文件 + rename）；摄像头掉线时 ffmpeg 退出 → 立刻开一个没有摄像头输入的新段继续录并警告。
- `sidecar.rs`（随包 `binaries/ffmpeg-<triple>` → `which ffmpeg`）、`permissions.rs`（macOS：`CGPreflightScreenCaptureAccess`/`CGRequestScreenCaptureAccess` extern "C"；摄像头/麦克风用 objc2-av-foundation `AVCaptureDevice`；打开系统设置用 `open x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture|Privacy_Camera|Privacy_Microphone`；Windows `ms-settings:privacy-webcam|privacy-microphone`）、`tray.rs`、`shortcuts.rs`（默认 `CmdOrCtrl+Shift+R`/`CmdOrCtrl+Shift+P`，按下时直接调录制器并广播 `shortcut://toggle|pause`）、`windows.rs`（`floating` 窗口 420×72、透明、无边框、置顶、倒计时阶段 420×200）、`disk.rs`（`fs2::available_space`）。
- 加命令行自测：`flowgt-recorder --selftest 5 [--pause-at 2 --resume-at 4]` 用 lavfi 走完整监管链路（含画中画），打印 `RecordingResult` JSON，exit 0。
- 图标：`ffmpeg -f lavfi -i color=c=0x5B6CFF:s=1024x1024 -frames:v 1 src-tauri/icons/icon-src.png` 然后 `npx tauri icon src-tauri/icons/icon-src.png -o src-tauri/icons`；托盘图标 22px 及 @2x。
- 先 `bash scripts/fetch-ffmpeg.sh`（把 Homebrew ffmpeg 复制成开发用 sidecar）。
- 验收：`cargo check`、`cargo clippy`、`npx tauri dev` 开窗（前端没好时能开空窗即可）、selftest 跑通并贴输出。

## 第 3 步：Svelte 5 前端（`src/`，runes，TypeScript strict，无 UI 库）
- hash 路由 `#/onboarding` `#/` `#/settings` `#/summary` `#/floating`；`src/lib/ipc.ts` 封装 invoke/listen，没有 `window.__TAURI_INTERNALS__` 时切到 `ipc-mock.ts`（vitest 和浏览器 `npm run dev` 都用 mock）。
- i18n `zh.json`/`en.json` 全覆盖（含 Rust 侧 warn./error. 码），zh 默认；主题 `data-theme`；所有颜色/间距/字体只用 `design/tokens.css` 变量。
- 页面照设计画布：首次权限向导（3 步、去系统设置、重新检查、跳过）；主窗口（显示器/区域选择、摄像头选择 + `getUserMedia` 实时预览 + 可拖可缩放的画中画舞台、麦克风电平表、系统声音选择、会议名、输出目录、「预计 1 小时 ≈ 400 MB」卡、磁盘空间、大红 REC 按钮 + 快捷键提示）；悬浮条（倒计时 → REC 点/时长/体积估算/电平/暂停/停止/隐藏摄像头，`data-tauri-drag-region`）；结算页（预览、体积/时长/分辨率/码率/编码器、在 Finder 中显示、复制路径、超预算时「压到 400 MB」、再录一段）；设置页（通用/视频/音频/快捷键/恢复/关于）。
- 无障碍：所有控件有 label，焦点环，键盘可操作画中画，`prefers-reduced-motion`。
- 测试 Vitest + Testing Library ≥ 60 个：format 工具（文件名预览 `2026-11-04 1430 周会`、889/793）、i18n 键对齐、mock 生命周期、组件与页面。
- 验收：`npm test` 全绿、`npm run check` 0 错、`npm run build` 成功、`npx tauri dev` 里五个页面都能走通（mock 不参与，真 IPC）。

## 第 4 步：脚本、测试、CI（`scripts/`、`tests/`、`.github/workflows/ci.yml`）
- `size-check.sh`：2 分钟合成**动态**1080p30 画面 + 画中画 + 音频，用 app 同参数编码，外推 `bytes/秒×3600/1e6` MB/h，350–450 PASS；再给一个静态画面对照。
- `quality-check.sh`：无损参考 vs app 参数编码，`ssim`/`psnr` 滤镜；再加 h264_videotoolbox 同码率对照行。
- `bench.sh`：pipeline 模式（lavfi + `-re`）与 `--app` 模式（抓 app + ffmpeg 进程）用 `ps -o %cpu,rss` 每秒采样，输出均值/峰值/RSS/丢帧，PASS：合计 CPU ≤ 15%、RSS ≤ 300 MB、drop_frames = 0；`--idle` 看空闲 < 1%。另有 `bench-startup.sh`（< 1 s）、`bench-stop-latency.sh`（< 2 s）。
- `crash-test.sh`：fragmented 录制 15 s 在 10 s 时 `kill -9`，ffprobe 仍能读到 ≥ 8 s；非 fragmented 对照应读不出。
- `integration.sh`：lavfi → MP4 → ffprobe 断言（时长、1 视频 + 1/2 音频轨、1920×1080、hevc、总码率 700–1100 kbps）+ 像素检查（屏幕蓝、摄像头红，右下角画中画中心必须红、圆角处必须蓝）。
- `bench.ps1`、`integration.ps1`：Windows 版，头部注明 UNTESTED。
- `ci.yml`：macos-14 + windows-latest 矩阵，`npm ci`、`npm run check`、`npm test`、`cargo test --workspace`、clippy、integration（Windows 的 continue-on-error）、`npx tauri build --debug`；不推远程。
- 在本机真跑所有能跑的脚本，把数字写进 `docs/BENCH.md`（标明是 lavfi pipeline 模式数字，不是真实抓屏）。

## 第 5 步：剩余文档
`docs/MANUAL-CHECKLIST.md`（人工清单，见下）、`docs/LICENSES.md`（MIT；ffmpeg LGPL 义务；Homebrew 构建是 GPL 不能随包）、`docs/TESTING.md`（测试金字塔表）、`docs/UX.md`（用户旅程流程图 + 需求 ↔ 组件对照）、`docs/PRIVACY.md`（双语：全部本地、CSP 禁外联）、`docs/WINDOWS-VERIFY.md`（Windows 机上一步步验证）、`docs/SYSTEM-AUDIO.md`（SCK 助手 / BlackHole / 不录 的对比表与推荐）。

## 第 6 步：Swift 助手 `helpers/sck-audio`
ScreenCaptureKit 只抓系统声音输出 s16le 48k 立体声 PCM 到 `--out <fifo>`；`--check`（权限没给 exit 2）、`--self-test`；`swift build -c release && swift test`；README 写 Rust 侧如何用（mkfifo → 助手 → ffmpeg `-f s16le -ar 48000 -ac 2 -i fifo`）。权限没给就老实报。

## 第 7 步：验收报告 `docs/reports/01-首版验收报告-2026-11-04.html`
中文、自包含 HTML、图形化：架构图、码率预算图（`400 MB × 8 ÷ 3600 s ≈ 0.89 Mbps`）、性能基准表、测试覆盖表、崩溃测试对照、Windows 未实测声明。**每个数字标来源等级 A/B/C/D + 核实日期**；末尾独立一节「我没把握的 2–3 条」+ 每条的验证动作。然后更新 `00-START-HERE.md`（阶段、下一步、报告编号列表）。

## 第 8 步：hours 库笔记
在 `../../../hours/30-项目/` 新建 `项目-screen-recorder.md`（frontmatter：`type: 项目`、`status: 进行中`、`部门: 工具`、`关系: 活跃`、`repo: 4-business/flowgt/screen-recorder`、`最近提交: <日期>`、`开始: 2026-11-04`），正文：为什么做、关键决定、时间线、下一步；在 `项目索引.md` 加一行。**hours 库不要 commit。**

## 只有 Gabriel 能做的（写进 MANUAL-CHECKLIST，做完告诉他）
1. 首次 `npm run tauri dev` 后点 屏幕录制 / 摄像头 / 麦克风 三个系统弹窗。
2. 真录 2 分钟（屏幕 + 画中画 + 麦克风），同时跑 `bash scripts/bench.sh --app --seconds 60`，把体积/时长/CPU 数字交回来补进报告。
3. Windows 机按 `docs/WINDOWS-VERIFY.md` 实测。
4. 决定是否建 GitHub 远程。
5. 系统声音：装 BlackHole（`brew install blackhole-2ch`，需重启）还是等 SCK 助手。

## 收尾
用中文给 Gabriel 一段总结：做了什么、测试汇总行（`npm test`、`cargo test` 原文）、基准数字表、没做成的、人工清单。
