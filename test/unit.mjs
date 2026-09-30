// FlowGT Recorder — 单元测试（Node 直跑，无浏览器）
// 覆盖 recorder-core 的纯逻辑：文件名构造、格式选择的回退行为。
// 用法: node test/unit.mjs

import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const coreSrc = fs.readFileSync(
  path.join(__dirname, '..', 'extension', 'recorder-core.js'),
  'utf8'
);

// recorder-core 是 IIFE，定义期不触碰浏览器 API —— 可在裸 Node 上下文里求值。
// MediaRecorder 故意不注入：pickFormat 应回退到 webm/opus。
const sandbox = { console };
vm.createContext(sandbox);
vm.runInContext(coreSrc + '\nthis.FlowGTRecorder = FlowGTRecorder;', sandbox);
const core = sandbox.FlowGTRecorder;

const results = [];
function check(name, actual, expected) {
  const ok = actual === expected;
  results.push(ok);
  console.log(`${ok ? '✅' : '❌'} ${name}${ok ? '' : ` — got ${JSON.stringify(actual)}, want ${JSON.stringify(expected)}`}`);
}

const d = new Date(2026, 8, 2, 9, 5); // 2026-09-02 09:05

// —— buildFilename ——
check('基础：标签+麦克风+mp4',
  core.buildFilename('Iris_Datacom_R2', true, d, 'mp4'),
  '2026-09-02_0905_Iris_Datacom_R2.mp4');

check('无标签 → meeting 兜底',
  core.buildFilename('', true, d, 'mp4'),
  '2026-09-02_0905_meeting.mp4');

check('null 标签不崩溃',
  core.buildFilename(null, true, d, 'webm'),
  '2026-09-02_0905_meeting.webm');

check('无麦克风 → _nomic 标记',
  core.buildFilename('test1', false, d, 'mp4'),
  '2026-09-02_0905_test1_nomic.mp4');

check('非法字符全部剔除（Windows 兼容）',
  core.buildFilename('a\\b/c:d*e?f"g<h>i|j', true, d, 'mp4'),
  '2026-09-02_0905_abcdefghij.mp4');

check('空白折叠为下划线',
  core.buildFilename('  Iris   round 2  ', true, d, 'mp4'),
  '2026-09-02_0905_Iris_round_2.mp4');

check('中文等 Unicode 保留',
  core.buildFilename('张三_二面', true, d, 'mp4'),
  '2026-09-02_0905_张三_二面.mp4');

check('超长标签截断到 60 字符',
  core.buildFilename('x'.repeat(100), true, d, 'mp4'),
  `2026-09-02_0905_${'x'.repeat(60)}.mp4`);

check('未传扩展名 → webm 兜底',
  core.buildFilename('a', true, d, undefined),
  '2026-09-02_0905_a.webm');

// —— pickFormat ——
check('无 MediaRecorder 环境 → 回退 webm/opus',
  core.pickFormat().ext, 'webm');

sandbox.MediaRecorder = { isTypeSupported: (m) => m.startsWith('audio/mp4') };
check('支持 audio/mp4 → 选 mp4', core.pickFormat().ext, 'mp4');
check('mp4 的 mime 是 AAC', core.pickFormat().mime, 'audio/mp4;codecs=mp4a.40.2');

sandbox.MediaRecorder = { isTypeSupported: (m) => m.includes('webm') };
check('只支持 webm → 选 webm', core.pickFormat().ext, 'webm');

// —— platform.js（与 host 同源的白名单）——
const psb = { URL };
vm.createContext(psb);
vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'extension', 'platform.js'), 'utf8'), psb);
const { detectPlatform, isMediaPage } = psb.FlowGTPlatform;

