// FlowGT Recorder — recording core
// 混音 + 编码 + 落盘信令。被 offscreen.js（生产）和 test-harness.js（测试）共用，
// 保证测试覆盖的就是生产代码本身。

const FlowGTRecorder = (() => {
  let recorder = null;
  let chunks = [];
  let audioCtx = null;
  let tabStream = null;
  let micStream = null;
  let recLabel = '';
  let blobUrl = null;
  let onDone = null;
  let endWatch = null;

  async function getMic() {
    // 麦克风（自己的声音）。未授权/无设备时返回 null，录音降级为只录标签页。
    try {
      return await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: true, noiseSuppression: true },
      });
    } catch (e) {
      console.warn('[FlowGT] mic unavailable, tab-only recording:', e);
      return null;
    }
  }

  function start({ tab, mic, label, playback = true, onStopped = null }) {
    if (recorder && recorder.state !== 'inactive') {
      throw new Error('recording already in progress');
    }
    tabStream = tab;
    micStream = mic;
    recLabel = label || '';
    onDone = onStopped;

    audioCtx = new AudioContext();
    const mixDest = audioCtx.createMediaStreamDestination();

    const tabSrc = audioCtx.createMediaStreamSource(tabStream);
    tabSrc.connect(mixDest);
    if (playback) {
      // tabCapture 会"劫持"标签页声音——必须回放到扬声器，否则开会时听不到对方
      tabSrc.connect(audioCtx.destination);
    }
    if (micStream) {
      audioCtx.createMediaStreamSource(micStream).connect(mixDest);
    }

    recorder = new MediaRecorder(mixDest.stream, {
      mimeType: 'audio/webm;codecs=opus',
      audioBitsPerSecond: 128000,
    });
    chunks = [];
    recorder.ondataavailable = (e) => {
      if (e.data && e.data.size > 0) chunks.push(e.data);
    };
    recorder.onstop = onRecorderStopped;
    recorder.start(10_000); // 每 10s 收一块，降低崩溃丢整段的风险

    // 会议标签页被关闭 → 自动收尾保存。
    // 'ended' 事件在部分采集路径下不可靠，轮询 readyState 兜底。
    const watched = tabStream.getTracks().filter((t) => t.readyState !== 'ended');
    watched.forEach((t) => t.addEventListener('ended', stop));
    endWatch = setInterval(() => {
      if (watched.some((t) => t.readyState === 'ended')) stop();
    }, 2000);

    return !!micStream;
  }

  function stop() {
    if (recorder && recorder.state !== 'inactive') recorder.stop();
  }

  function isActive() {
    return !!(recorder && recorder.state !== 'inactive');
  }

  async function onRecorderStopped() {
    try {
      const blob = new Blob(chunks, { type: 'audio/webm' });
      chunks = [];
      cleanupStreams();
      if (blob.size === 0) return;

      blobUrl = URL.createObjectURL(blob);
      await chrome.runtime.sendMessage({
        target: 'background',
        type: 'save-recording',
        url: blobUrl,
        filename: buildFilename(),
      });
    } catch (e) {
      console.error('[FlowGT] save failed:', e);
    } finally {
      if (onDone) onDone();
    }
  }

  function cleanupStreams() {
    if (endWatch) clearInterval(endWatch);
    endWatch = null;
    for (const s of [tabStream, micStream]) {
      if (s) s.getTracks().forEach((t) => t.stop());
    }
    tabStream = micStream = null;
    if (audioCtx) {
      audioCtx.close().catch(() => {});
      audioCtx = null;
    }
  }

  function revokeBlobUrl() {
    if (blobUrl) URL.revokeObjectURL(blobUrl);
    blobUrl = null;
  }

  function buildFilename() {
    const d = new Date();
    const pad = (n) => String(n).padStart(2, '0');
    const stamp = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}_${pad(d.getHours())}${pad(d.getMinutes())}`;
    // 过滤 Windows/macOS 均非法的字符，中文等 Unicode 保留
    const label = recLabel
      .trim()
      .replace(/[\\/:*?"<>|]/g, '')
      .replace(/\s+/g, '_')
      .slice(0, 60);
    return label ? `${stamp}_${label}.webm` : `${stamp}_meeting.webm`;
  }

  return { getMic, start, stop, isActive, revokeBlobUrl };
})();
