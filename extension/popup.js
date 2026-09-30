// FlowGT Recorder — popup
const $ = (id) => document.getElementById(id);
let timerInterval = null;
let isRecording = false;

async function refreshUI() {
  const { rec } = await chrome.storage.session.get('rec');
  isRecording = !!(rec && rec.recording);
  const btn = $('actionBtn');
  btn.classList.toggle('recording', isRecording);
  btn.textContent = isRecording ? 'Stop & save' : 'Start recording';
  $('label').disabled = isRecording;
  if (isRecording) {
    $('label').value = rec.label || '';
    $('micWarn').style.display = rec.micOk ? 'none' : 'block';
    startTimer(rec.startTime);
  } else {
    stopTimer();
    setStatus('Ready — records this tab + your mic');
    checkMicPermission();
  }
}

function setStatus(html, cls = '') {
  const el = $('status');
  el.className = cls;
  el.innerHTML = html;
}

function startTimer(startTime) {
  const render = () => {
    const s = Math.floor((Date.now() - startTime) / 1000);
    const mm = String(Math.floor(s / 60)).padStart(2, '0');
    const ss = String(s % 60).padStart(2, '0');
    setStatus(
      `<span class="rec-dot"></span>Recording <span id="timer">${mm}:${ss}</span>`,
      'recording'
    );
  };
  render();
  timerInterval = setInterval(render, 1000);
}

function stopTimer() {
  if (timerInterval) clearInterval(timerInterval);
  timerInterval = null;
}

async function checkMicPermission() {
  try {
    const st = await navigator.permissions.query({ name: 'microphone' });
    $('micWarn').style.display = st.state === 'granted' ? 'none' : 'block';
  } catch {
    /* leave hidden if query unsupported */
  }
}

$('actionBtn').addEventListener('click', async () => {
  const btn = $('actionBtn');
  btn.disabled = true;
  try {
    if (isRecording) {
      await chrome.runtime.sendMessage({ target: 'background', type: 'stop-recording' });
      stopTimer();
      setStatus('✓ Saved to Downloads/flowgt-recordings/');
      setTimeout(refreshUI, 800);
    } else {
      setStatus('Starting…');
      const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
      if (!tab || tab.url?.startsWith('chrome://')) {
        throw new Error('Switch to the meeting tab first, then click Start.');
      }
      const res = await chrome.runtime.sendMessage({
        target: 'background',
        type: 'start-recording',
        tabId: tab.id,
        label: $('label').value,
      });
      if (!res || !res.ok) throw new Error(res?.error || 'Could not start recording');
      await refreshUI();
    }
  } catch (e) {
    setStatus(e.message, 'error');
  } finally {
    btn.disabled = false;
  }
});

$('grantMic').addEventListener('click', (e) => {
  e.preventDefault();
  chrome.tabs.create({ url: chrome.runtime.getURL('permission.html') });
});

// Account status — flowgt.co.nz session via background fetch
async function loadAccount() {
  const el = $('account');
  try {
    const res = await chrome.runtime.sendMessage({ target: 'background', type: 'get-account' });
    if (res?.signedIn) {
      el.textContent = res.email;
      el.title = `Signed in to FlowGT (${res.plan || 'member'})`;
    } else {
      el.innerHTML = '<a href="#" id="signin">Sign in at flowgt.co.nz →</a>';
      document.getElementById('signin').addEventListener('click', (e) => {
        e.preventDefault();
        chrome.tabs.create({ url: 'https://flowgt.co.nz/portal.html' });
      });
    }
  } catch {
    el.textContent = '';
  }
}

// Platform-aware shortcut hint
if (!navigator.userAgent.includes('Mac')) $('shortcutHint').textContent = 'Ctrl+Shift+9';

refreshUI();
loadAccount();

// ---------- Media download (YouTube / Bilibili → local yt-dlp helper) ----------
// Simplified platform glyphs in each platform's brand colour (not official artwork).
const DL_BRAND = {
  youtube: {
    name: 'YouTube',
    svg: '<svg viewBox="0 0 28 20" aria-hidden="true"><rect width="28" height="20" rx="5" fill="#FF0000"/><path d="M11 5.5v9l8-4.5z" fill="#fff"/></svg>',
    titleSuffix: /\s*-\s*YouTube\s*$/,
  },
  bilibili: {
    name: 'Bilibili',
    svg: '<svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="#00AEEC" stroke-width="2" stroke-linecap="round"><path d="M7.2 2.6l2.6 2.6M16.8 2.6l-2.6 2.6"/><rect x="2" y="5.5" width="20" height="15" rx="4"/><path d="M8 11.2v2.2M16 11.2v2.2"/></svg>',
    titleSuffix: /\s*_哔哩哔哩_bilibili\s*$|\s*-\s*哔哩哔哩.*$/,
  },
};
const DL_DEMO = {
  youtube: { url: 'https://www.youtube.com/watch?v=jNQXAC9IVRw', title: 'Me at the zoo - YouTube' },
  bilibili: { url: 'https://www.bilibili.com/video/BV1xx411c7mD', title: '字幕君交流场所_哔哩哔哩_bilibili' },
};
const DL_ERRORS = {
  'host-missing': 'Local helper not installed. Run host/install.sh (see README).',
  'host-forbidden': 'Local helper rejected this extension. Re-run host/install.sh.',
  'host-timeout': 'Local helper did not respond. Re-run host/install.sh.',
  'ytdlp-missing': 'yt-dlp not found. Install it, then re-run host/install.sh.',
  interrupted: 'Download interrupted. Try again.',
  'host-disconnected': 'Download interrupted. Try again.',
  busy: 'Another download is still running.',
  'url-not-allowed': 'Only YouTube and Bilibili video pages are supported.',
};

