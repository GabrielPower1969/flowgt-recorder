> **2026-11-12 Mac 上架版**：新增 `scripts/build-mas.sh`，打出符合 Mac App Store 要求的包（全部进沙箱、独立 LGPL ffmpeg、不用私有接口）。本机测试签名下真录通过；上传还差 Apple 证书。分支 `mac/app-store-sandbox`（草稿 PR），结果和留给 Gabriel 的事见 `docs/reports/06-Mac-上架版重构-2026-11-12.md`，英文说明见 `docs/MAC-APP-STORE.md`。
> **2026-11-11 Mac 验收（Faceplate ＋ 新图标）**：PR #15、#16 在 Mac 上合并、打包、真录通过；修了标题栏按钮压面板、窗口不能拖、屏幕名三处（分支 `mac/faceplate-fixes`，草稿 PR）。没修的和要人工看的见 `docs/reports/03-Mac验收-Faceplate-2026-11-11.md`。
> **Windows 状态（2026-11-11）**：第一次在真机上跑通。四项自动检查全绿（需合上 3 个修复）；屏幕＋摄像头＋麦克风能录成 HEVC 1080p 的 MP4，每小时约 388 MB，两次 30 分钟长录完整；修了 8 个问题。没有人看过或听过录像，音画同步没有用真实信号测，电脑声音录不了，CI 因 GitHub 账单问题没有运行。16 个草稿 PR 待审（#15 是按新设计重做的界面「Faceplate」，#16 是新的应用图标）。详见 `docs/reports/02-Windows通宵开发-2026-11-11.md`。
> **2026-11-11 合并（D31）**：桌面版和 Chrome 插件合进同一个公开仓库 `github.com/GabrielPower1969/flowgt-recorder`，插件在 `chrome-extension/`。Windows 负责人今晚在 winserver 上按 `AGENTS.md` 开工。
> **2026-11-10 提案（已提交）**：双人分工 `docs/COLLABORATION.md`；全平台目标架构 `docs/PLATFORM-ARCHITECTURE.md`（不换 Qt，留在 Tauri，每个平台一个采集引擎）；agent 入口 `AGENTS.md` ＋ 任务卡 `docs/tasks/`。许可证：陈广宇 2026-11-10 定为 MIT，不收费，`LICENSE` 不变。
> **2026-11-04 22:15 现状**：MacBook 版真实全流程跑通（抓屏 + 麦克风 + 系统声音 → 暂停 → 继续 → 停止 → 一个 MP4），界面已按 Google Meet 共享屏幕重做（D18）。
> **下一步**：① Gabriel 给桌面版授权，验证摄像头画中画；② Gabriel 建 Google OAuth 客户端（`docs/MANUAL-CHECKLIST.md` 顶部），才能测登录；③ Chrome 插件已于 2026-11-11 合入 `chrome-extension/`（D31）。Windows 零实测。
> 数字见 `docs/BENCH.md`，决定见 `docs/DECISIONS.md` D18–D20。下面的内容写于更早的阶段。

# 00 · START HERE

**现在的阶段（2026-11-04）**：首版可跑（core ＋ app ＋ 前端），待人工真机验收。`cargo test --workspace` 131 项通过
（core 112 ＋ app 19），clippy 干净，合成输入自检通过（A · 2026-11-04，见 `docs/BENCH.md`）；真屏幕录制、Windows 都还没测。macOS 可跑；Windows 只有代码与单元测试；
没有 GitHub 远程。

**一句话目标**：把会议、面试、演示在本机录成「屏幕 ＋ 摄像头小窗 ＋ 声音」的一个 MP4，一小时约 400 MB，
什么都不上传。

## Latest state (2026-11-05)

**Native capture landed (D27):** on macOS the `sck-record` helper (ScreenCaptureKit + AVAssetWriter) records screen,
camera, mic and system audio on one clock; ffmpeg only joins and mixes at the end. This replaces the chopped-sound defect
of D26 (measurements in D27 / report). The floating bar is no longer in the video.

**One app window (D28):** the picker has a second tab on macOS listing every on-screen window with its app icon and a
thumbnail; the helper records just that window (follows moves and resizes; closing it ends the recording and keeps the
file). Verified in the real app with the WeChat window. Not verified: pause/resume with native segments, long recordings,
Drive upload of a native file.

**Recording panel (D29):** the floating window now shows, under the bar, the live output frame (what is going into the
file, camera square included) with the name of the screen or window being recorded; a minimised recorded window
shows a notice until it is back. Collapsible to the bar with the chevron; draggable anywhere.

## 下一步

