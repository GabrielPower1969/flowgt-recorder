// Shared by bench-start-stop.mjs and bench-flows.mjs: talk to the app's floating window over the
// WebView2 debugging port that scripts/bench-app-win.ps1 opens. Needs Node 22+ (global WebSocket).
// Everything is evaluated inside the webview, so every time stamp comes from one clock (the page's Date.now()).
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function connect(port = process.env.CDP_PORT || 9222) {
  const pages = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  const page = pages.find((p) => p.type === "page" && p.url.includes("#/floating"));
  if (!page) throw new Error(`no floating window on port ${port}; start the app with scripts/bench-app-win.ps1`);
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
  let id = 0;
  const pending = new Map();
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    const p = pending.get(m.id);
    if (!p) return;
    pending.delete(m.id);
    m.error ? p.rej(new Error(JSON.stringify(m.error))) : p.res(m.result);
  };
  const send = (method, params = {}) => new Promise((res, rej) => {
    const i = ++id;
    pending.set(i, { res, rej });
    ws.send(JSON.stringify({ id: i, method, params }));
  });
  /** Evaluate an expression in the page and return its value. */
  const ev = async (js) => {
    const r = await send("Runtime.evaluate", { expression: js, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || "exception in page");
    return r.result.value;
  };
  return { ev, close: () => ws.close() };
}

/** The page-side expression that calls an app command. */
export const inv = (cmd, args = "{}") => `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${args})`;

/** Merge `patch` into the saved settings and record the whole first display. */
export const applySettings = (ev, patch) => ev(`(async () => {
  const merge = (t, p) => { for (const k in p) {
    if (p[k] && typeof p[k] === "object" && !Array.isArray(p[k]) && t[k] && typeof t[k] === "object") merge(t[k], p[k]);
    else t[k] = p[k]; } return t; };
  const s = await ${inv("get_settings")};
  merge(s, ${JSON.stringify(patch)});
  s.capture = { kind: "display", displayId: "0" };
  await ${inv("save_settings", "{ settings: s }")};
  return 1; })()`);

export const median = (values) => {
  const v = values.filter((x) => x !== null && x !== undefined).sort((a, b) => a - b);
  return v.length ? v[Math.floor(v.length / 2)] : null;
};
