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