const PLATFORM_CASES = [
  ['https://www.youtube.com/watch?v=jNQXAC9IVRw', 'youtube', true],
  ['https://youtube.com/watch?v=jNQXAC9IVRw&t=30s', 'youtube', true],
  ['https://m.youtube.com/watch?v=jNQXAC9IVRw', 'youtube', true],
  ['https://music.youtube.com/watch?v=jNQXAC9IVRw', 'youtube', true],
  ['https://www.youtube.com/shorts/abcdEFGH123', 'youtube', true],
  ['https://www.youtube.com/live/abcdEFGH123', 'youtube', true],
  ['https://youtu.be/jNQXAC9IVRw', 'youtube', true],
  ['https://www.youtube.com/', 'youtube', false],
  ['https://www.youtube.com/@channel', 'youtube', false],
  ['https://www.youtube.com/watch', 'youtube', false],
  ['https://www.bilibili.com/video/BV1xx411c7mD', 'bilibili', true],
  ['https://www.bilibili.com/video/BV1xx411c7mD/?p=2&spm_id_from=333', 'bilibili', true],
  ['https://m.bilibili.com/video/BV1xx411c7mD', 'bilibili', true],
  ['https://www.bilibili.com/video/av170001', 'bilibili', true],
  ['https://www.bilibili.com/bangumi/play/ep12345', 'bilibili', true],
  ['https://b23.tv/AbCd12', 'bilibili', true],
  ['https://www.bilibili.com/', 'bilibili', false],
  ['https://space.bilibili.com/12345', null, false],
  ['https://youtube.com.evil.com/watch?v=jNQXAC9IVRw', null, false],
  ['https://notyoutube.com/watch?v=jNQXAC9IVRw', null, false],
  ['https://evil.com/?u=https://www.youtube.com/watch?v=x', null, false],
  ['chrome://extensions', null, false],
  ['file:///Users/x/video.mp4', null, false],
  ['not a url', null, false],
  ['', null, false],
];
for (const [url, platform, media] of PLATFORM_CASES) {
  check(`平台识别 ${url || '(空)'} → ${platform}`, detectPlatform(url), platform);
  check(`视频页判断 ${url || '(空)'} → ${media}`, isMediaPage(url), media);
}

// —— host 纯逻辑：yt-dlp 参数与输出解析 ——
const { buildArgs, parseProgress, parseFileLine, isCookieError } = await import('../host/ytdlp-args.mjs');
const { unpackedExtensionId } = await import('../host/ext-id.mjs');

const va = buildArgs({ url: 'https://youtu.be/x', kind: 'video', platform: 'youtube', useCookies: true, outDir: '/o', nodePath: '/n' });
check('参数：视频格式串', va[va.indexOf('-f') + 1], 'bv*+ba[ext=m4a]/bv*+ba/b');
check('参数：YouTube 忽略 cookies 请求', va.includes('--cookies-from-browser'), false);
check('参数：YouTube JS 运行时', va[va.indexOf('--js-runtimes') + 1], 'node:/n');
check('参数：URL 在 -- 之后且是最后一个', `${va.at(-2)} ${va.at(-1)}`, '-- https://youtu.be/x');
check('参数：输出模板在 outDir 下', va[va.indexOf('-o') + 1], '/o/%(title).80s [%(id)s].%(ext)s');
const aa = buildArgs({ url: 'https://b23.tv/x', kind: 'audio', platform: 'bilibili', useCookies: true, outDir: '/o' });
check('参数：音频转 m4a', aa[aa.indexOf('--audio-format') + 1], 'm4a');
check('参数：B 站 cookies 来自 Chrome', aa[aa.indexOf('--cookies-from-browser') + 1], 'chrome');
let threw = false;
try { buildArgs({ url: 'x', kind: 'exe', platform: 'youtube', outDir: '/o' }); } catch { threw = true; }
check('参数：非法 kind 抛错', threw, true);

check('进度解析：常规行', JSON.stringify(parseProgress('FLOWGT_PROGRESS  42.0%|   3.51MiB/s|00:07')),
  JSON.stringify({ percent: 42, speed: '3.51MiB/s', eta: '00:07' }));
check('进度解析：NA 字段置空', JSON.stringify(parseProgress('FLOWGT_PROGRESS 100.0%|1.92MiB/s|NA')),
  JSON.stringify({ percent: 100, speed: '1.92MiB/s', eta: '' }));
check('进度解析：非进度行返回 null', parseProgress('[youtube] Extracting URL'), null);
check('进度解析：坏百分比返回 null', parseProgress('FLOWGT_PROGRESS N/A|x|y'), null);
check('路径行解析', parseFileLine('FLOWGT_FILE:/a/b [id].mp4'), '/a/b [id].mp4');
check('路径行：非路径行 null', parseFileLine('FLOWGT_PROGRESS 1%|a|b'), null);
check('cookies 错误识别', isCookieError('ERROR: Failed to decrypt with DPAPI'), true);
check('普通错误不当成 cookies 错误', isCookieError('ERROR: Video unavailable'), false);

const eid = unpackedExtensionId('/Users/x/flowgt-recorder/extension');
check('扩展 ID：32 位 a-p', /^[a-p]{32}$/.test(eid), true);
check('扩展 ID：同路径稳定', unpackedExtensionId('/Users/x/flowgt-recorder/extension'), eid);

const failed = results.filter((r) => !r).length;
console.log(`\n===== 单元测试 ${results.length - failed}/${results.length} 通过 =====`);
process.exit(failed ? 1 : 0);
