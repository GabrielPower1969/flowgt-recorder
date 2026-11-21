# 13 · Windows：NVIDIA 显卡实测，和微软商店上架准备

- 日期：2026-11-21 至 22，无人值守。作者：Claude（Windows 负责人）。分支 `win/nvidia-test-store-prep`（基于 main `29a5e01`），草稿 PR。
- 证据在 `tests/out/nvidia/`（git 忽略），下文方括号里是这个目录下的文件名。
- 测试机：Intel Core i7-9700，32 GB 内存，Windows 11 Pro 26200。**NVIDIA GeForce GTX 1660 Ti，驱动 610.60**；Intel UHD 630 核显也开着（双显卡）。屏幕 Odyssey G8，3840×2160，接在 NVIDIA 上。摄像头 2K USB Camera（实际以 640×480 MJPEG 打开），麦克风 Fifine Microphone。

## 1. 结论

| 项目 | 结果 |
|---|---|
| **NVIDIA 上能不能录** | **修之前完全录不了**：点开始后几秒就报“摄像头断开”，然后整段失败，文件 0 字节 `[flows 之前的 probe]`。原因和修法见 3.1。修好后可以正常录 |
| 应用选的编码器 | `hevc_nvenc`（双显卡机器上也是它，NVENC 在选择顺序里排第一）`[manifest 里的 plan]` |
| 画面是否留在显卡上 | **不是**。现在的路径是：抓屏（显卡上）→ `hwdownload` 拷回内存 → CPU 缩放到 1080p → NVENC。4K 屏只录屏时 ffmpeg 占 **约 1 个核（99–105%）**；把 4K 画面直接交给 NVENC 只占 3%。想在显卡上缩放（`scale_d3d11`、映射到 CUDA）在这个 ffmpeg 版本里都失败（“Could not create the texture 80070057”），见 5.1 |
| 录制时 CPU（摄像头＋麦克风） | ffmpeg 占 **232% 个核（8 核的 29%）**，应用本身约 0% `[实测 20 秒采样]` |
| 每小时大小 | 摄像头＋麦克风真录 5 分钟 26.89 MB，即 **约 322 MB/h**（预算 400）`[ss2-5min.txt]` |
| 倒数结束 → 进入录制（中位数） | 摄像头＋麦克风 **−10 ms**，只麦克风 −14 ms，只录屏 −14 ms（Intel：−10／−11／−6） |
| 点停止 → 文件存好（中位数） | 摄像头＋麦克风 **313 ms**，只麦克风 261 ms，只录屏 261 ms（Intel：250 ms）；5 分钟录像 515 ms（Intel 288 ms） |
| 开头声音空洞 | 保守路（编码前剪帧）**0.196 秒** → 加进热编码器路径后 **0.035 秒**（Intel 0.021）`[flows-before.json → flows-after.json]` |
| 第一个视频包 | 关键帧（K），解码 0 错误 |
| 倒数中取消 | 修之前：工作文件夹留下一个空的 `seg-000.mp4`；修之后干净 |
| 暂停过／不倒数的录像，画面比声音长 | 1.12 秒／1.08 秒（老问题，这次没修，见 4.3） |
| 上架版（MSIX）＋ `hevc_mf` 在包里崩不崩 | **未验证**：要 WSL 编 ffmpeg、要开发者模式装包，两样都要你来做（见 4.1）。包外面 `hevc_mf`／`h264_mf` 在 NVIDIA 上能用，系统选的是“NVIDIA HEVC Encoder MFT” |

## 2. 怎么测的

工具都装在 D 盘的一个工具文件夹里（git、node 26、pwsh 7、gh、rust 1.100）。**Visual Studio Build Tools 没有用安装程序**（要管理员权限，会弹 UAC），而是从微软官方 VS 渠道清单下载同样的包，解开放在同一个工具文件夹里：MSVC 14.44 加 Windows SDK 10.0.26100（含 makeappx、WACK）。编译器完全一样，不影响性能。用户环境变量里加了这些路径，没有改任何系统设置。

