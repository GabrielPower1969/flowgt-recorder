// tabCapture 握手最终确认：需要真实用户手势（键盘快捷键 = 真实 invocation）。
// 本脚本等待系统空闲 ≥20 秒才启动（不打扰正在用电脑的人），
// 抢占前台约 15 秒完成一次真实录音，结果写入 test/tabcapture-check.log。
// 用法: node test/manual-tabcapture-check.mjs [--now]  (--now 跳过空闲等待)

import puppeteer from 'puppeteer-core';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import http from 'node:http';
import { fileURLToPath } from 'node:url';

const pexec = promisify(execFile);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..');
const EXT_DIR = path.join(ROOT, 'extension');
const CHROME = path.join(
  ROOT,
  '.browsers/chrome/mac_arm-152.0.7977.64/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing'
);
const LOG = path.join(__dirname, 'tabcapture-check.log');
const log = (m) => {
  const line = `[${new Date().toISOString()}] ${m}`;
  console.log(line);
  fs.appendFileSync(LOG, line + '\n');
};

async function idleSeconds() {
  const { stdout } = await pexec('sh', ['-c',
    `ioreg -c IOHIDSystem | awk '/HIDIdleTime/ {print $NF/1000000000; exit}'`]);
  return parseFloat(stdout.trim());
}

if (!process.argv.includes('--now')) {
  log('等待系统空闲 ≥20s…（按 Ctrl+C 取消；加 --now 立即执行）');
  for (;;) {
    const idle = await idleSeconds();
    if (idle >= 20) break;
    await new Promise((r) => setTimeout(r, 5000));
  }
  log('系统已空闲，开始测试');
}

const toneHtml = fs.readFileSync(path.join(__dirname, 'tone.html'));
const server = http.createServer((_, r) => { r.setHeader('content-type', 'text/html'); r.end(toneHtml); });
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const TONE_URL = `http://127.0.0.1:${server.address().port}/tone.html`;

const WORK = fs.mkdtempSync(path.join(os.tmpdir(), 'flowgt-tc-'));
const DL = path.join(WORK, 'downloads');
fs.mkdirSync(path.join(WORK, 'profile', 'Default'), { recursive: true });
fs.writeFileSync(path.join(WORK, 'profile', 'Default', 'Preferences'),
  JSON.stringify({ download: { default_directory: DL, prompt_for_download: false } }));

let verdict = 'FAIL';
let browser;
try {
  browser = await puppeteer.launch({
    executablePath: CHROME, headless: false,
    args: [
      `--user-data-dir=${path.join(WORK, 'profile')}`,
      `--disable-extensions-except=${EXT_DIR}`, `--load-extension=${EXT_DIR}`,
      '--autoplay-policy=no-user-gesture-required', '--no-first-run', '--window-size=900,600',
    ],
  });
  const swT = await browser.waitForTarget((t) => t.type() === 'service_worker', { timeout: 10000 });
  let sw = null;
  for (let i = 0; i < 20 && !sw; i++) { sw = await swT.worker().catch(() => null); if (!sw) await new Promise((r) => setTimeout(r, 250)); }
  const extId = new URL(swT.url()).host;

  await sw.evaluate(async (url) => (await chrome.tabs.create({ url, active: true })).id, TONE_URL);
  await new Promise((r) => setTimeout(r, 1500));

  // 前台 + 真实快捷键 → 真实 invocation
  await pexec('open', ['-a', CHROME.replace(/\/Contents\/MacOS\/.*$/, '')]);
  await new Promise((r) => setTimeout(r, 1200));
  let popupT = null;
  for (let i = 0; i < 6 && !popupT; i++) {
    await pexec('osascript', ['-e', 'tell application "System Events" to key code 25 using {command down, shift down}']);
    popupT = await browser.waitForTarget((x) => x.url() === `chrome-extension://${extId}/popup.html`, { timeout: 2500 }).catch(() => null);
    if (!popupT) {
      await pexec('osascript', ['-e', 'tell application "System Events" to tell process "Google Chrome for Testing" to set frontmost to true']).catch(() => {});
      await new Promise((r) => setTimeout(r, 600));
    }
  }
  if (!popupT) throw new Error('快捷键唤起失败（6 次重试后）');
  log('真实 invocation 成功（⇧⌘9 popup 已开）');

  // 立即点真实 Start 按钮
  const cdp = await popupT.createCDPSession();
  await cdp.send('Runtime.evaluate', {
    expression: `document.getElementById('label').value='tabcapture-check'; document.getElementById('actionBtn').click();`,
  });
  await new Promise((r) => setTimeout(r, 3000));
  const st = await sw.evaluate(async () => chrome.storage.session.get(['rec', 'lastError']));
  log(`rec=${JSON.stringify(st.rec)} lastError=${JSON.stringify(st.lastError)}`);
  if (!st.rec?.recording) throw new Error(`tabCapture 启动失败: ${st.lastError?.msg || '未知'}`);

  log('tabCapture 录音进行中，录 6 秒…');
  await new Promise((r) => setTimeout(r, 6000));
  await sw.evaluate(() => chrome.runtime.sendMessage({ target: 'offscreen', type: 'stop' }));
  await new Promise((r) => setTimeout(r, 3000));

  const dir = path.join(DL, 'flowgt-recordings');
  const files = fs.existsSync(dir) ? fs.readdirSync(dir).filter((f) => f.endsWith('.webm')) : [];
  if (!files.length) throw new Error('未找到落盘文件');
  const file = path.join(dir, files[0]);
  const { stderr } = await pexec('ffmpeg', ['-i', file, '-af', 'volumedetect', '-f', 'null', '-']);
  const mean = stderr.match(/mean_volume:\s*(-?[\d.]+) dB/)?.[1];
  log(`文件: ${files[0]} mean_volume=${mean}dB`);
  if (+mean > -50) {
    verdict = 'PASS';
    log('✅ tabCapture 生产路径确认可用（真实唤起 → 采集 → 混音 → 落盘 → 有声音）');
  } else {
    verdict = 'SILENT';
    log('⚠️ 文件落盘但音频近乎静音，需人工复核');
  }
} catch (e) {
  log(`❌ ${e.message}`);
} finally {
  if (browser) await browser.close().catch(() => {});
  server.close();
  log(`RESULT: ${verdict}`);
  process.exit(verdict === 'PASS' ? 0 : 1);
}
