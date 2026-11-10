// FlowGT Recorder — Native Messaging host 集成测试
// 真 spawn host 进程、走真实 4 字节长度前缀帧协议，yt-dlp 用 test/fixtures/yt-dlp-stub.mjs 替身。
// 不碰网络、不碰 Chrome。用法: node test/host.test.mjs

import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const HOST = path.join(__dirname, '..', 'host', 'flowgt-host.mjs');
const STUB = path.join(__dirname, 'fixtures', 'yt-dlp-stub.mjs');
fs.chmodSync(STUB, 0o755);

const results = [];
function check(name, ok, detail = '') {
  results.push(ok);
  console.log(`${ok ? '✅' : '❌'} ${name}${detail ? ` — ${detail}` : ''}`);
}

function frame(obj) {
  const body = Buffer.from(JSON.stringify(obj), 'utf8');
  const head = Buffer.alloc(4);
  head.writeUInt32LE(body.length, 0);
  return Buffer.concat([head, body]);
}

// 启动一个 host，返回 {send, next(type), all, close, exited}
function startHost(env = {}) {
  const work = fs.mkdtempSync(path.join(os.tmpdir(), 'flowgt-host-'));
  const outDir = path.join(work, 'out');
  const stubLog = path.join(work, 'argv.jsonl');
  const proc = spawn(process.execPath, [HOST], {
    env: { ...process.env, FLOWGT_YTDLP: STUB, FLOWGT_OUTDIR: outDir, FLOWGT_STUB_LOG: stubLog, ...env },
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  const all = [];
  const waiters = [];
  let buf = Buffer.alloc(0);
  proc.stdout.on('data', (c) => {
    buf = Buffer.concat([buf, c]);
    while (buf.length >= 4) {
      const len = buf.readUInt32LE(0);
      if (buf.length < 4 + len) break;
      const msg = JSON.parse(buf.subarray(4, 4 + len).toString('utf8'));
      buf = buf.subarray(4 + len);
      all.push(msg);
      waiters.splice(0).forEach((w) => w());
    }
  });
  const exited = new Promise((r) => proc.on('exit', (code) => r(code)));
  async function next(types, timeoutMs = 8000) {
    const want = [].concat(types);
    const deadline = Date.now() + timeoutMs;
    let seen = 0;
    for (;;) {
      for (; seen < all.length; seen++) if (want.includes(all[seen].type)) return all.splice(0, seen + 1).at(-1);
      if (Date.now() > deadline) throw new Error(`timeout waiting for ${want} (got ${JSON.stringify(all)})`);
      await new Promise((r) => { waiters.push(r); setTimeout(r, 100); });
    }
  }
  const argvLog = () =>
    fs.existsSync(stubLog) ? fs.readFileSync(stubLog, 'utf8').trim().split('\n').map((l) => JSON.parse(l)) : [];
  return {
    send: (o) => proc.stdin.write(frame(o)),
    raw: (b) => proc.stdin.write(b),
    next, all, outDir, argvLog, exited,
    close: () => proc.stdin.end(),
    kill: () => proc.kill('SIGKILL'),
  };
}

const YT = 'https://www.youtube.com/watch?v=jNQXAC9IVRw';
const BILI = 'https://www.bilibili.com/video/BV1xx411c7mD';

try {
  // H1 ping → pong 带 yt-dlp 版本
  {
    const h = startHost();
    h.send({ type: 'ping' });
    const m = await h.next('pong');
    check('H1 ping → pong 报 yt-dlp 版本', m.ytdlp === 'stub-1.0', JSON.stringify(m));
    check('H1 pong 报下载目录', m.outDir === h.outDir, m.outDir);
    h.close();
    check('H1 stdin 关闭后 host 退出', (await h.exited) === 0);
  }

  // H2 YouTube 视频：进度节流 + done + 参数
  {
    const h = startHost();
    h.send({ type: 'download', url: YT, kind: 'video', useCookies: true });
    const done = await h.next(['done', 'error']);
    check('H2 YouTube 视频下载完成', done.type === 'done', JSON.stringify(done));
    check('H2 产物落在下载目录', done.file?.startsWith(h.outDir) && fs.existsSync(done.file), done.file);
    check('H2 视频扩展名 mp4', done.file?.endsWith('.mp4'));
    const a = h.argvLog()[0] || [];
    check('H2 最高画质 + AAC 音轨优先', a.includes('bv*+ba[ext=m4a]/bv*+ba/b'), a.join(' '));
    check('H2 合流为 mp4', a[a.indexOf('--merge-output-format') + 1] === 'mp4');
    check('H2 单条不下播放列表', a.includes('--no-playlist'));
    check('H2 YouTube 永不带 cookies（即使请求带）', !a.includes('--cookies-from-browser'));
    check('H2 YouTube 配 node JS 运行时', a.includes('--js-runtimes') && a[a.indexOf('--js-runtimes') + 1] === `node:${process.execPath}`);
    check('H2 URL 位于 "--" 之后（防参数注入）', a.at(-2) === '--' && a.at(-1) === YT);
    check('H2 Windows 兼容文件名 + 不改 mtime', a.includes('--windows-filenames') && a.includes('--no-mtime'));
    h.close();
  }

  // H3 进度节流：stub 瞬间刷 51 行进度，host 只能转发 ≤2 条
  {
    const h = startHost();
    const seen = [];
    h.send({ type: 'download', url: YT, kind: 'audio' });
    for (;;) {
      const m = await h.next(['progress', 'done', 'error']);
      seen.push(m);
      if (m.type !== 'progress') break;
    }
    const n = seen.filter((m) => m.type === 'progress').length;
    check('H3 进度节流（51 行 → ≤2 条）', n >= 1 && n <= 2, `转发 ${n} 条`);
    check('H3 进度字段完整', seen[0].type === 'progress' && typeof seen[0].percent === 'number' && seen[0].speed.includes('MiB/s'), JSON.stringify(seen[0]));
    h.close();
  }

  // H4 B 站音频 + cookies
  {
    const h = startHost();
    h.send({ type: 'download', url: BILI, kind: 'audio', useCookies: true });
    const done = await h.next(['done', 'error']);
    check('H4 B 站音频完成且为 m4a', done.type === 'done' && done.file.endsWith('.m4a'), done.file);
    const a = h.argvLog()[0] || [];
    check('H4 音频提取为 m4a 最高质量', a.includes('-x') && a[a.indexOf('--audio-format') + 1] === 'm4a' && a[a.indexOf('--audio-quality') + 1] === '0');
    check('H4 B 站带 Chrome cookies', a[a.indexOf('--cookies-from-browser') + 1] === 'chrome');
    check('H4 B 站不加 JS 运行时', !a.includes('--js-runtimes'));
    h.close();
  }

  // H5 B 站 useCookies=false → 不带 cookies
  {
    const h = startHost();
    h.send({ type: 'download', url: BILI, kind: 'video', useCookies: false });
    await h.next(['done', 'error']);
    check('H5 关闭开关则 B 站不带 cookies', !(h.argvLog()[0] || []).includes('--cookies-from-browser'));
    h.close();
  }

  // H6 白名单：非平台域 / 仿冒域 / 非 http 协议全部拒绝，且根本不启动 yt-dlp
  {
    const h = startHost();
    const bad = [
      'https://evil.example.com/watch?v=x',
      'https://youtube.com.evil.com/watch?v=x',
      'https://notbilibili.com/video/BV1xx411c7mD',
      'file:///etc/passwd',
      '--exec=rm -rf ~',
    ];
    const codes = [];
    for (const url of bad) {
      h.send({ type: 'download', url, kind: 'video' });
      codes.push((await h.next('error')).code);
    }
    check('H6 白名单外 URL 全部拒绝', codes.every((c) => c === 'url-not-allowed'), codes.join(','));
    check('H6 被拒时从未调用 yt-dlp', h.argvLog().length === 0);
    h.send({ type: 'download', url: YT, kind: 'exe' });
    check('H6 非法 kind 拒绝', (await h.next('error')).code === 'bad-kind');
    h.close();
  }

  // H7 yt-dlp 失败 → error 带真实原因（取 ERROR 行，不是 WARNING）
  {
    const h = startHost({ FLOWGT_STUB_MODE: 'fail' });
    h.send({ type: 'download', url: YT, kind: 'video' });
    const m = await h.next(['done', 'error']);
    check('H7 失败回报 ytdlp-failed', m.type === 'error' && m.code === 'ytdlp-failed', JSON.stringify(m));
    check('H7 错误信息取 ERROR 行', /boom: video unavailable/.test(m.msg) && !/^ERROR/.test(m.msg), m.msg);
    // 失败后 job 已释放，可再次下载
    h.send({ type: 'download', url: YT, kind: 'video' });
    check('H7 失败后可再次发起', (await h.next(['done', 'error'])).code === 'ytdlp-failed');
    h.close();
  }

  // H8 cookies 失败 → 自动去掉 cookies 重试一次并成功
  {
    const h = startHost({ FLOWGT_STUB_MODE: 'cookiefail' });
    h.send({ type: 'download', url: BILI, kind: 'video', useCookies: true });
    const m = await h.next(['done', 'error']);
    const runs = h.argvLog();
    check('H8 cookies 失败后自动重试成功', m.type === 'done', JSON.stringify(m));
    check('H8 共调用 2 次，第 2 次无 cookies', runs.length === 2 && runs[0].includes('--cookies-from-browser') && !runs[1].includes('--cookies-from-browser'));
    h.close();
  }

  // H9 下载中：并发拒绝 + 取消 + 取消后可再下载
  {
    const h = startHost({ FLOWGT_STUB_MODE: 'slow' });
    h.send({ type: 'download', url: YT, kind: 'video' });
    await h.next('progress');
    h.send({ type: 'download', url: YT, kind: 'audio' });
    check('H9 下载中再发起被拒（busy）', (await h.next('error')).code === 'busy');
    h.send({ type: 'ping' });
    check('H9 下载中 ping 仍可响应', (await h.next('pong')).ytdlp === 'stub-1.0');
    h.send({ type: 'cancel' });
    const c = await h.next(['cancelled', 'done', 'error']);
    check('H9 取消生效', c.type === 'cancelled', JSON.stringify(c));
    h.close();
  }

  // H10 帧协议：两帧粘在一次写入里、一帧拆成两次写入，都能正确解码；坏 JSON 不崩溃
  {
    const h = startHost();
    h.raw(Buffer.concat([frame({ type: 'ping' }), frame({ type: 'nope' })]));
    const got = [await h.next(['pong', 'error']), await h.next(['pong', 'error'])].map((m) => m.type).sort();
    check('H10 粘包：两帧一次写入都被处理', got.join() === 'error,pong', got.join());
    const f = frame({ type: 'ping' });
    h.raw(f.subarray(0, 3));
    await new Promise((r) => setTimeout(r, 100));
    h.raw(f.subarray(3));
    check('H10 半包：一帧分两次写入', (await h.next('pong')).type === 'pong');
    const bad = Buffer.from('{not json', 'utf8');
    const head = Buffer.alloc(4);
    head.writeUInt32LE(bad.length, 0);
    h.raw(Buffer.concat([head, bad]));
    check('H10 坏 JSON 返回错误而不崩溃', (await h.next('error')).code === 'bad-message');
    h.send({ type: 'ping' });
    check('H10 坏 JSON 之后仍然可用', (await h.next('pong')).type === 'pong');
    h.close();
  }

  // H11 Chrome 断开时正在下载 → 子进程被收掉、host 退出
  {
    const h = startHost({ FLOWGT_STUB_MODE: 'slow' });
    h.send({ type: 'download', url: YT, kind: 'video' });
    await h.next('progress');
    h.close();
    const code = await Promise.race([h.exited, new Promise((r) => setTimeout(() => r('timeout'), 3000))]);
    check('H11 断开连接时 host 收尾退出', code === 0, `exit=${code}`);
  }

  // H12 yt-dlp 不存在 → ytdlp-missing（引导用户安装）
  {
    const h = startHost({ FLOWGT_YTDLP: '/nonexistent/yt-dlp' });
    h.send({ type: 'ping' });
    check('H12 缺 yt-dlp 时 pong.ytdlp 为 null', (await h.next('pong')).ytdlp === null);
    h.send({ type: 'download', url: YT, kind: 'video' });
    check('H12 缺 yt-dlp 时下载报 ytdlp-missing', (await h.next('error')).code === 'ytdlp-missing');
    h.close();
  }
} catch (e) {
  check('未捕获异常', false, e.stack?.split('\n').slice(0, 2).join(' | '));
}

const failed = results.filter((r) => !r).length;
console.log(`\n===== host 集成测试 ${results.length - failed}/${results.length} 通过 =====`);
process.exit(failed ? 1 : 0);
