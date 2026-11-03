# 人工清单 · 只有人能做的事

> 这是 FlowGT Recorder 里 Claude 做不了、必须陈广宇亲手做的事。每项写了**为什么只能人做、步骤、完成的样子、结果填哪里**。
> 做完一项就把 `[ ]` 改成 `[x]` 并写上日期；结果数字填进当期报告（第一份是
> `docs/reports/01-首版验收报告-2026-11-04.html`，整合者负责生成）。
> 本清单写于 2026-11-04，当时代码还在并行搭建，「期望值」是按设计推出来的，不是实测——实测以报告为准。

## 速查勾选（人工项，细节见下面各节）

- [ ] 首次 `npm run tauri dev` 时点掉三个 TCC 弹窗（屏幕录制 ／ 摄像头 ／ 麦克风）——见 (a)
- [ ] 真录 2 分钟，回报体积、时长、CPU（含 `bash scripts/bench.sh --app --seconds 60`）——见 (b)
- [ ] Windows 真机验证（按 `docs/WINDOWS-VERIFY.md`）——见 (e)
- [ ] 决定是否建 GitHub 远程（私有／公开、账号、仓库名）——见 (f)
- [ ] 系统声音：BlackHole 还是 SCK 助手的决定（见 `docs/SYSTEM-AUDIO.md`，目前建议先 BlackHole）——见 (g)

| # | 事 | 状态 | 日期 |
|---|---|---|---|
| a | macOS 三个权限弹窗 | [ ] | |
| b | 2 分钟真录验收（含一次暂停）＋ 同时跑 bench | [ ] | |
| c | 拔摄像头测试 | [ ] | |
| d | `kill -9` 崩溃恢复测试 | [ ] | |
| e | Windows 真机实测 | [ ] | |
| f | 是否建 GitHub 远程 | [ ] | |
| g | 系统声音：装 BlackHole 还是等 SCK 助手 | [ ] | |
| h | 发布用 LGPL ffmpeg 的下载与校验 | [ ] | |

---

## (a) macOS 权限：屏幕录制 ／ 摄像头 ／ 麦克风

**为什么只能人做。** 这三个是 macOS 的 TCC 系统对话框，任何自动化（包括 Claude）点了都等于绕过系统安全，
而且屏幕录制根本没有「允许」按钮——只能去「系统设置」里手动勾。

**步骤。**

1. 在仓库目录运行 `npm run tauri dev`。第一次启动会进首次向导（Onboarding）。
2. 向导第一步检查 ffmpeg：应显示找到的路径和版本（开发模式下是 `/opt/homebrew/bin/ffmpeg` 7.1.1）。
3. 向导到「屏幕录制」：点「请求权限」。macOS 不会弹「允许」，而是弹「“××” 想要录制此电脑的屏幕」并给一个
   「打开系统设置」按钮。点它，到 **系统设置 → 隐私与安全性 → 屏幕录制**（macOS 15 起叫
   「屏幕与系统音频录制」），把对应条目**打开**。
4. **授权对象会是哪一个？两种情况都正常：**
   - **开发模式**（`npm run tauri dev`）：二进制 `src-tauri/target/debug/flowgt-recorder` 没有签名，
     macOS 会把权限算到**启动它的终端**头上——列表里出现的可能是「终端」「iTerm」「Claude」「Visual Studio Code」
     或直接是 `flowgt-recorder`。哪个出现就勾哪个；换一个终端启动就要再勾一次。
   - **打包版**（`npm run tauri build` 出来的 `FlowGT Recorder.app`）：列表里就叫「FlowGT Recorder」。
5. 屏幕录制勾完必须**退出并重开 app**，权限才生效（macOS 的规矩，向导会提示）。
6. 向导到「摄像头」：这个有正常的弹窗，文案是
   “FlowGT Recorder shows your camera as a picture-in-picture in your recordings.” → 点「好」。
7. 向导到「麦克风」：弹窗文案 “FlowGT Recorder records your voice together with the screen.” → 点「好」。
8. 回到向导，三项都应显示「已授权」；点「完成」进主窗口。

**完成的样子。** 主窗口左侧预览里能看到自己的摄像头画面，麦克风电平表随说话跳动；设置 → 权限 三项全绿。
`系统设置 → 隐私与安全性` 的 屏幕录制／摄像头／麦克风 三个列表里都能找到对应条目且已打开。

**结果填哪里。** 报告 01「权限」一节：三项各自的状态、授权对象实际显示的名字（终端还是 app）、macOS 版本号。

---

## (b) 2 分钟真录验收

**为什么只能人做。** 真录需要真实屏幕内容、真摄像头、真人说话，还要在录制中按快捷键；而且要有人看结算页上的数字
是否和预期一致。Claude 可以陪着看日志，但不能替你按键。

**准备。**

- 开一个**有动态内容**的网页（比如一段在播放的视频或不停滚动的页面），放在主显示器上。
- 摄像头开、麦克风开、系统声音先选「关」（系统声音另见 (g)）。
- 会议名填 `验收 b`，看文件名预览应是 `2026-11-04 HHMM 验收 b.mp4` 这样的格式。
- 另开一个终端，准备好但先不运行：`bash scripts/bench.sh --app --seconds 60`。