```
npm install; pwsh scripts/fetch-ffmpeg.ps1
npm run check; npm test; cargo test（普通、--features msix）; cargo clippy（两种）; scripts/integration.ps1; scripts/bench.ps1
npm run tauri build
powershell -File scripts/bench-app-win.ps1
node scripts/bench-start-stop.mjs cam-mic 5 8 '{...摄像头+麦克风}'    # 再 mic 3 次、screen 3 次、5 分钟 1 次
node scripts/bench-flows.mjs src-tauri/binaries/ffprobe-x86_64-pc-windows-msvc.exe
```

找“录不了”的原因：用 core 里的 `recording_command_gated` 把应用实际的 ffmpeg 参数打出来，直接跑 ffmpeg，一项一项去掉参数，看是哪个参数引起的（一次性脚本 `try-argv.ps1`）。

## 3. 改了什么

### 3.1 NVIDIA 上录不了（`encoders.rs`）

Windows 录屏命令里有一个全局的 D3D11 设备（`-init_hw_device d3d11va`，抓屏要用）。有了它，NVENC 就以 D3D11 模式打开，而这个模式不接受 `yuv420p` 输入：报 `GetSequenceParams failed: invalid param`，编码器打不开。去掉其中任何一样都能录：

| 试法 | 结果 | ffmpeg CPU |
|---|---|---|
| 原样（`-pix_fmt yuv420p`） | 打不开 | — |
| 去掉 `-init_hw_device` | 能录 | 101% |
| 改成 `-pix_fmt nv12` | 能录 | **105%** |
| 改成 `-pix_fmt bgra` | 能录 | 158% |

采用的改法：NVENC 和 Media Foundation 一样，输入用 NV12（`encoders::input_pix_fmt`）。文件里还是 8 位 4:2:0，Mac 不受影响（Mac 没有 NVENC）。编码器自检（`self_test_command`）不带 D3D11 设备，所以自检一直能过，只有真录的时候才失败，这也是为什么之前没发现。

### 3.2 NVENC 进“热编码器剪包”路径（`command.rs`）

`gate_cuts_after_encoding` 加上 `_nvenc`。编码器参数：`-bf 0 -delay 0 -zerolatency 1 -forced-idr 1`，意思是不用 B 帧、不压帧、在闸门处强制 IDR 关键帧。单元测试断言了这些参数、剪包的 bitstream 过滤器、强制关键帧的表达式；原来测“保守路”的那段改用 `hevc_amf`。

| | 修前（保守路） | 修后（热编码器） |
|---|---|---|
| 前 3 个声音包时间 | 0.000、**0.196**、0.217 | 0.000、0.035、0.056 |
| 声音最大空洞 | 0.196 s | **0.035 s** |
| 倒数结束 → 录制 | −10 ms | −10 ms（不变） |

### 3.3 取消倒数留下文件夹（`recorder.rs`）

NVENC 关闭得比 QSV 慢，取消时 ffmpeg 还占着 `seg-000.mp4`，删文件夹失败，文件夹就留下了。新增 `remove_work_dir`：删不掉就每 100 ms 重试，最多 2 秒。用在三个不持锁的取消路径上。修后 `workFoldersBefore 3 → After 3`，ffmpeg 0 个。

### 3.4 其他

- `scripts/build-store-win.ps1`：先看 `WindowsSdkDir`／`VCToolsInstallDir` 环境变量，再找默认安装位置，这样 SDK 不装在 C 盘也能用。
- `scripts/bench.ps1`：给 `_mf` 编码器 NV12 输入，和应用一致。之前在 NVIDIA 上 `_mf` 全部“format negotiation failed”，其实是脚本的问题。
- `src-tauri/msix/AppxManifest.xml`：包描述去掉了“computer sound”（Windows 上不录电脑声音）。
- 新增 `docs/STORE-LISTING.md`（商店页面中英文材料、runFullTrust 申请理由、年龄分级建议答案）和 `docs/PRIVACY-POLICY.md`（隐私政策草稿，中英文）。

## 4. 没做到的

