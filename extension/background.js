// FlowGT Recorder — service worker
// 职责：拿 tabCapture streamId、管理 offscreen 文档、落盘下载、维护录音状态。
// 真正的音频采集/混音/编码都在 offscreen.js 里（service worker 没有 DOM/AudioContext）。

importScripts('platform.js');

const OFFSCREEN_URL = 'offscreen.html';
const REC_TYPES = new Set(['start-recording', 'stop-recording', 'save-recording', 'recording-finished']);

async function ensureOffscreen() {
  const contexts = await chrome.runtime.getContexts({
    contextTypes: ['OFFSCREEN_DOCUMENT'],
  });
  if (contexts.length > 0) return;
  await chrome.offscreen.createDocument({
    url: OFFSCREEN_URL,
    reasons: ['USER_MEDIA'],
    justification: '录制会议标签页音频与麦克风，混音后编码保存',
  });
  // createDocument resolve 时 offscreen.js 可能还没注册好监听器 → ping 直到就绪
  const deadline = Date.now() + 3000;
  for (;;) {
    try {
      const pong = await chrome.runtime.sendMessage({ target: 'offscreen', type: 'ping' });
      if (pong && pong.ok) return;
    } catch (_) {
      /* listener 未就绪 */
    }
    if (Date.now() > deadline) throw new Error('offscreen 文档启动超时');
    await new Promise((r) => setTimeout(r, 50));
  }
}

async function closeOffscreen() {
  const contexts = await chrome.runtime.getContexts({
    contextTypes: ['OFFSCREEN_DOCUMENT'],
  });
  if (contexts.length > 0) await chrome.offscreen.closeDocument();
}

async function setRecState(rec) {
  await chrome.storage.session.set({ rec });
}

async function clearRecState() {
  await chrome.storage.session.remove('rec');
  chrome.action.setBadgeText({ text: '' });
}

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg.target !== 'background') return;
  (async () => {
    try {
      switch (msg.type) {
        case 'start-recording': {
          const { rec } = await chrome.storage.session.get('rec');
          if (rec && rec.recording) throw new Error('已有录音在进行中');
          const streamId = await chrome.tabCapture.getMediaStreamId({
            targetTabId: msg.tabId,
          });
          await ensureOffscreen();
          const res = await chrome.runtime.sendMessage({
            target: 'offscreen',
            type: 'start',
            streamId,
            label: msg.label,
          });
          if (!res || !res.ok) throw new Error(res?.error || 'offscreen 启动失败');
          await setRecState({
            recording: true,
            startTime: Date.now(),
            tabId: msg.tabId,
            label: msg.label,
            micOk: res.micOk,
          });
          chrome.action.setBadgeText({ text: 'REC' });
          chrome.action.setBadgeBackgroundColor({ color: '#d32f2f' });
          sendResponse({ ok: true, micOk: res.micOk });
          break;
        }

        case 'stop-recording': {
          // offscreen 收到后会 stop recorder → 回一条 save-recording
          await chrome.runtime.sendMessage({ target: 'offscreen', type: 'stop' });
          sendResponse({ ok: true });
          break;
        }

        case 'save-recording': {
          // offscreen 已生成 blob URL；下载完成前不能关 offscreen（blob URL 会失效）
          const downloadId = await chrome.downloads.download({
            url: msg.url,
            filename: `flowgt-recordings/${msg.filename}`,
            saveAs: false,
            conflictAction: 'uniquify',
          });
          watchDownload(downloadId);
          await clearRecState();
          sendResponse({ ok: true });
          break;
        }

        case 'get-account': {
          // flowgt.co.nz 登录态：host_permissions 下的扩展 fetch 不受 CORS/SameSite 限制，
          // fg_sess cookie 会随请求带上。60 秒缓存。
          const cached = (await chrome.storage.session.get('account')).account;
          if (cached && Date.now() - cached.at < 60_000) {
            sendResponse(cached.data);
            break;
          }
          let data;
          try {
            const r = await fetch('https://flowgt.co.nz/api/session', {
              credentials: 'include',
            });
            if (r.ok) {
              const j = await r.json();
              data = { signedIn: true, email: j.user?.email, plan: j.user?.plan };
            } else {
              data = { signedIn: false };
            }
          } catch {
            data = { signedIn: false, offline: true };
          }
          await chrome.storage.session.set({ account: { at: Date.now(), data } });
          sendResponse(data);
          break;
        }

        case 'recording-finished': {
          // 录音收尾（正常停止或会议标签页被关闭自动收尾），save-recording 另行到达
          await clearRecState();
          sendResponse({ ok: true });
          break;
        }

        case 'host-status':
          sendResponse(await hostStatus());
          break;

        case 'download-media':
          sendResponse(await startDownload(msg));
          break;

        case 'cancel-download':
          if (hostPort) hostPort.postMessage({ type: 'cancel' });
          sendResponse({ ok: true });
          break;

        case 'clear-download':
          await setDl(null);
          sendResponse({ ok: true });
          break;

        case 'show-downloads':
          chrome.downloads.showDefaultFolder();
          sendResponse({ ok: true });
          break;

        default:
          sendResponse({ ok: false, error: `未知消息类型: ${msg.type}` });
      }
    } catch (e) {
      console.error('[FlowGT]', e);
      // 只有录音链路的失败才清录音状态——下载出错不能抹掉正在进行的录音
      if (REC_TYPES.has(msg.type)) await clearRecState().catch(() => {});
      await chrome.storage.session
        .set({ lastError: { msg: e.message || String(e), at: Date.now(), type: msg.type } })
        .catch(() => {});
      sendResponse({ ok: false, error: e.message || String(e) });
    }
  })();
  return true; // 异步 sendResponse
});

