/**
 * Vitest setup: jest-dom matchers plus the browser APIs jsdom lacks
 * (pointer events, ResizeObserver, matchMedia, media devices, AudioContext, clipboard).
 */
import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// Web Storage: Node 25+ defines its own `localStorage` / `sessionStorage` globals, which are undefined
// without `--localstorage-file` and shadow the jsdom ones. Point the globals back at jsdom's Storage.
for (const key of ["localStorage", "sessionStorage"] as const) {
  if (typeof globalThis[key] !== "undefined") continue;
  const dom = (globalThis as { jsdom?: { window: Window } }).jsdom;
  if (dom) Object.defineProperty(globalThis, key, { value: dom.window[key], configurable: true, writable: true });
}

// Deterministic language: the mock settings default to "auto", which resolves via navigator.language.
Object.defineProperty(window.navigator, "language", { value: "zh-CN", configurable: true });

// PointerEvent (jsdom has none). Minimal: MouseEvent + pointer fields.
if (typeof window.PointerEvent === "undefined") {
  class PointerEventPolyfill extends MouseEvent {
    pointerId: number;
    pointerType: string;
    pressure: number;
    isPrimary: boolean;
    width: number;
    height: number;
    constructor(type: string, init: PointerEventInit = {}) {
      super(type, init);
      this.pointerId = init.pointerId ?? 1;
      this.pointerType = init.pointerType ?? "mouse";
      this.pressure = init.pressure ?? 0.5;
      this.isPrimary = init.isPrimary ?? true;
      this.width = init.width ?? 1;
      this.height = init.height ?? 1;
    }
  }
  Object.defineProperty(window, "PointerEvent", { value: PointerEventPolyfill, configurable: true, writable: true });
}
if (typeof Element.prototype.setPointerCapture !== "function") {
  Element.prototype.setPointerCapture = () => {};
  Element.prototype.releasePointerCapture = () => {};
  Element.prototype.hasPointerCapture = () => false;
}
if (typeof Element.prototype.scrollIntoView !== "function") {
  Element.prototype.scrollIntoView = () => {};
}

// ResizeObserver: never fires; components fall back to their width/height props.
if (typeof window.ResizeObserver === "undefined") {
  class ResizeObserverStub {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  Object.defineProperty(window, "ResizeObserver", { value: ResizeObserverStub, configurable: true, writable: true });
}

// matchMedia: light scheme, no reduced motion; listeners accepted and ignored.
if (typeof window.matchMedia !== "function") {
  window.matchMedia = (query: string): MediaQueryList =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }) as unknown as MediaQueryList;
}

// Media devices: a fake stream whose tracks can be stopped and inspected.
export function makeFakeStream(kind: "video" | "audio" = "video", deviceId = "default"): MediaStream {
  const track = {
    kind,
    id: `${kind}-${deviceId}`,
    label: `Fake ${kind}`,
    enabled: true,
    readyState: "live",
    stop: vi.fn(function (this: { readyState: string }) {
      this.readyState = "ended";
    }),
    getSettings: () => ({ deviceId }),
    addEventListener: () => {},
    removeEventListener: () => {},
  };
  const tracks = [track];
  return {
    id: `stream-${deviceId}`,
    active: true,
    getTracks: () => tracks,
    getVideoTracks: () => tracks.filter((t) => t.kind === "video"),
    getAudioTracks: () => tracks.filter((t) => t.kind === "audio"),
    addEventListener: () => {},
    removeEventListener: () => {},
  } as unknown as MediaStream;
}

export const fakeMediaDevices = {
  getUserMedia: vi.fn(async (constraints: MediaStreamConstraints) =>
    makeFakeStream(constraints.audio ? "audio" : "video"),
  ),
  enumerateDevices: vi.fn(async (): Promise<MediaDeviceInfo[]> => [
    { deviceId: "cam-1", groupId: "g1", kind: "videoinput", label: "FaceTime HD Camera", toJSON: () => ({}) },
    { deviceId: "mic-1", groupId: "g2", kind: "audioinput", label: "MacBook Pro Microphone", toJSON: () => ({}) },
    { deviceId: "mic-2", groupId: "g3", kind: "audioinput", label: "AirPods Pro", toJSON: () => ({}) },
  ]),
  addEventListener: () => {},
  removeEventListener: () => {},
};
Object.defineProperty(window.navigator, "mediaDevices", { value: fakeMediaDevices, configurable: true });

// AudioContext: analyser returns silence (all samples 0) unless a test overrides getFloatTimeDomainData.
export class FakeAnalyserNode {
  fftSize = 2048;
  smoothingTimeConstant = 0.8;
  get frequencyBinCount() {
    return this.fftSize / 2;
  }
  getFloatTimeDomainData(arr: Float32Array) {
    arr.fill(0);
  }
  connect() {}
  disconnect() {}
}
export class FakeAudioContext {
  state: AudioContextState = "running";
  sampleRate = 48000;
  static instances: FakeAudioContext[] = [];
  closed = false;
  constructor() {
    FakeAudioContext.instances.push(this);
  }
  createMediaStreamSource() {
    return { connect: () => {}, disconnect: () => {} };
  }
  createAnalyser() {
    return new FakeAnalyserNode();
  }
  async resume() {
    this.state = "running";
  }
  async close() {
    this.closed = true;
    this.state = "closed";
  }
}
Object.defineProperty(window, "AudioContext", { value: FakeAudioContext, configurable: true, writable: true });

// Media element playback is a no-op in jsdom.
Object.defineProperty(HTMLMediaElement.prototype, "play", { value: vi.fn(() => Promise.resolve()), configurable: true });
Object.defineProperty(HTMLMediaElement.prototype, "pause", { value: vi.fn(), configurable: true });
Object.defineProperty(HTMLMediaElement.prototype, "load", { value: vi.fn(), configurable: true });

// Clipboard.
export const fakeClipboard = { writeText: vi.fn(async (_text: string) => {}) };
Object.defineProperty(window.navigator, "clipboard", { value: fakeClipboard, configurable: true });
