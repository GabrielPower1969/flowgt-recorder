#!/usr/bin/env node
// FlowGT Recorder — Native Messaging host
// Chrome 经 stdin/stdout 与本进程通信：每条消息 = 4 字节小端长度前缀 + UTF-8 JSON。
// ⚠ stdout 是协议通道：任何日志只能写 stderr，否则 Chrome 会断开连接。
//
// 消息（扩展 → host）：
//   {type:'ping'}                                  → {type:'pong', ytdlp, outDir}
//   {type:'download', url, kind, useCookies}       → progress* → done | error | cancelled
//   {type:'cancel'}
// 环境变量（测试注入用）：FLOWGT_YTDLP（yt-dlp 路径）、FLOWGT_OUTDIR（下载目录）

import { spawn, execFile } from 'node:child_process';
import readline from 'node:readline';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';
import { buildArgs, parseProgress, parseFileLine, isCookieError } from './ytdlp-args.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const YTDLP = process.env.FLOWGT_YTDLP || 'yt-dlp';
const OUT_DIR = process.env.FLOWGT_OUTDIR || path.join(os.homedir(), 'Downloads', 'flowgt-downloads');
const PROGRESS_INTERVAL_MS = 500; // 进度节流：≤2 条/秒

// 平台白名单与扩展共用同一份源码（extension/platform.js）
const sandbox = { URL }; // vm 上下文默认没有 URL 全局，缺了会让所有链接都被判非法
vm.createContext(sandbox);
vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'extension', 'platform.js'), 'utf8'), sandbox);
const { detectPlatform } = sandbox.FlowGTPlatform;

const log = (...a) => process.stderr.write(`[flowgt-host] ${a.join(' ')}\n`);

function send(obj) {
  const body = Buffer.from(JSON.stringify(obj), 'utf8');
  const head = Buffer.alloc(4);
  head.writeUInt32LE(body.length, 0);
  process.stdout.write(Buffer.concat([head, body]));
}

// ---------- 下载任务（同一时刻只允许一个） ----------
let job = null; // {child, cancelled}

function killJob() {
  if (!job?.child) return;
  try {
    process.kill(-job.child.pid, 'SIGTERM'); // 整个进程组：yt-dlp 会再拉起 ffmpeg
  } catch {
    job.child.kill('SIGTERM');
  }
}

function startDownload(msg) {
  if (job) return send({ type: 'error', code: 'busy', msg: 'A download is already running' });
  const platform = detectPlatform(msg.url);
  if (!platform) return send({ type: 'error', code: 'url-not-allowed', msg: 'Only YouTube and Bilibili pages are supported' });
  if (msg.kind !== 'video' && msg.kind !== 'audio') {
    return send({ type: 'error', code: 'bad-kind', msg: `Unknown kind: ${msg.kind}` });
  }
  fs.mkdirSync(OUT_DIR, { recursive: true });
  job = { child: null, cancelled: false };
  run({ url: msg.url, kind: msg.kind, platform, useCookies: !!msg.useCookies });
}

function run(opts) {
  const args = buildArgs({ ...opts, outDir: OUT_DIR, nodePath: process.execPath });
  let child;
  try {
    child = spawn(YTDLP, args, { stdio: ['ignore', 'pipe', 'pipe'], detached: true });
  } catch (e) {
    job = null;
    return send({ type: 'error', code: 'ytdlp-missing', msg: e.message });
  }
  job.child = child;

  let file = null;
  let lastSent = 0;
  const errTail = [];
  let settled = false;

  const onLine = (line) => {
    const f = parseFileLine(line);
    if (f) return void (file = f);
    const p = parseProgress(line);
    if (p) {
      const now = Date.now();
      if (now - lastSent >= PROGRESS_INTERVAL_MS) {
        lastSent = now;
        send({ type: 'progress', ...p });
      }
      return;
    }
    if (line.trim()) {
      errTail.push(line);
      if (errTail.length > 30) errTail.shift();
    }
  };
  readline.createInterface({ input: child.stdout }).on('line', onLine);
  readline.createInterface({ input: child.stderr }).on('line', onLine);

  const finish = (code, spawnError) => {
    if (settled) return;
    settled = true;
    const cancelled = job?.cancelled;
    if (spawnError) {
      job = null;
      return send({
        type: 'error',
        code: spawnError.code === 'ENOENT' ? 'ytdlp-missing' : 'spawn-failed',
        msg: spawnError.message,
      });
    }
    if (cancelled) {
      job = null;
      return send({ type: 'cancelled' });
    }
    if (code === 0 && file) {
      job = null;
      return send({ type: 'done', file });
    }
    const errText = errTail.join('\n');
    if (opts.useCookies && isCookieError(errText)) {
      log('cookies unavailable, retrying without cookies');
      send({ type: 'progress', percent: 0, speed: '', eta: '', note: 'retrying-without-cookies' });
      return run({ ...opts, useCookies: false });
    }
    job = null;
    const lastErr = [...errTail].reverse().find((l) => /^ERROR/.test(l)) || errTail.at(-1) || `yt-dlp exited ${code}`;
    send({ type: 'error', code: 'ytdlp-failed', msg: lastErr.replace(/^ERROR:\s*/, '') });
  };
  child.on('error', (e) => finish(null, e));
  child.on('close', (code) => finish(code, null));
}

function ping() {
  execFile(YTDLP, ['--version'], { timeout: 10_000 }, (err, stdout) => {
    send({ type: 'pong', ytdlp: err ? null : stdout.trim(), outDir: OUT_DIR });
  });
}

function handle(msg) {
  switch (msg?.type) {
    case 'ping':
      return ping();
    case 'download':
      return startDownload(msg);
    case 'cancel':
      if (job) {
        job.cancelled = true;
        killJob();
      }
      return;
    default:
      return send({ type: 'error', code: 'bad-message', msg: `Unknown message type: ${msg?.type}` });
  }
}

// ---------- 帧解码 ----------
let buf = Buffer.alloc(0);
process.stdin.on('data', (chunk) => {
  buf = Buffer.concat([buf, chunk]);
  while (buf.length >= 4) {
    const len = buf.readUInt32LE(0);
    if (buf.length < 4 + len) break;
    const raw = buf.subarray(4, 4 + len).toString('utf8');
    buf = buf.subarray(4 + len);
    let msg;
    try {
      msg = JSON.parse(raw);
    } catch {
      send({ type: 'error', code: 'bad-message', msg: 'Invalid JSON' });
      continue;
    }
    handle(msg);
  }
});
// Chrome 断开（扩展重载/浏览器退出）→ 连同正在跑的下载一起收尾
process.stdin.on('end', () => {
  if (job) {
    job.cancelled = true;
    killJob();
  }
  setTimeout(() => process.exit(0), 200);
});
