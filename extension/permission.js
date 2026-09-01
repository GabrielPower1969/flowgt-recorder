// One-time microphone grant (extension-origin permission is shared with the
// offscreen document, so recordings pick up the mic automatically afterwards).
navigator.mediaDevices
  .getUserMedia({ audio: true })
  .then((stream) => {
    stream.getTracks().forEach((t) => t.stop());
    document.getElementById('result').textContent =
      '✓ Microphone enabled. You can close this tab and start recording.';
  })
  .catch((e) => {
    const el = document.getElementById('result');
    el.className = 'err';
    el.textContent = `Permission failed (${e.name}). Click the icon left of the address bar, allow the microphone, then reload this page.`;
  });
