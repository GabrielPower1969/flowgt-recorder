> **2026-11-04 22:15 现状**：MacBook 版真实全流程跑通（抓屏 + 麦克风 + 系统声音 → 暂停 → 继续 → 停止 → 一个 MP4），界面已按 Google Meet 共享屏幕重做（D18）。
> **下一步**：① Gabriel 给桌面版授权，验证摄像头画中画；② Gabriel 建 Google OAuth 客户端（`docs/MANUAL-CHECKLIST.md` 顶部），才能测登录；③ Chrome 插件在 `chrome-extension` 分支（独立会话），做完合并。Windows 零实测。
> 数字见 `docs/BENCH.md`，决定见 `docs/DECISIONS.md` D18–D20。下面的内容写于更早的阶段。

# 00 · START HERE

**现在的阶段（2026-11-04）**：首版可跑（core ＋ app ＋ 前端），待人工真机验收。`cargo test --workspace` 131 项通过
（core 112 ＋ app 19），clippy 干净，合成输入自检通过（A · 2026-11-04，见 `docs/BENCH.md`）；真屏幕录制、Windows 都还没测。macOS 可跑；Windows 只有代码与单元测试；
没有 GitHub 远程。

**一句话目标**：把会议、面试、演示在本机录成「屏幕 ＋ 摄像头小窗 ＋ 声音」的一个 MP4，一小时约 400 MB，
什么都不上传。

## Latest state (2026-11-05)

**Open defect:** with the camera on, recorded sound is chopped. Camera off works: whole recording, 1 silent window of
168, sound about 0.4 s ahead of the picture. See `docs/DECISIONS.md` D26. Waiting on Gabriel: camera in its own process,
or native capture. Single-app recording needs native capture.

## 下一步

| # | 事 | 谁 | 怎么算完 |
|---|---|---|---|
| 1 | 本机点三个权限弹窗，跑一次 2 分钟真录（含一次暂停），对照期望值 | 陈广宇 | `docs/MANUAL-CHECKLIST.md` (a)(b) 全部打钩，结果填进报告 01 |
| 2 | Windows 机实测 | 陈广宇 ＋ Claude | 按 `docs/WINDOWS-VERIFY.md` 跑完，结果写成报告 02 |
| 3 | ScreenCaptureKit 原生视频路径（**只在** avfoundation 性能或画质不够时做） | Claude | `docs/BENCH.md` 给出 avfoundation 的数字后再决定 |
| 4 | 系统声音方案定案：SCK 助手 还是 BlackHole（见 `docs/SYSTEM-AUDIO.md`，决定 D7；目前建议先 BlackHole） | 陈广宇定，Claude 做 | `docs/DECISIONS.md` 新增 D17 |
| 5 | 是否建 GitHub 远程（私有？名字 `flowgt/screen-recorder`？） | 陈广宇 | 决定后告诉 Claude；决定 D14 改写 |
| 6 | LGPL ffmpeg 可复现静态构建进 `src-tauri/binaries/`，连同对应源码包与 configure 行一起存档 | Claude | `docs/LICENSES.md` 的发布清单全部打钩 |
| 7 | 窗口捕获（单窗口），需要原生 ScreenCaptureKit／WGC 路径（决定 D9） | Claude | UI 里的「窗口」选项可用 |

顺序就是编号顺序；3 可能被跳过，7 最后。

## 最新报告

| # | 文件 | 内容 |
|---|---|---|
| 01 | `docs/reports/01-首版验收报告-2026-11-04.html` | **待写**：首版验收，测试、自检、基准、本机真录 |

规则：报告按两位数编号，最大编号＝最新；每次新报告在这里加一行。

## 文件夹地图

| 位置 | 是什么 |
|---|---|
| `README.md` | 英文手册，末尾有中文速览 |
| `CLAUDE.md` | 给 LLM 和新人的地图：命令、目录、不变量、怎么验证、风险分级、加设置／加命令的菜谱 |
| `HOURS.md` | Hours 读本目录的唯一入口 |
| `docs/ARCHITECTURE.md` · `docs/DECISIONS.md` | 架构与决定（D1–D16），真相来源 |
| `docs/DESIGN.md` · `design/tokens.css` | 「Studio」视觉与设计令牌 |
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
