// 界面视觉稿截图：亮/暗 × 待机/录音中/麦克风未授权
import puppeteer from 'puppeteer-core';
import fs from 'node:fs'; import path from 'node:path'; import os from 'node:os';
import { fileURLToPath } from 'node:url';
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..');
const EXT_DIR = path.join(ROOT, 'extension');
const CHROME = path.join(ROOT, '.browsers/chrome/mac_arm-152.0.7977.64/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing');
const OUT = process.argv[2] || path.join(ROOT, 'design-shots');
fs.mkdirSync(OUT, { recursive: true });
const WORK = fs.mkdtempSync(path.join(os.tmpdir(), 'flowgt-ui-'));

const b = await puppeteer.launch({
  executablePath: CHROME, headless: 'new',
  args: [`--user-data-dir=${WORK}`, `--disable-extensions-except=${EXT_DIR}`, `--load-extension=${EXT_DIR}`, '--no-first-run'],
});
const swT = await b.waitForTarget((t) => t.type() === 'service_worker', { timeout: 10000 });
let sw = null;
for (let i = 0; i < 20 && !sw; i++) { sw = await swT.worker().catch(() => null); if (!sw) await new Promise(r => setTimeout(r, 250)); }
const extId = new URL(swT.url()).host;

async function shot(name, { dark = false, recState = null } = {}) {
  if (recState) await sw.evaluate(async (rec) => chrome.storage.session.set({ rec }), recState);
  else await sw.evaluate(async () => chrome.storage.session.remove('rec'));
  const p = await b.newPage();
  await p.emulateMediaFeatures([{ name: 'prefers-color-scheme', value: dark ? 'dark' : 'light' }]);
  await p.setViewport({ width: 320, height: 340, deviceScaleFactor: 2 });
  await p.goto(`chrome-extension://${extId}/popup.html`);
  await new Promise((r) => setTimeout(r, 900));
  await p.screenshot({ path: path.join(OUT, name), fullPage: false });
  await p.close();
}

await shot('popup-idle-light.png');
await shot('popup-idle-dark.png', { dark: true });
await shot('popup-recording-light.png', { recState: { recording: true, startTime: Date.now() - 754000, label: 'Iris_Datacom_R2', micOk: true } });
await shot('popup-recording-dark.png', { dark: true, recState: { recording: true, startTime: Date.now() - 754000, label: 'Iris_Datacom_R2', micOk: true } });

// media download card (demo query param forces platform detection for review shots)
async function dlShot(name, { demo, dark = false, dl = null }) {
  await sw.evaluate(async () => chrome.storage.session.remove('rec'));
  if (dl) await sw.evaluate(async (d) => chrome.storage.session.set({ dl: d }), dl);
  else await sw.evaluate(async () => chrome.storage.session.remove('dl'));
  const p = await b.newPage();
  await p.emulateMediaFeatures([{ name: 'prefers-color-scheme', value: dark ? 'dark' : 'light' }]);
  await p.setViewport({ width: 320, height: 300, deviceScaleFactor: 2 });
  await p.goto(`chrome-extension://${extId}/popup.html?demo=${demo}`);
  await new Promise((r) => setTimeout(r, 900));
  await p.screenshot({ path: path.join(OUT, name), fullPage: true });
  await p.close();
}
const busy = { active: true, platform: 'youtube', kind: 'video', title: 'Me at the zoo', percent: 63, speed: '4.97MiB/s', eta: '00:12' };
await dlShot('download-youtube-light.png', { demo: 'youtube' });
await dlShot('download-youtube-dark.png', { demo: 'youtube', dark: true });
await dlShot('download-bilibili-light.png', { demo: 'bilibili' });
await dlShot('download-bilibili-dark.png', { demo: 'bilibili', dark: true });
await dlShot('download-progress-light.png', { demo: 'youtube', dl: busy });
await dlShot('download-done-light.png', { demo: 'youtube', dl: { ...busy, active: false, done: true, percent: 100 } });
await dlShot('download-error-light.png', { demo: 'youtube', dl: { platform: 'youtube', error: 'host-missing' } });
await sw.evaluate(async () => chrome.storage.session.remove('dl'));

// permission page
const pp = await b.newPage();
await pp.setViewport({ width: 720, height: 480, deviceScaleFactor: 2 });
await pp.goto(`chrome-extension://${extId}/permission.html`);
await new Promise((r) => setTimeout(r, 700));
await pp.screenshot({ path: path.join(OUT, 'permission-page.png') });

await b.close();
console.log('shots →', OUT);
