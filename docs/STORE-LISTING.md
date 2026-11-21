# Microsoft Store listing (draft) · 商店页面材料（草稿）

Draft of 2026-11-21 for Partner Center. Nothing here is submitted. Privacy policy text: `docs/PRIVACY-POLICY.md`
(needs a public URL before submission). Build and package rules: `docs/WINDOWS-STORE.md`.

## 1. Names · 名称

| Field | English | 中文 |
|---|---|---|
| Product name (reserve in Partner Center) | **FlowGT Recorder** | **流录** |
| Alternatives if taken | FlowGT Screen Recorder | 流录屏 · FlowGT 录屏 |

The package `DisplayName` comes from `tauri.conf.json` → `productName` ("FlowGT Recorder"). A Chinese display name
in the package needs a `zh-cn` resource string; until then the Chinese name lives only on the Store page.

## 2. Short description · 一句话介绍

- EN: Record your screen, camera and voice into one small MP4 that stays on your PC.
- 中文：把屏幕、摄像头和你的声音录成一个小小的 MP4，只存在你的电脑上。

## 3. Description · 详细介绍

**English**

FlowGT Recorder turns a meeting, an interview or a demo into one MP4: your screen, your camera in a round
picture-in-picture corner, and your microphone. An hour of 1080p is about 400 MB, small enough to keep and to share.

- Press record, get a 3-second countdown, and the file starts exactly when the countdown ends.
- Pause and resume as often as you like; stop gives you one file, saved in well under a second.
- Hardware encoding (NVIDIA, Intel, or Windows' own encoder) keeps the fan quiet during long recordings.
- If the PC crashes mid-recording, the parts already recorded are recovered on the next start.
- Nothing leaves your computer: no account, no telemetry, no ads. If you choose to sign in to Google, finished
  recordings can also go to a folder in your own Google Drive, and nowhere else.

Note: on Windows the app does not yet record the computer's own sound (system audio); it records the
microphone.

**中文**

流录把会议、面试或演示录成一个 MP4：屏幕、圆形小窗里的摄像头画面，加上麦克风的声音。1080p 一小时约 400 MB，
存得下，也方便发给别人。

- 点开始，倒数 3 秒，倒数结束那一刻文件就开始。
- 随时暂停、继续；点停止得到一个完整文件，不到一秒就存好。
- 用显卡硬件编码（NVIDIA、Intel 或 Windows 自带编码器），长时间录制风扇也安静。
- 录到一半电脑死机，下次打开会把已经录下的部分找回来。
- 什么都不离开你的电脑：不用注册账号，没有遥测，没有广告。只有你自己登录 Google 后，录像才可以另存一份到你自己的
  Google Drive 文件夹，不会去任何别的地方。

说明：Windows 上暂不录制电脑声音（系统声音），只录麦克风。

## 4. Feature list · 功能列表（Partner Center 最多 20 条，每条 200 字符内）

| English | 中文 |
|---|---|
| Screen + camera + microphone in one MP4 | 屏幕＋摄像头＋麦克风合成一个 MP4 |
| Round camera picture-in-picture, four corners | 圆形摄像头小窗，四个角可选 |
| Whole screen or a selected area | 整个屏幕或框选区域 |
| About 400 MB per hour at 1080p (HEVC) | 1080p 每小时约 400 MB（HEVC） |
| 1080p or 4K quality switch | 1080p／4K 清晰度切换 |
| Countdown, pause, resume, stop | 倒数、暂停、继续、停止 |
| Hardware encoding on NVIDIA, Intel and other GPUs | NVIDIA、Intel 等显卡硬件编码 |
| Crash recovery of unfinished recordings | 意外中断的录像可以找回 |
| Global shortcuts (Alt+Shift+R / P / S) | 全局快捷键（Alt+Shift+R／P／S） |
| Scrolling screenshots | 滚动截图 |
| Optional upload to your own Google Drive | 可选上传到你自己的 Google Drive |
| No account, no telemetry, no ads | 无需账号，无遥测，无广告 |
| English and Chinese interface | 中英文界面 |

## 5. Keywords · 关键词（最多 7 个）

- EN: screen recorder, webcam, screen capture, meeting recording, HEVC, picture in picture, privacy
- 中文：录屏, 屏幕录制, 摄像头, 会议录制, 画中画, 录像, 隐私

## 6. Screenshots · 截图

4–6 screenshots at 1920×1080 or larger, real app, no personal data. Files live in `tests/out/store-screens/`
(git-ignored) and are uploaded by hand. List and status: report 13.

## 7. `runFullTrust` justification (English, paste into Partner Center)

FlowGT Recorder is a desktop (Win32) screen recorder built with Tauri. It needs full trust because it captures the
display through DXGI Desktop Duplication, opens the camera and microphone through DirectShow, and runs the
FFmpeg encoder (`ffmpeg.exe`, LGPL, shipped inside the package) as a child process to write the MP4 with hardware
video encoding. None of this is available to an app container. The app makes no network connections unless the
user signs in to Google to upload their own recordings to their own Google Drive; it has no telemetry and no
update mechanism outside the Store.

## 8. Age rating questionnaire (IARC), suggested answers

| Question area | Answer |
|---|---|
| Category | Utility / productivity app, not a game |
| Violence, fear, sexuality, nudity, language, drugs, gambling | No |
| Users can interact or exchange content with other users | No (recordings stay local; Drive upload is to the user's own account) |
| Shares the user's location | No |
| Allows purchases (digital goods) | No |
| Unrestricted internet access (browser) | No |
| Collects personal information shared with third parties | No |

Expected result: 3+ / PEGI 3 / Everyone.
