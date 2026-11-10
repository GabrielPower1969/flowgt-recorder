---
type: 接口
领域: 一人公司
名称: screen-recorder · FlowGT Recorder
路径: 4-business/flowgt/screen-recorder
更新: 2026-11-04
---
# screen-recorder · FlowGT Recorder

## 这是什么

一个只在本机工作的桌面录屏软件：屏幕 ＋ 摄像头小窗 ＋ 麦克风（系统声音为实验功能），硬件 HEVC 编码，
一小时约 400 MB，macOS 与 Windows 双平台。Tauri 2 ＋ Rust 纯核心 ＋ Svelte 5，ffmpeg 作旁车进程。
没有任何网络代码，什么都不上传。用途：把会议、面试、演示录成能直接喂给 `interview-analysis` 管线的文件。

2026-11-11 起（D31）：桌面版和 Chrome 插件合成**一个公开仓库** `GabrielPower1969/flowgt-recorder`，插件在
`chrome-extension/`。本机旧目录 `4-business/flowgt/flowgt-recorder/` 只是合并前的插件副本，以后不再在那里改。

## 谁负责

陈广宇（Gabriel Chen）。代码由多个并行 agent 在 2026-11-04 搭起，文件归属见 `docs/ARCHITECTURE.md`。

## 状态（2026-11-04）

v0.1 首版，验收中。macOS 已构建、本机在测；Windows 只有代码和单元测试，**没在真机跑过**；系统声音方案未定案；
窗口捕获还没有；没有 GitHub 远程，只有本地提交。现在的阶段和下一步以 `00-START-HERE.md` 为准。

## 给 Hours 索引什么

| 读什么 | 拿什么 |
|---|---|
| `00-START-HERE.md` | 现在的阶段、下一步编号列表、最新报告编号 |
| `docs/reports/NN-*.html` | 编号报告；最大编号＝最新。第一份是 `01-首版验收报告-2026-11-04.html` |
| `docs/DECISIONS.md` | D1–D14 决定日志，写进项目笔记的「关键决定」 |
| `docs/MANUAL-CHECKLIST.md` | 等陈广宇做的人工项，适合进晨报的「待办」 |
| `git log --oneline` | `最近提交` 属性 |

项目笔记属性建议：`type: 项目` · `部门: 一人公司·FlowGT` · `关系: 活跃` · `status: 进行中` ·
`repo: 4-business/flowgt/screen-recorder`。

## 规矩

- 代码事实（结构、命令、架构、测试）留在仓库：`README.md`、`CLAUDE.md`、`docs/`。Hours 只记**为什么、时间线、决定、钱**。
- 本文件是 Hours 读取本目录的唯一入口；改目录结构先改这里和 `CLAUDE.md` 的目录地图。
- 路径一律相对本仓库或 workspace 根目录（`4-business/flowgt/screen-recorder`）；禁止绝对路径。
- 录像文件本体永不进 git，也永不进 Hours；Hours 只记路径和时长。
