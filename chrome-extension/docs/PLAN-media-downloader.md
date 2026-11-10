# 执行计划：B 站 / YouTube 一键下载功能（v0.3.0）

> **执行状态（2026-09-30）：已完成。** 单元 81/81 · host 集成 38/38 · E2E 48/48，两平台真实冒烟通过。
> 与原计划的偏离（均有实测依据，详见 ARCHITECTURE §8）：
> 1. yt-dlp 用 pip 装进独立 venv——`brew install` 被 Xcode 许可协议卡住（需 sudo）。
> 2. 视频格式由 `bestvideo+bestaudio` 改为 `bv*+ba[ext=m4a]/bv*+ba/b`——前者实测合出 Opus 音轨，QuickTime 无声。
> 3. YouTube 追加 `--js-runtimes node` + `yt-dlp[default]`——否则 yt-dlp 警告缺格式，拿不到真正最高画质。
> 4. **Native Messaging 做了 E2E**（原计划说不做）——官方文档确认用户级 host 在 profile 目录查找，CfT 临时 profile 可装测试 host。
> 5. 下载目录由 host 决定、不由扩展传入（安全收紧）。
> 6. CWS 禁令从 D 级升为 A 级（官方排障页原文）。

> **给执行者（Opus）**：这是完整规格，按 §8 顺序做即可。所有踩坑结论已在 §2 固化，
> 不要重新发明方案。做完必须满足 §7 验收清单。仓库现状：`main @ 7e15e72`，
> 工作区有一个未提交的 `docs/member-guide.html`（第一步先单独提交它）。
> 本仓库注释/文档语言习惯：中文注释 + 英文 UI，保持一致。

## 0. 需求（用户原话拆解）

- popup 里新增下载功能：**自动识别**当前标签页是 B 站还是 YouTube（用平台 logo 示意，不让用户选平台）。
- 用户只选 **音频 / 视频**；视频默认最高质量，音频也是。
- 下载到本地。

## 1. 关键决策（已定，不要改）

| 决策 | 理由 |
|---|---|
| 扩展只做 UI，真正下载交给本机 **yt-dlp**（经 Native Messaging host） | 扩展内解析 YouTube/B 站流不可行（签名加密常变、DASH 音视频分离需 ffmpeg 合流）。yt-dlp + ffmpeg 是唯一可维护路线，顺便把二期 Native host 地基打了 |
| **此功能永不进 Chrome Web Store 版和成员 zip** | CWS 政策禁止 YouTube 下载类扩展、YouTube ToS 禁止下载 `[D · 业界通行认知 · 上架前 Opus 须 fetch developer.chrome.com 政策原文核对并在 ARCHITECTURE.md 标注来源]`。这是自用/开发版功能。成员 zip（v0.2.0 已发出）不更新 |
| 视频容器 mp4，质量优先 | `-f bestvideo+bestaudio/best`。注意：YouTube 4K 多为 VP9/AV1，装进 mp4 后 QuickTime 可能不播（IINA/VLC 可以）——在 README 里写明，不为此牺牲画质 |
| 音频统一转 m4a | `-f bestaudio -x --audio-format m4a --audio-quality 0`（ffmpeg 已装） |
| B 站默认带 Chrome cookies，YouTube 默认不带 | B 站未登录拿不到 1080P+ `[C · 社区共识 · 落地后实测确认]`。用 `--cookies-from-browser chrome`；macOS 首次会弹 "Chrome Safe Storage" 钥匙串授权 `[C · 待实测]`，README 写明。popup 给开关 |
| 下载目录 `~/Downloads/flowgt-downloads/` | 与录音目录 `flowgt-recordings/` 平行，不混 |

## 2. 已核实事实与踩坑（执行时直接引用）

- 本机：ffmpeg 7.1.1 ✅（/opt/homebrew/bin/ffmpeg）、node ✅（/opt/homebrew/bin/node）、
  **yt-dlp 未安装** → 第一步 `brew install yt-dlp`。`[A · 本机 which 实测 · 2026-09-30]`
- Native Messaging（Opus 动手前 fetch 官方文档核对一遍协议细节）：
  - 消息帧：**4 字节小端长度前缀 + UTF-8 JSON**，走 stdin/stdout；host 由 Chrome 启动，
    **无 login shell PATH** → manifest 的 `path` 指向一个 shell wrapper，里面写死
    `/opt/homebrew/bin/node` 绝对路径（install 脚本探测后生成）。
  - macOS 用户级 manifest 位置：
    `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/<host名>.json`
    `[B · developer.chrome.com nativeMessaging 文档 · 需 Opus 逐字核对]`
  - host 名定为 `nz.co.flowgt.recorder`；manifest `allowed_origins: ["chrome-extension://<ID>/"]`。
  - **unpacked 扩展 ID 不固定**：install 脚本用 node 解析
    `~/Library/Application Support/Google/Chrome/*/Preferences` 与 `Secure Preferences`
    的 `extensions.settings`，找 `manifest.name === "FlowGT Recorder"` 的 key 自动拿 ID；
    找不到就提示用户从 `chrome://extensions` 复制 ID 作为脚本参数。
