// FlowGT Recorder — offscreen document
// 生产采集入口：tabCapture streamId + 麦克风 → recorder-core。

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg.target !== 'offscreen') return;
  if (msg.type === 'ping') {
    sendResponse({ ok: true });
    return;
  }
  if (msg.type === 'start') {
    start(msg.streamId, msg.label)
      .then((micOk) => sendResponse({ ok: true, micOk }))
      .catch((e) => sendResponse({ ok: false, error: e.message || String(e) }));
    return true;
  }
  if (msg.type === 'stop') {
    FlowGTRecorder.stop();
    sendResponse({ ok: true });
  }
  if (msg.type === 'saved') {
    FlowGTRecorder.revokeBlobUrl();
    sendResponse({ ok: true });
  }
});

async function start(streamId, label) {
  // 标签页音频（会议里对方的声音）。
  // tab capture 的 getUserMedia 必须 audio+video 同时请求（官方示例如此，
  // 只请求 audio 会报 "Requested device not found"）。拿到后立刻停掉视频轨。
  const constraint = {
    mandatory: { chromeMediaSource: 'tab', chromeMediaSourceId: streamId },
  };
  const tab = await navigator.mediaDevices.getUserMedia({
    audio: constraint,
    video: constraint,
  });
  tab.getVideoTracks().forEach((t) => t.stop());

  const mic = await FlowGTRecorder.getMic();
  FlowGTRecorder.start({
    tab,
    mic,
    label,
    playback: true,
    onStopped: () => {
      chrome.runtime
        .sendMessage({ target: 'background', type: 'recording-finished' })
        .catch(() => {});
    },
  });
  return !!mic;
}