let dlPage = null; // {url, title, platform} when the active tab is a video page

function cleanTitle(title, platform) {
  return (title || '').replace(DL_BRAND[platform]?.titleSuffix || /$^/, '').trim();
}

function renderDl(dl) {
  const platform = dl?.platform || dlPage?.platform;
  if (!platform) return;
  const brand = DL_BRAND[platform];
  $('dlLogo').innerHTML = brand.svg;
  $('dlPlatform').textContent = brand.name;
  const title = dl?.active || !dlPage ? dl?.title : dlPage.title;
  $('dlTitle').textContent = title ? `· ${title}` : '';
  $('dlTitle').title = title || '';

  const busy = !!dl?.active;
  $('dlIdle').hidden = busy || !dlPage;
  $('dlCookiesRow').hidden = busy || dlPage?.platform !== 'bilibili';
  $('dlBusy').hidden = !busy;

  if (busy) {
    const pct = Math.max(0, Math.min(100, dl.percent || 0));
    $('dlBar').style.width = `${pct}%`;
    const kind = dl.kind === 'audio' ? 'Audio' : 'Video';
    $('dlProgress').textContent =
      dl.note === 'retrying-without-cookies' ? 'Retrying without login…'
        : pct >= 100 ? `${kind} · Finishing…`
          : [`${kind} · ${pct.toFixed(0)}%`, dl.speed, dl.eta && `${dl.eta} left`].filter(Boolean).join(' · ');
  }

  const res = $('dlResult');
  if (!busy && dl?.done) {
    res.className = 'dl-result ok';
    res.innerHTML = '<span>✓ Saved to Downloads/flowgt-downloads</span><span class="actions"><button class="link" id="dlShow">Show</button><button class="link" id="dlDismiss">Dismiss</button></span>';
    res.hidden = false;
  } else if (!busy && dl?.error) {
    res.className = 'dl-result err';
    const text = DL_ERRORS[dl.error] || (dl.errorMsg ? `Download failed: ${dl.errorMsg}` : 'Download failed.');
    res.innerHTML = '<span></span><span class="actions"><button class="link" id="dlDismiss">Dismiss</button></span>';
    res.firstChild.textContent = text; // errorMsg comes from yt-dlp — never inject as HTML
    res.hidden = false;
  } else {
    res.hidden = true;
  }
  $('dlShow')?.addEventListener('click', () => chrome.runtime.sendMessage({ target: 'background', type: 'show-downloads' }));
  $('dlDismiss')?.addEventListener('click', () => chrome.runtime.sendMessage({ target: 'background', type: 'clear-download' }));
}

async function startDl(kind) {
  if (!dlPage) return;
  $('dlVideo').disabled = $('dlAudio').disabled = true;
  try {
    const res = await chrome.runtime.sendMessage({
      target: 'background',
      type: 'download-media',
      url: dlPage.url,
      kind,
      useCookies: $('dlCookies').checked,
      title: dlPage.title,
    });
    if (!res?.ok) renderDl({ platform: dlPage.platform, error: res?.error || 'unknown' });
  } finally {
    $('dlVideo').disabled = $('dlAudio').disabled = false;
  }
}

async function initDownload() {
  const demo = DL_DEMO[new URLSearchParams(location.search).get('demo')];
  let url, title;
  if (demo) {
    ({ url, title } = demo);
  } else {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    url = tab?.url;
    title = tab?.title;
  }
  const { dl } = await chrome.storage.session.get('dl');
  if (url && FlowGTPlatform.isMediaPage(url)) {
    const platform = FlowGTPlatform.detectPlatform(url);
    dlPage = { url, title: cleanTitle(title, platform), platform };
  }
  if (!dlPage && !dl) return; // not a video page and nothing in flight → card stays hidden

  try {
    const { dlCookies } = await chrome.storage.local.get('dlCookies');
    if (typeof dlCookies === 'boolean') $('dlCookies').checked = dlCookies;
  } catch { /* default: on */ }

  $('dlCard').hidden = false;
  renderDl(dl);

  // Pre-flight: tell the user up front if the local helper is missing (skipped in demo shots)
  if (dlPage && !dl?.active && !demo) {
    const st = await chrome.runtime.sendMessage({ target: 'background', type: 'host-status' });
    if (st && !st.installed) {
      const { dl: now } = await chrome.storage.session.get('dl');
      if (!now?.active) renderDl({ platform: dlPage.platform, error: st.error });
    }
  }
}

$('dlVideo').addEventListener('click', () => startDl('video'));
$('dlAudio').addEventListener('click', () => startDl('audio'));
$('dlCancel').addEventListener('click', () => chrome.runtime.sendMessage({ target: 'background', type: 'cancel-download' }));
$('dlCookies').addEventListener('change', (e) => chrome.storage.local.set({ dlCookies: e.target.checked }));
chrome.storage.onChanged.addListener((changes, area) => {
  if (area === 'session' && changes.dl && !$('dlCard').hidden) renderDl(changes.dl.newValue);
});

initDownload();
