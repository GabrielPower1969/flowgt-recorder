# Privacy · 隐私

## English

- **Everything stays on this computer.** Recordings, settings and logs are local files. The app has no
  account, no server and no upload feature.
- **No telemetry, no crash reporter, no update check, no remote fonts.** The Content Security Policy in
  `src-tauri/tauri.conf.json` allows `connect-src ipc: http://ipc.localhost` only, so the webview cannot
  open a network connection. Changing that line is a product decision (CLAUDE.md invariant 12).
- **Permissions.** Screen recording (to capture the display), camera (picture-in-picture), microphone
  (your voice). The camera and microphone prompts come from macOS; screen recording is switched on by you
  in System Settings. Denying a permission disables only that input.
- **Where files live.** Finished recordings: `~/Movies/FlowGT Recorder/` (changeable in Settings).
  In-progress segments and the manifest: `.flowgt-recorder/<session>/` beside them, removed after a
  successful stop. Settings: the app data folder (`settings.json`).
- **Deleting.** Delete the MP4 in Finder. Remove leftover work folders with Settings, Recovery, Discard.
  Uninstalling the app and deleting the settings folder removes everything else.
- Verification status: the CSP line is read from the config file (level A, 2026-11-04). "No network traffic
  at runtime" has not yet been checked with a packet monitor (not yet measured).

## 中文

- **所有内容只留在这台电脑上。** 录像、设置、日志都是本地文件；没有账号、没有服务器、没有上传功能。
- **没有遥测、崩溃上报、更新检查和远程字体。** `src-tauri/tauri.conf.json` 里的内容安全策略只允许
  `connect-src ipc: http://ipc.localhost`，网页层无法联网。改这一行属于产品决定。
- **权限。** 屏幕录制（抓屏幕）、摄像头（画中画）、麦克风（你的声音）。摄像头和麦克风由 macOS 弹窗询问；
  屏幕录制要你在「系统设置」里手动打开。拒绝某项只会让对应输入不可用。
- **文件在哪。** 成品：`~/Movies/FlowGT Recorder/`（可在设置里改）。录制中的分段和清单：旁边的
  `.flowgt-recorder/<session>/`，正常停止后会清理。设置：应用数据目录里的 `settings.json`。
- **怎么删。** 在 Finder 里删 MP4；残留的工作目录用「设置 → 恢复 → 丢弃」；卸载应用并删除设置目录即可清空其余。
- 验证状态：CSP 一行已从配置文件读到（A 级，2026-11-04）；「运行时无任何网络流量」尚未用抓包工具核实（未测）。

## Google Drive（登录后才有） / Google Drive (only after you sign in)

不登录：什么都不离开这台电脑。登录 Google 后：每段录像录完会上传到你自己 Drive 主页的「FlowGT Recorder」文件夹，本地文件保留；App 只申请 `drive.file` 权限，只能看到它自己建的文件，看不到你 Drive 里的其他内容。退出登录：设置 → 通用。

Not signed in: nothing leaves this computer. Signed in to Google: each finished recording is uploaded to the "FlowGT Recorder" folder in the home of your own Drive, and the local file stays. The app asks only for `drive.file`, so it sees only what it created, not the rest of your Drive. Sign out in Settings → General.

