// 测试装置：用 getDisplayMedia（配 --auto-select-tab-capture-source-by-title）
// 获得真实标签页音频，其余全部走生产 recorder-core。
const out = (t) => (document.getElementById('out').textContent = t);

document.getElementById('startBtn').addEventListener('click', async () => {
  try {
    const label = new URLSearchParams(location.search).get('label') || 'harness';
    const tab = await navigator.mediaDevices.getDisplayMedia({ video: true, audio: true });
    // 注意：视频轨不停——gdm 路径下标签页关闭只有视频轨发 ended，
    // core 靠监听所有轨道来自动收尾（生产 tabCapture 路径音轨会正常 ended）。
    const mic = await FlowGTRecorder.getMic();
    window.__tab = tab; // 测试观测用
    const micOk = FlowGTRecorder.start({
      tab,
      mic,
      label,
      playback: false, // 测试环境不需要回放
      onStopped: () => (document.title = 'harness-stopped'),
    });
    out(`RECORDING micOk=${micOk} fmt=${FlowGTRecorder.pickFormat().ext}`);
  } catch (e) {
    out(`FAIL ${e.name}: ${e.message}`);
  }
});

document.getElementById('stopBtn').addEventListener('click', () => {
  FlowGTRecorder.stop();
  out('STOPPING');
});

chrome.runtime.onMessage.addListener((msg, _s, sendResponse) => {
  if (msg.target === 'offscreen' && msg.type === 'saved') {
    FlowGTRecorder.revokeBlobUrl();
    sendResponse({ ok: true });
  }
});