function watchDownload(downloadId) {
  const listener = (delta) => {
    if (delta.id !== downloadId || !delta.state) return;
    if (delta.state.current === 'complete' || delta.state.current === 'interrupted') {
      chrome.downloads.onChanged.removeListener(listener);
      // 通知 offscreen 释放 blob，再关掉 offscreen 文档
      chrome.runtime
        .sendMessage({ target: 'offscreen', type: 'saved' })
        .catch(() => {})
        .finally(() => closeOffscreen().catch(() => {}));
    }
  };
  chrome.downloads.onChanged.addListener(listener);
}

// ---------- 下载助手：Native Messaging → 本机 yt-dlp（host/flowgt-host.mjs） ----------
// 状态唯一真源：storage.session.dl = {active, platform, kind, title, percent, speed, eta,
//   note, file, done, error, errorMsg, at}；popup 通过 storage.onChanged 渲染。
// 端口只在"查询状态/下载中"打开，空闲即断开，避免 host 进程与 service worker 常驻。

const HOST_NAME = 'nz.co.flowgt.recorder';
let hostPort = null;
let pendingPing = null;
let dlState = null;

async function setDl(next) {
  dlState = next ? { ...next, at: Date.now() } : null;
  if (dlState) await chrome.storage.session.set({ dl: dlState });
  else await chrome.storage.session.remove('dl');
}
const patchDl = (patch) => setDl({ ...(dlState || {}), ...patch });

// service worker 重启时端口必然已断，残留的 active 下载已被 host 收掉 → 标记为中断
chrome.storage.session.get('dl').then(({ dl }) => {
  if (dl?.active && !hostPort) setDl({ ...dl, active: false, error: 'interrupted', errorMsg: 'Download was interrupted' });
  else if (dl && !dlState) dlState = dl;
});

function hostErrorCode(message) {
  if (/not found/i.test(message)) return 'host-missing';
  if (/forbidden/i.test(message)) return 'host-forbidden';
  return 'host-disconnected';
}

function disconnectHost() {
  if (!hostPort) return;
  hostPort.disconnect();
  hostPort = null;
}

function connectHost() {
  if (hostPort) return hostPort;
  const port = chrome.runtime.connectNative(HOST_NAME);
  hostPort = port;
  port.onMessage.addListener(onHostMessage);
  port.onDisconnect.addListener(() => {
    const err = chrome.runtime.lastError?.message || 'Local helper disconnected';
    if (hostPort === port) hostPort = null;
    if (pendingPing) pendingPing({ installed: false, error: hostErrorCode(err), errorMsg: err });
    if (dlState?.active) patchDl({ active: false, error: hostErrorCode(err), errorMsg: err });
  });
  return port;
}

function onHostMessage(m) {
  switch (m.type) {
    case 'pong':
      if (pendingPing) pendingPing({ installed: true, ytdlp: m.ytdlp, outDir: m.outDir });
      if (!dlState?.active) disconnectHost();
      break;
    case 'progress':
      patchDl({ percent: m.percent, speed: m.speed, eta: m.eta, note: m.note || '' });
      break;
    case 'done':
      patchDl({ active: false, done: true, percent: 100, file: m.file, note: '', speed: '', eta: '' });
      disconnectHost();
      break;
    case 'cancelled':
      setDl(null);
      disconnectHost();
      break;
    case 'error':
      patchDl({ active: false, error: m.code, errorMsg: m.msg });
      disconnectHost();
      break;
  }
}

function hostStatus() {
  return new Promise((resolve) => {
    const finish = (r) => {
      pendingPing = null;
      clearTimeout(timer);
      resolve({ ok: true, ...r });
    };
    const timer = setTimeout(() => finish({ installed: false, error: 'host-timeout' }), 5000);
    pendingPing = finish;
    try {
      connectHost().postMessage({ type: 'ping' });
    } catch (e) {
      finish({ installed: false, error: 'host-missing', errorMsg: e.message });
    }
  });
}

async function startDownload({ url, kind, useCookies, title }) {
  if (dlState?.active) return { ok: false, error: 'busy' };
  const platform = FlowGTPlatform.detectPlatform(url);
  if (!platform) return { ok: false, error: 'url-not-allowed' };
  if (kind !== 'video' && kind !== 'audio') return { ok: false, error: 'bad-kind' };
  await setDl({
    active: true, platform, kind, title: (title || '').slice(0, 200),
    percent: 0, speed: '', eta: '', note: '', file: null, done: false, error: null, errorMsg: '',
  });
  try {
    connectHost().postMessage({ type: 'download', url, kind, useCookies: !!useCookies });
  } catch (e) {
    await patchDl({ active: false, error: 'host-missing', errorMsg: e.message });
  }
  return { ok: true };
}