**步骤。**

1. 按 `⌘⇧R`。应出现 3-2-1 倒计时，主窗口隐藏，悬浮条出现，托盘图标变红点。
2. 录到约 0:30 时，在终端运行 `bash scripts/bench.sh --app --seconds 60`，让它测 60 秒。
3. 录到约 1:00 时按 `⌘⇧P` 暂停一次，悬浮条应显示「已暂停」，时长停住；等 10 秒再按 `⌘⇧P` 继续。
4. 全程对着麦克风说几句话，并在屏幕上滚动、切窗口。
5. 总时长到约 2:10（含 10 秒暂停）时按 `⌘⇧R` 停止。应短暂显示「正在完成」然后进结算页（Summary）。
6. 在结算页点「在 Finder 中显示」，用 QuickTime 打开文件从头放到尾，确认声音、画面、小窗都在。

**期望值表。** 按 400 MB/h 预算推算（`889 kbps × 120 s ÷ 8 ≈ 13.3 MB` 是上限附近）；CPU 与内存以
`docs/BENCH.md` 的实测为参照，测之前留空。

| 看什么 | 期望 | 实际 |
|---|---|---|
| 时长 | 约 120 s（暂停的 10 s 不计入）；暂停处允许 < 1 s 的接缝 | |
| 体积 | ≤ 约 14 MB；动态网页 ＋ 摄像头多在 8–14 MB；全静态可能只有 3–6 MB | |
| 与预算比（over budget） | ≤ +5 %；为负是正常的（静态省出来的） | |
| 分辨率 | 1920 宽，高度按屏幕比例，偶数 | |
| 帧率 | ≈ 30 fps，丢帧（drop_frames）≈ 0 | |
| 视频编码器 | `hevc_videotoolbox`（Apple Silicon／Intel 均为硬件） | |
| 音轨数 | 1（mixed） | |
| 片段数 | 2（一次暂停 ＝ 两段） | |
| 文件名 | `2026-11-04 HHMM 验收 b.mp4`，在 `~/Movies/FlowGT Recorder/` | |
| 工作目录 | 拼接成功后 `~/Movies/FlowGT Recorder/.flowgt-recorder/<session>/` 应被清理；若还在，记下来 | |
| bench `--app` 60 s | ffmpeg CPU %、app CPU %、内存——抄 `bench-out/` 的输出 | |
| 悬浮条 | 时长每秒走、已写入 MB 增长、预计总量接近实际 | |

**完成的样子。** 上表「实际」列填满，QuickTime 能完整播放，声音与画面同步，小窗在选定的角上。

**结果填哪里。** 报告 01「真录」一节 ＋ `docs/BENCH.md` 的 `--app` 行。

---

## (c) 拔摄像头测试

**为什么只能人做。** 要物理拔掉一个 USB 摄像头（或断开 iPhone 连续互通相机）。内置 FaceTime 相机拔不掉，所以
需要一个外接摄像头。

**步骤。**

1. 设置里把摄像头选成外接的那个，开始一段录制（会议名 `验收 c`），录 20 秒。
2. 直接拔掉 USB 线（或在 iPhone 上断开连续互通）。
3. 观察悬浮条和 Toast；再录 20 秒后 `⌘⇧R` 停止。

**完成的样子。** 录制**不中断**；Toast 出现「摄像头已断开」（`warn.cameraLost`）；悬浮条上摄像头状态变灰；
最终文件完整可播，时长约 40 s。小窗在拔掉后的表现（停在最后一帧、还是消失、还是黑块）由应用层实现决定——
**如实记下看到的**，这决定下一版要不要改。

**结果填哪里。** 报告 01「异常」一节。

---

## (d) `kill -9` 崩溃恢复测试

**为什么只能人做。** 要在真录中途杀进程，然后重开 app 走恢复流程；两步都会触发窗口和权限行为，要人看着。

**步骤（变体一：杀 ffmpeg）。**

```bash
# 先开始一段录制（会议名 验收 d1），录 30 秒以上，然后在另一个终端：
pgrep -fl ffmpeg          # 应能看到正在录的 ffmpeg 进程
pkill -9 -f ffmpeg        # 模拟 ffmpeg 崩溃
```

观察 app 的反应（报错 Toast？自动开新段？），记下来。然后**完全退出 app**（托盘 → 退出），重新
`npm run tauri dev`，进 **设置 → 恢复**，应列出 `验收 d1`，点「恢复」。

**步骤（变体二：杀整个 app）。**

```bash
# 开始另一段录制（会议名 验收 d2），录 30 秒以上，然后：
pkill -9 -f flowgt-recorder   # 连带 ffmpeg 一起死
```

重开 app → 设置 → 恢复 → 应列出 `验收 d2` → 点「恢复」。

**完成的样子。** 两种变体都能恢复出可播放的 MP4，时长 ≈ 杀进程前的录制时长（最多少 2 秒左右，即最后一个
未写完的片段）；结算页上 `recovered` 显示为是；恢复后 `.flowgt-recorder/<session>/` 被清理。
「丢弃」按钮能删掉另一条不想要的记录。

