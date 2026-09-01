// FlowGT Recorder — service worker
// 职责：拿 tabCapture streamId、管理 offscreen 文档、落盘下载、维护录音状态。
// 真正的音频采集/混音/编码都在 offscreen.js 里（service worker 没有 DOM/AudioContext）。

const OFFSCREEN_URL = 'offscreen.html';

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

        default:
          sendResponse({ ok: false, error: `未知消息类型: ${msg.type}` });
      }
    } catch (e) {
      console.error('[FlowGT]', e);
      await clearRecState().catch(() => {});
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