- 本仓库既有铁律（docs/ARCHITECTURE.md §4）：E2E 用 Chrome for Testing（`.browsers/` 已 pin 152）；
  **不要**给 Native Messaging 写 E2E——CfT 的 manifest 查找路径与正式版 Chrome 不同且未核实，
  改用 Node 直接 spawn host 进程做集成测试（见 §6）。
- 防注入：host 里 yt-dlp 一律 `execFile`（参数数组），**绝不拼 shell 字符串**；
  下载前校验 URL host 白名单：`youtube.com / www|m|music.youtube.com / youtu.be /
  bilibili.com / www|m.bilibili.com / b23.tv`，白名单外直接拒绝——防扩展被攻破后 host 被滥用。

## 3. 消息协议（popup ↔ background ↔ host）

popup → background（`chrome.runtime.sendMessage`，沿用现有 `{target:'background', type}` 风格）：

| type | 载荷 | 语义 |
|---|---|---|
| `download-media` | `{url, kind:'video'\|'audio', useCookies:bool}` | 启动下载；已有下载在跑则拒绝（同录音的双启守卫） |
| `cancel-download` | — | kill 当前 yt-dlp |
| （状态读取） | — | popup 直接读 `storage.session` 的 `dl` |

background ↔ host（`chrome.runtime.connectNative('nz.co.flowgt.recorder')` 长连接）：

| 方向 | 消息 | 说明 |
|---|---|---|
| bg→host | `{type:'ping'}` → host 回 `{type:'pong', ytdlp:'2025.x'\|null}` | 检测 host 与 yt-dlp 是否就绪 |
| bg→host | `{type:'download', url, kind, useCookies, outDir}` | outDir 固定 `~/Downloads/flowgt-downloads` |
| host→bg | `{type:'progress', percent, speed, eta}` | 解析 yt-dlp `--newline` 输出，**节流 ≤2 条/秒** |
| host→bg | `{type:'done', file}` / `{type:'error', msg}` | 完成/失败 |

background 把状态镜像到 `storage.session.dl = {active, platform, kind, percent, speed, file, error, at}`，
popup 轮询/`storage.onChanged` 刷新。host 未安装（connectNative 报错）→ `dl.error='host-missing'`，
popup 显示 "Local helper not installed" + 指向 README 安装一节。

## 4. 文件清单

**新增：**

| 文件 | 内容 |
|---|---|
| `host/flowgt-host.mjs` | Native host（Node）：帧编解码、URL 白名单、execFile 调 yt-dlp、进度解析节流、`FLOWGT_YTDLP` 环境变量可注入 stub（测试用）。视频参数：`-f bestvideo+bestaudio/best --merge-output-format mp4 --no-playlist --newline -o "<outDir>/%(title).80s [%(id)s].%(ext)s"`；音频参数：`-f bestaudio -x --audio-format m4a --audio-quality 0 --no-playlist --newline -o 同上`；B 站且 useCookies → 追加 `--cookies-from-browser chrome` |
| `host/install.sh` | ① brew 检查/提示装 yt-dlp ② 探测 node 绝对路径生成 wrapper `flowgt-host.sh` ③ 自动探测扩展 ID（§2 方法，可用 `$1` 覆盖）④ 生成 manifest JSON 写入 Chrome NativeMessagingHosts 目录 ⑤ chmod +x ⑥ 自检：直接 spawn host 发 ping |
| `extension/platform.js` | 纯函数 `detectPlatform(url) → 'youtube'\|'bilibili'\|null`（IIFE 挂 `FlowGTPlatform`，同 recorder-core 风格，vm 可加载做单测） |
| `test/host.test.mjs` | host 集成测试：spawn 真 host 进程 + stub yt-dlp（脚本打印假进度/假产物），验证 ping/pong、帧编解码、progress 节流、done 带文件名、白名单拒绝 `evil.com`、stub 非零退出 → error |

**修改：**

