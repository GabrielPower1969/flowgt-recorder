// FlowGT Recorder — E2E test
// 真实 Chrome for Testing 加载扩展，getDisplayMedia 自动选源拿真实标签页音频，
// 走与生产完全相同的 recorder-core 管线（混音→编码→落盘），ffmpeg 验证产物。
//
// 为什么不用 tabCapture 直测：getMediaStreamId 硬性要求真实用户手势唤起
// （--allowlisted-extension-id 在 Chrome 152 已不足以豁免设备层授权），
// 自动化里无法合成。tabCapture 握手用 test/manual-tabcapture-check.mjs 单独确认。
//
// 用法: node test/e2e.mjs [--headed]

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
// Chrome 137+ 正式版禁用了 --load-extension，必须用 Chrome for Testing
const CHROME = path.join(
  ROOT,
  '.browsers/chrome/mac_arm-152.0.7977.64/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing'
);
const HEADED = process.argv.includes('--headed');

const toneHtml = fs.readFileSync(path.join(__dirname, 'tone.html'));
const server = http.createServer((_, res) => {
  res.setHeader('content-type', 'text/html');
  res.end(toneHtml);
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const TONE_URL = `http://127.0.0.1:${server.address().port}/tone.html`;

const WORK = fs.mkdtempSync(path.join(os.tmpdir(), 'flowgt-e2e-'));
const DOWNLOAD_DIR = path.join(WORK, 'downloads');
const REC_DIR = path.join(DOWNLOAD_DIR, 'flowgt-recordings');
fs.mkdirSync(DOWNLOAD_DIR, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
function check(name, ok, detail = '') {
  results.push({ name, ok, detail });
  console.log(`${ok ? '✅' : '❌'} ${name}${detail ? ` — ${detail}` : ''}`);
}

async function waitForFile(sinceMs, timeoutMs = 15_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (fs.existsSync(REC_DIR)) {
      const hit = fs
        .readdirSync(REC_DIR)
        .filter((f) => f.endsWith('.webm'))
        .map((f) => ({ f, st: fs.statSync(path.join(REC_DIR, f)) }))
        .filter(({ st }) => st.mtimeMs >= sinceMs && st.size > 0)
        .sort((a, b) => b.st.mtimeMs - a.st.mtimeMs)[0];
      if (hit) {
        const p = path.join(REC_DIR, hit.f);
        let last = -1;
        for (;;) {
          const s = fs.statSync(p).size;
          if (s === last) return p;
          last = s;
          await sleep(300);
        }
      }
    }
    await sleep(300);
  }
  return null;
}

async function analyzeAudio(file) {
  // MediaRecorder 的 webm 头里没有时长，解码到 null 取真实时长与音量
  const { stderr } = await pexec('ffmpeg', ['-i', file, '-af', 'volumedetect', '-f', 'null', '-']);
  const t = [...stderr.matchAll(/time=(\d+):(\d+):([\d.]+)/g)].at(-1);
  const duration = t ? +t[1] * 3600 + +t[2] * 60 + +t[3] : 0;
  const mean = stderr.match(/mean_volume:\s*(-?[\d.]+) dB/);
  const codec = (
    await pexec('ffprobe', [
      '-v', 'error', '-select_streams', 'a:0',
      '-show_entries', 'stream=codec_name', '-of', 'csv=p=0', file,
    ])
  ).stdout.trim();
  return { duration, meanVolume: mean ? +mean[1] : -Infinity, codec };
}

let browser;
try {
  // 用 profile Preferences 定下载目录（CDP setDownloadBehavior 会把文件改成 GUID 名）
  const profileDir = path.join(WORK, 'profile');
  fs.mkdirSync(path.join(profileDir, 'Default'), { recursive: true });
  fs.writeFileSync(
    path.join(profileDir, 'Default', 'Preferences'),
    JSON.stringify({
      download: { default_directory: DOWNLOAD_DIR, prompt_for_download: false },
      profile: { exit_type: 'Normal' },
    })
  );
  browser = await puppeteer.launch({
    executablePath: CHROME,
    headless: HEADED ? false : 'new',
    args: [
      `--user-data-dir=${profileDir}`,
      `--disable-extensions-except=${EXT_DIR}`,
      `--load-extension=${EXT_DIR}`,
      '--use-fake-ui-for-media-stream',
      '--use-fake-device-for-media-stream',
      '--autoplay-policy=no-user-gesture-required',
      '--auto-select-tab-capture-source-by-title=FlowGT test tone',
      '--no-first-run',
      '--no-default-browser-check',
    ],
  });

  const swT = await browser.waitForTarget(
    (t) => t.type() === 'service_worker' && t.url().includes('background.js'),
    { timeout: 10_000 }
  );
  let sw = null;
  for (let i = 0; i < 20 && !sw; i++) {
    sw = await swT.worker().catch(() => null);
    if (!sw) await sleep(250);
  }
  if (!sw) throw new Error('拿不到 service worker 句柄');
  const extId = new URL(swT.url()).host;

  // ---- 测试 1：完整录音 8 秒（真实标签页音频 + 假麦克风混音）----
  console.log('— 测试 1：录 8 秒 → 停止 → 落盘 —');
  let tone = await browser.newPage();
  await tone.goto(TONE_URL);
  const harness = await browser.newPage();
  await harness.goto(`chrome-extension://${extId}/test-harness.html?label=autotest_基础`);

  const t1start = Date.now();
  await harness.click('#startBtn');
  await sleep(2500);
  const s1 = await harness.$eval('#out', (el) => el.textContent);
  check('T1 采集启动', s1.startsWith('RECORDING'), s1);
  check('T1 麦克风并入混音', s1.includes('micOk=true'), s1);

  await sleep(8000);
  await harness.click('#stopBtn');
  const file1 = await waitForFile(t1start);
  check('T1 文件落盘到 flowgt-recordings/', !!file1, file1 ? path.basename(file1) : '未找到');
  if (file1) {
    check('T1 文件名含标签', path.basename(file1).includes('autotest_基础'), path.basename(file1));
    const a = await analyzeAudio(file1);
    check('T1 编码为 opus', a.codec === 'opus', a.codec);
    check('T1 时长 ≥6 秒', a.duration >= 6, `${a.duration.toFixed(1)}s`);
    check('T1 音频非静音(混音有内容)', a.meanVolume > -50, `mean_volume=${a.meanVolume}dB`);
  }

  // ---- 测试 2：录音中会议标签页被关闭 → 自动收尾保存 ----
  console.log('— 测试 2：中途关标签页自动保存 —');
  await harness.close(); // 同一页面第二次 getDisplayMedia 会挂住（picker 复用问题），换新页
  await tone.close();
  await sleep(500);
  tone = await browser.newPage();
  await tone.goto(TONE_URL);
  const harness2 = await browser.newPage();
  await harness2.goto(`chrome-extension://${extId}/test-harness.html?label=autotest_abort`);
  const t2start = Date.now();
  await harness2.click('#startBtn');
  await sleep(2000);
  const s2 = await harness2.$eval('#out', (el) => el.textContent);
  check('T2 第二次采集启动', s2.startsWith('RECORDING'), s2);
  await sleep(4000);
  // 模拟会议标签页被关掉：向采集轨道派发 ended（gdm 测试路径下 Chrome 不自动发，
  // 生产 tabCapture 的真实 ended 行为由 manual-tabcapture-check 覆盖）
  await harness2.evaluate(() =>
    window.__tab.getTracks().forEach((t) => t.dispatchEvent(new Event('ended'))));
  await tone.close();
  const stoppedTitle = await harness2
    .waitForFunction(() => document.title === 'harness-stopped', { timeout: 8000 })
    .then(() => true)
    .catch(() => false);
  check('T2 轨道结束触发自动收尾', stoppedTitle);
  const file2 = await waitForFile(t2start);
  check('T2 自动落盘', !!file2, file2 ? path.basename(file2) : '未找到');
  if (file2) {
    const a2 = await analyzeAudio(file2);
    check('T2 中断录音有内容且非静音', a2.duration >= 2 && a2.meanVolume > -50,
      `${a2.duration.toFixed(1)}s, ${a2.meanVolume}dB`);
  }

  // ---- 测试 3：background 防双重启动守卫 ----
  console.log('— 测试 3：双重启动守卫 —');
  await sw.evaluate(async () => {
    await chrome.storage.session.set({
      rec: { recording: true, startTime: Date.now(), tabId: 1, label: 'guard' },
    });
  });
  const guardRes = await harness2.evaluate(() =>
    chrome.runtime.sendMessage({ target: 'background', type: 'start-recording', tabId: 999999, label: 'x' })
  );
  check('T3 录音中再启动被拒绝', !guardRes?.ok && /已有录音/.test(guardRes?.error || ''), guardRes?.error);
  await sw.evaluate(async () => chrome.storage.session.remove('rec'));

  // ---- 测试 4：background 状态机（badge / 状态清理）----
  console.log('— 测试 4：save-recording 信令与状态 —');
  const fakeSave = await harness2.evaluate(async () => {
    // 用一个极小的 blob 走真实 save-recording 通道
    const blob = new Blob([new Uint8Array(1024)], { type: 'audio/webm' });
    const url = URL.createObjectURL(blob);
    return chrome.runtime.sendMessage({
      target: 'background', type: 'save-recording', url, filename: 'signal-test.webm',
    });
  });
  check('T4 save-recording 信令成功', !!fakeSave?.ok, JSON.stringify(fakeSave));
  await sleep(1500);
  const sig = fs.existsSync(path.join(REC_DIR, 'signal-test.webm'));
  check('T4 指定文件名落盘', sig);
} catch (e) {
  check('未捕获异常', false, e.stack?.split('\n')[0] || String(e));
} finally {
  if (browser) await browser.close().catch(() => {});
  server.close();
}

const failed = results.filter((r) => !r.ok);
console.log(`\n===== ${results.length - failed.length}/${results.length} 通过 =====`);
console.log(`产物目录: ${REC_DIR}`);
process.exit(failed.length ? 1 : 0);
