#!/usr/bin/env node
// 假 yt-dlp：模拟真实 yt-dlp 的输出协议（进度模板行 + after_move 路径行），供 host 集成测试用。
// 行为由环境变量控制：
//   FLOWGT_STUB_MODE = ok | fail | cookiefail | slow
//   FLOWGT_STUB_LOG  = 每次调用把 argv 追加写入该文件（JSON 行）
import fs from 'node:fs';
import path from 'node:path';

const argv = process.argv.slice(2);
if (argv[0] === '--version') {
  console.log('stub-1.0');
  process.exit(0);
}
if (process.env.FLOWGT_STUB_LOG) fs.appendFileSync(process.env.FLOWGT_STUB_LOG, JSON.stringify(argv) + '\n');

const mode = process.env.FLOWGT_STUB_MODE || 'ok';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const progress = (p) => console.log(`FLOWGT_PROGRESS ${p.toFixed(1)}%|  1.00MiB/s|00:0${Math.max(0, 9 - Math.floor(p / 12))}`);

if (mode === 'fail') {
  console.error('WARNING: something noisy');
  console.error('ERROR: [stub] boom: video unavailable');
  process.exit(1);
}
if (mode === 'cookiefail' && argv.includes('--cookies-from-browser')) {
  console.error('ERROR: Failed to decrypt with DPAPI / keyring');
  process.exit(1);
}

if (mode === 'slow') {
  process.on('SIGTERM', () => process.exit(143));
  for (let i = 0; i < 200; i++) {
    progress(i / 2);
    await sleep(100);
  }
}

// ok：短时间内狂刷 50 行进度（检验 host 节流）
for (let i = 0; i <= 50; i++) progress(i * 2);
const tpl = argv[argv.indexOf('-o') + 1];
const ext = argv.includes('-x') ? 'm4a' : 'mp4';
const file = tpl.replace('%(title).80s', 'Stub Title').replace('%(id)s', 'stubid').replace('%(ext)s', ext);
fs.mkdirSync(path.dirname(file), { recursive: true });
fs.writeFileSync(file, 'stub media');
console.log(`FLOWGT_FILE:${file}`);
process.exit(0);
