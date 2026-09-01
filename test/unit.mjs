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

const failed = results.filter((r) => !r).length;
console.log(`\n===== 单元测试 ${results.length - failed}/${results.length} 通过 =====`);
process.exit(failed ? 1 : 0);