1. **上架版没测（一·7）**。`build-ffmpeg-win.sh` 要 WSL，本机没装（`wsl --status` 报 `REGDB_E_CLASSNOTREG`）；装 WSL 要管理员权限和重启。装包还要开发者模式（现在是关的）。所以“`hevc_mf` 在包里配 NVIDIA 驱动崩不崩”还不知道。另外这台机器有 Intel 核显：自编 ffmpeg 带 QSV，上架版在这台机器上会先选 `hevc_qsv`，要测 NVIDIA 的 MF 得在 BIOS 里关核显，或者直接在包目录里跑 `ffmpeg.exe -c:v hevc_mf`。
2. **推送和草稿 PR**：这台机器上 `gh` 没登录，`git push` 也没有凭据。提交已在本地分支上。
3. **暂停过／不倒数的录像，画面比声音长约 1.1 秒（一·6）**：量了，没修。报告 12 试过“共用一个钟”，会在开头补重复帧，需要另想办法。
4. **商店截图**：5 张在 `tests/out/store-screens/`（3009×1938，英文 3 张、中文 2 张）。为了不露个人信息，截图时临时关了摄像头、把保存位置改成 D 盘上一个不带用户名的文件夹、屏幕上放了一张演示幻灯片，截完已经改回原设置。**没有摄像头画中画的截图**：要有人出镜，需要你来拍。
5. `cargo fmt --check` 在 main 上就不通过（整个代码库都没按 rustfmt 默认宽度排版），这次没动。
6. Windows App Certification Kit 已和 SDK 一起解压到工具文件夹，没运行（要先装包、要管理员权限）。

## 5. 还能优化的地方

1. **在显卡上缩放（收益最大，约省 1 个核）**。4K 屏现在每帧拷回内存再缩放。`scale_d3d11` 在 BtbN 9.0 版里建纹理就失败；可以试别的 ffmpeg 版本，或者给 ddagrab 加缩放。只录屏时，直接 4K→NVENC 只占 3%。
2. **摄像头＋麦克风比只录屏多用约 1.3 个核**。摄像头本身只有 640×480，所以多出来的应该在滤镜里（画中画遮罩、1080p overlay、音频的 speechnorm）。还没拆开测。
3. 5 分钟录像停止要 515 ms（Intel 288 ms），没查原因。

## 6. 给你的步骤

**测试收尾：**

1. `gh auth login` 登录 GitHub，然后我推 `win/nvidia-test-store-prep`、开草稿 PR。
2. 管理员终端里装 WSL：`wsl --install --no-distribution`，然后重启。我把 Ubuntu 装到 D 盘，第一次打开设用户名和密码要你来。之后我编 ffmpeg、打上架包。
3. 打开开发者模式（设置 → 系统 → 开发者选项），我再 `Add-AppxPackage -Register` 装上，测只录屏的开始／停止和 `hevc_mf`。

**上架：**

1. 注册合作伙伴中心个人账号（免费），预留名字“FlowGT Recorder”和中文名“流录”。
2. 把 *Product identity* 里的 `Package/Identity/Name`、`Publisher`、`PublisherDisplayName` 发给我。
3. 定隐私政策挂在哪个公开网址，补上联系方式（`docs/PRIVACY-POLICY.md` 第 8 条）。

拿到身份信息后，我把 `tauri.conf.json` 的版本改成 1.0.0，用真身份跑 `build-store-win.ps1`，再跑 WACK（要管理员的那一步叫你）。

## 7. 检查

```
npm run check                 212 files, 0 errors, 0 warnings
npm test                      160 passed
cargo test --workspace        51 + 149 passed，普通版和 --features msix 各一遍
cargo clippy -D warnings      两种构建都干净
scripts/integration.ps1       PASS（encoder=hevc_nvenc, 957 kbps）
scripts/bench.ps1             FAIL：只有 h264_qsv 571 MB/h、h264_mf 497 MB/h 超预算（链里靠后的兜底编码器）；hevc_nvenc 416、hevc_mf 385
```

原始输出：`final/*.txt`、`bench.txt`、`bench-after.txt`、`ss-*.txt`（修前）、`ss2-*.txt`（修后）、`flows-before.json`、`flows-after.json`、`gpu-path.txt`。
