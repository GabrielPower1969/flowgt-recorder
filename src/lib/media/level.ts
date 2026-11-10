/** Microphone level meter: getUserMedia + AnalyserNode, RMS in 0..1, ~30 Hz callbacks. */
import { stopStream } from "./camera";

export interface LevelMeterHandle { stop(): void }

export function rms(samples: Float32Array): number {
  if (samples.length === 0) return 0;
  let sum = 0;
  for (let i = 0; i < samples.length; i++) sum += samples[i] * samples[i];
  return Math.min(1, Math.sqrt(sum / samples.length) * 3);
}

export async function startLevelMeter(deviceLabel: string | null, onLevel: (v: number) => void): Promise<LevelMeterHandle | null> {
  if (typeof navigator === "undefined" || !navigator.mediaDevices?.getUserMedia || typeof AudioContext === "undefined") return null;
  const devices = await navigator.mediaDevices.enumerateDevices().catch(() => []);
  const mic = devices.find((d) => d.kind === "audioinput" && d.label === deviceLabel);
  let stream: MediaStream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: mic ? { deviceId: { exact: mic.deviceId } } : true, video: false });
  } catch {
    return null;
  }
  const ctx = new AudioContext();
  const analyser = ctx.createAnalyser();
  analyser.fftSize = 1024;
  ctx.createMediaStreamSource(stream).connect(analyser);
  const buf = new Float32Array(analyser.fftSize);
  let alive = true;
  const timer = setInterval(() => {
    if (!alive) return;
    analyser.getFloatTimeDomainData(buf);
    onLevel(rms(buf));
  }, 33);
  return {
    stop() {
      alive = false;
      clearInterval(timer);
      stopStream(stream);
      void ctx.close();
    },
  };
}