| # | 事 | 谁 | 怎么算完 |
|---|---|---|---|
| 1 | 本机点三个权限弹窗，跑一次 2 分钟真录（含一次暂停），对照期望值 | 陈广宇 | `docs/MANUAL-CHECKLIST.md` (a)(b) 全部打钩，结果填进报告 01 |
| 2 | Windows 机实测 | 陈广宇 ＋ Claude | 按 `docs/WINDOWS-VERIFY.md` 跑完，结果写成报告 02 |
| 3 | ScreenCaptureKit 原生视频路径（**只在** avfoundation 性能或画质不够时做） | Claude | `docs/BENCH.md` 给出 avfoundation 的数字后再决定 |
| 4 | 系统声音方案定案：SCK 助手 还是 BlackHole（见 `docs/SYSTEM-AUDIO.md`，决定 D7；目前建议先 BlackHole） | 陈广宇定，Claude 做 | `docs/DECISIONS.md` 新增 D17 |
| 5 | 是否建 GitHub 远程（私有？名字 `flowgt/screen-recorder`？） | 陈广宇 | 决定后告诉 Claude；决定 D14 改写 |
| 6 | LGPL ffmpeg 可复现静态构建进 `src-tauri/binaries/`，连同对应源码包与 configure 行一起存档 | Claude | `docs/LICENSES.md` 的发布清单全部打钩 |
| 7 | ~~窗口捕获（单窗口）~~ 已做：D28，macOS 原生路径 | Claude | ✔ 2026-11-05 |

顺序就是编号顺序；3 可能被跳过，7 最后。

## 最新报告

| # | 文件 | 内容 |
|---|---|---|
| 01 | `docs/reports/01-首版验收报告-2026-11-04.html` | **待写**：首版验收，测试、自检、基准、本机真录 |
| 03 | `docs/reports/03-Mac验收-Faceplate-2026-11-11.md` | Faceplate 界面与新图标在 Mac 上的逐项验收：命令输出、真录、修了什么、没修什么、要人工看的 |
| 06 | `docs/reports/06-Mac-上架版重构-2026-11-12.md` | Mac 上架版：沙箱、独立 LGPL ffmpeg、去掉私有接口；沙箱里的实测结果、没验证的、留给 Gabriel 的事（04、05 两份还在本机，未提交） |
| 02 | `docs/reports/02-Windows通宵开发-2026-11-11.md` | Windows 第一次真机：环境、修复、测试矩阵、体验问题、要人做的事 |

规则：报告按两位数编号，最大编号＝最新；每次新报告在这里加一行。

## 文件夹地图

| 位置 | 是什么 |
|---|---|
| `README.md` | 英文手册，末尾有中文速览 |
| `CLAUDE.md` | 给 LLM 和新人的地图：命令、目录、不变量、怎么验证、风险分级、加设置／加命令的菜谱 |
| `HOURS.md` | Hours 读本目录的唯一入口 |
| `docs/ARCHITECTURE.md` · `docs/DECISIONS.md` | 架构与决定（D1–D16），真相来源 |
| `docs/DESIGN.md` · `design/tokens.css` | 「Faceplate」视觉与设计令牌 |
| `docs/UX.md` | 用户旅程、每屏 UX 清单、快捷键、无障碍 |
| `docs/TESTING.md` | 测试金字塔：每层证明什么、证明不了什么 |
| `docs/BENCH.md` | 实测 CPU 与 MB/h |
| `docs/WINDOWS-VERIFY.md` | Windows 真机要跑什么 |
| `docs/SYSTEM-AUDIO.md` | SCK 助手 vs BlackHole |
| `docs/MANUAL-CHECKLIST.md` | **只有人能做的事**，带步骤和完成的样子 |
| `docs/PRIVACY.md` | 本地承诺、权限、文件在哪、怎么删 |
| `docs/LICENSES.md` | MIT ＋ ffmpeg LGPL 义务 ＋ 第三方声明 |
| `docs/reports/` | 编号报告 |
| `src/` | Svelte 前端；`src/lib/types.ts` 是 IPC 契约 |
| `src-tauri/src/` | Rust 应用层：进程监督、权限、托盘、快捷键 |
| `src-tauri/crates/core/` | 纯 Rust 核心：码率预算、ffmpeg 命令、解析、恢复 |
| `helpers/sck-audio/` | Swift 系统声音助手（实验） |
| `scripts/` · `tests/` | `fetch-ffmpeg.sh` · `integration.sh` · `bench.sh` 与测试素材 |

## 人工清单

只有人能做的事都在 `docs/MANUAL-CHECKLIST.md`：权限弹窗、2 分钟真录、拔摄像头、`kill -9`、Windows 真机、
GitHub 远程、BlackHole、发布版 ffmpeg 的下载与校验。每项写了为什么只能人做、步骤、完成的样子、结果填哪里。

## 决定日志

`docs/DECISIONS.md`，格式 `D<n> · 日期 · 决定 — 为什么 — 后果`，只追加不删除。现在到 D16（D15 core 接手决定，D16 app crate 设计）。