**结果填哪里。** 报告 01「崩溃恢复」一节：两种变体各自的实际时长、文件大小、是否可播。

---

## (e) Windows 真机实测

**为什么只能人做。** 这里没有 Windows 机器；所有 Windows 代码只过了单元测试（决定 D8）。要有人在真机上装工具链、
点隐私开关、跑录制。

**步骤。** 全部按 `docs/WINDOWS-VERIFY.md` 来，大致是：装 Rust ＋ Visual Studio C++ 生成工具 ＋ Node ＋
WebView2 → 取 Windows 版 LGPL ffmpeg → `npm run tauri dev` → 设置 → 隐私 允许摄像头和麦克风 → 跑 (b) 的同一套。

**完成的样子。** `docs/WINDOWS-VERIFY.md` 里的表全部填完，写成 `docs/reports/02-Windows实测-<日期>.html`。

---

## (f) 是否建 GitHub 远程

**为什么只能人做。** 建仓库、取名、选公开私有，是所有者的决定（决定 D14）；Claude 不能替你推代码到互联网。

**要决定的三件事。**

1. **私有还是公开？** 公开意味着 MIT 代码任何人可用，`docs/LICENSES.md` 的 ffmpeg 声明必须到位。
2. **放哪个账号？** 组织 `flowgt`（仓库名 `flowgt/screen-recorder`）还是个人 `GabrielPower1969`？
3. **叫什么？** `screen-recorder` 还是 `flowgt-recorder`——注意 `flowgt-recorder` 已经是旧的 Chrome native host 项目名，
   两个仓库同名会混。

**完成的样子。** 把三个答案告诉 Claude；Claude 改写 D14 为新决定、`git remote add`、**经你确认后**才 `git push`。
`src-tauri/binaries/` 不进仓库。

---

## (g) 系统声音：装 BlackHole 还是等 SCK 助手

**为什么只能人做。** 装 BlackHole 是装一个系统级虚拟音频驱动，要输入密码并**重启电脑**；SCK 助手则要在真会议里
验证能不能稳定收到对方声音。两条路都要人在机器前，而且选哪条是产品决定（D7 悬而未决）。

**选项 A · BlackHole（成熟、要装驱动）。**

```bash
brew install blackhole-2ch     # 装完提示重启
```

重启后打开「音频 MIDI 设置」，建一个**多输出设备**（内置输出 ＋ BlackHole 2ch），系统输出选它——这样你自己还能
听到声音，BlackHole 同时把声音喂给录制。app 设置 → 系统声音 → 选「BlackHole 2ch」。

**选项 B · ScreenCaptureKit 助手（原生、实验）。** 不装任何东西；app 设置 → 系统声音 → 「自动」或「助手」，
它用屏幕录制权限直接抓系统音频。在一个真的 Zoom／Teams／Meet 通话里录 2 分钟，看对方声音是否完整、有没有断。
细节见 `docs/SYSTEM-AUDIO.md`。

**完成的样子。** 决定写进 `docs/DECISIONS.md` 成 D17（D15、D16 已被占用；选 A、选 B、还是「A 为默认 B 继续试验」），并把 (b) 用选定的
系统声音再录一次，确认音轨里有对方的声音。

---

## (h) 发布用 LGPL ffmpeg 构建的下载与校验

**为什么只能人做。** 从互联网下载并信任一个二进制，是所有者的判断；还要亲眼确认它的许可证输出，这是发布合规
（`docs/LICENSES.md`）的证据。开发用的 Homebrew ffmpeg 是 GPL 版，**绝不能随 app 发布**。

**步骤。**

1. 看 `scripts/fetch-ffmpeg.sh` 里写的下载地址和 SHA256，确认来源你认可。
2. 运行 `bash scripts/fetch-ffmpeg.sh`，它把文件放到 `src-tauri/binaries/ffmpeg-<目标三元组>` 与 `ffprobe-<目标三元组>`。
3. 校验：

```bash
shasum -a 256 src-tauri/binaries/ffmpeg-*            # 对照脚本里的值
src-tauri/binaries/ffmpeg-aarch64-apple-darwin -L | head -30
#   必须看到 "GNU Lesser General Public License"；
#   configuration 行里不得出现 --enable-gpl、--enable-nonfree、--enable-libx264、--enable-libx265
src-tauri/binaries/ffmpeg-aarch64-apple-darwin -hide_banner -encoders | grep -E "videotoolbox|openh264|aac"
#   要有 hevc_videotoolbox、h264_videotoolbox、aac
```

4. 把该二进制对应的**源码包和 configure 行**存到你指定的归档位置（LGPL 要求发布者能提供与二进制完全对应的源码）。

**完成的样子。** `-L` 输出是 LGPL，配置行干净，SHA256 对得上，源码包有存档位置，`docs/LICENSES.md` 的发布清单
全部打钩。

**结果填哪里。** `docs/LICENSES.md` 发布清单 ＋ 报告 01「发布准备」一节（版本号、SHA256、来源）。