| 文件 | 改动 |
|---|---|
| `extension/manifest.json` | permissions 加 `nativeMessaging`；version → `0.3.0` |
| `extension/background.js` | 新增 `download-media`/`cancel-download` 分支 + host 端口管理（复用 lastError/状态机模式） |
| `extension/popup.html` | 录音卡下方新增 Download 卡：平台徽标（内联 SVG：B 站电视 icon `#00AEEC`、YouTube 播放键 `#FF0000`，深浅色模式都可读）+ 视频标题行（tab.title）+ 两按钮 `Video (best)` / `Audio (m4a)` + cookies 开关（仅 B 站显示，默认开）+ 进度条（`--signal` 填充、8px 圆角，遵守 VI：signal 上文字用 `--ink`）+ 错误/完成态。**非 B 站/YouTube 页整卡隐藏**，现有录音 UI 一像素不动 |
| `extension/popup.js` | `tabs.query` 当前页 → `FlowGTPlatform.detectPlatform`；支持 `?demo=youtube\|bilibili` 查询参数强制显示（截图/评审用，生产无害）；下载按钮/取消/进度渲染 |
| `test/unit.mjs` | 追加 detectPlatform 用例：watch/shorts/youtu.be/music、BV 页/b23.tv、子域、非平台页 null、`chrome://` null |
| `package.json` | `test:host` 脚本；`test` 串联 unit+host+e2e |
| `test/screenshot-ui.mjs` | 追加 `popup.html?demo=youtube` 与 `?demo=bilibili`（明暗各一）出图到 design-shots/ |
| `README.md` | 新增 "Media downloader (dev build only)" 一节：安装 host 步骤、边界（CWS 版不含此功能、4K VP9 播放器提示、B 站 1080P+ 需登录）、合规提醒（个人用途，遵守平台条款） |
| `docs/ARCHITECTURE.md` | §2 组件图加 host；新增 §8 下载器（协议表、白名单、store 构建剔除清单：`test-harness.*`、`host/`、popup 下载卡代码） |

## 5. UI 规格（FlowGT VI，英文文案）

- 卡片：`--surface` 底、`--line` 描边、12px 圆角，与现有 popup 语言一致。
- 平台徽标行：`[SVG logo 16px] YouTube · <标题截断一行>`；logo 用各平台官方色，不改色。
- 按钮：次级样式（描边 accent，不抢录音主按钮的 signal 绿）；下载中变进度条 + Cancel。
- 完成态：`✓ Saved to Downloads/flowgt-downloads/`；错误态 `--danger` 文案。
- **完成后跑 screenshot-ui.mjs，把 4 张新截图发给用户过目**（DESIGN_COLLABORATION 铁律：可见视觉必须创始人过目）。

## 6. 测试与验证

1. `npm run test:unit` — 含新增 detectPlatform 用例，全绿。
2. `npm run test:host` — stub 集成测试全绿（不碰网络）。
3. `npm run test:e2e` — 既有 25 项必须仍然 25/25（下载功能不进 E2E，原因见 §2）。
4. **真实冒烟（本机、小体积）**：装完 yt-dlp 后
   `yt-dlp -f bestaudio -x --audio-format m4a -o '<scratchpad>/%(id)s.%(ext)s' <一条 CC 授权短视频>`
   → ffprobe 验证产物是 aac/m4a。B 站同样来一条短视频冒烟。
5. `host/install.sh` 在用户真 Chrome 上执行 + 自检 ping 通过。
6. **留给用户的 1 分钟验收**（写在最终回复里）：真 Chrome 刷新扩展 → 开任一 YouTube 页 →
   popup 应出现 YouTube 徽标 → 点 `Audio (m4a)` → 文件落 `~/Downloads/flowgt-downloads/`。

## 7. 验收清单

- [ ] `docs/member-guide.html` 已单独提交（独立 commit，在功能 commit 之前）
- [ ] yt-dlp 已装且 host ping 报出版本号
- [ ] YouTube / B 站页面 popup 自动亮对应 logo；其他页面下载卡不出现；录音功能零回归
- [ ] 视频=最高质量 mp4、音频=m4a；文件名含标题、Windows 非法字符由 yt-dlp 自净
- [ ] 同时只允许一个下载；取消可用；host 缺失有清晰引导
- [ ] URL 白名单拒绝非平台域；yt-dlp 调用无 shell 拼接
- [ ] 三层测试全绿 + 两平台真实冒烟通过
- [ ] 4 张 UI 截图已发用户
- [ ] README + ARCHITECTURE 更新；记忆文件 `flowgt-recorder-project.md` 更新（新结论：host 架构落地、CWS 剔除清单）
- [ ] 两个 commit 已 push：`docs: member guide` + `v0.3.0 — bilibili/youtube downloader via native yt-dlp host`（末尾带 Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>）

## 8. 执行顺序

S0 提交 member-guide → S1 `brew install yt-dlp` + 冒烟 → S2 `host/`（host+install+测试 stub）→
S3 `test/host.test.mjs` 跑绿 → S4 扩展改动（platform.js → manifest → background → popup）→
S5 unit.mjs 扩用例跑绿 → S6 E2E 回归 25/25 → S7 截图出图发用户 → S8 真机 install.sh + 两平台冒烟 →
S9 文档 + 记忆 → S10 commit ×2 + push。

## 9. 明确不做

- 不写 Native Messaging 的 E2E；不动成员 zip；不动录音链路任何代码；
- 不做播放列表批量下载（`--no-playlist` 写死，v0.3 单条）；
- 不在任何将来上架的构建里包含本功能（剔除清单入 ARCHITECTURE §8）。
