// Start/stop latency of the real app, driven through its floating window (Windows; see bench-app-win.ps1).
//
//   node scripts/bench-start-stop.mjs <label> [runs=3] [recordSeconds=8] ['<settings patch json>']
//   e.g. node scripts/bench-start-stop.mjs cam-mic 5 8 '{"countdownSeconds":3,"audio":{"micEnabled":true},"camera":{"enabled":true}}'
//
// Per run: start (with the countdown from the settings), wait for the "recording" state, record, stop,
// wait for the saved file. One JSON row per run, then a SUMMARY line of medians.
//   countdown_end_to_recording_ms  "recording" state minus the moment the countdown ends
//                                  (first tick + countdown seconds; the click itself without a countdown).
//                                  Negative means the app was recording before "1" disappeared.
//   stop_click_to_file_ms          stop click until stop_recording returns the file
//   missing_ms                     (stop click - countdown end) - file duration: content the user lost
// Results used in docs/reports/12-Windows-开始与保存提速-2026-11-20.md.
import { connect, inv, applySettings, median, sleep } from "./bench-cdp.mjs";

const [label = "run", runsArg, secArg, patch] = process.argv.slice(2);
const runs = +runsArg || 3;
const recMs = (+secArg || 8) * 1000;
const { ev, close } = await connect();

// Event log kept in the page, stamped with the page clock.
await ev(`(async () => { if (window.__bench) return; window.__bench = []; const I = window.__TAURI_INTERNALS__;
  for (const name of ["countdown://tick", "recorder://state", "recorder://finalizing", "recorder://finished", "recorder://error"]) {
    const h = I.transformCallback((e) => window.__bench.push({ t: Date.now(), name, p: e.payload }));
    await I.invoke("plugin:event|listen", { event: name, target: { kind: "Any" }, handler: h });
  } return 1; })()`);
if (patch) await applySettings(ev, JSON.parse(patch));
const cd = (await ev(inv("get_settings"))).countdownSeconds;

const rows = [];
for (let r = 0; r < runs; r++) {
  await sleep(4000); // let device lists and caches age the way a user's would
  await ev(`window.__bench.length = 0, 1`);
  const click = await ev(`(() => { const t = Date.now();
    ${inv("start_recording", "{ meetingName: 'bench' }")}.catch((e) => window.__bench.push({ t: Date.now(), name: "start-error", p: e }));
    return t; })()`);
  let log, recAt;
  for (;;) {
    await sleep(20);
    log = await ev(`window.__bench`);
    const rec = log.find((e) => e.name === "recorder://state" && e.p.phase === "recording");
    if (rec) { recAt = rec.t; break; }
    const errors = log.filter((e) => /error/.test(e.name));
    if (errors.length) throw new Error(JSON.stringify(errors));
    if (Date.now() - click > 40000) throw new Error("not recording after 40 s");
  }
  const first = log.find((e) => e.name === "countdown://tick" && e.p.remaining === cd);
  const end = cd > 0 && first ? first.t + cd * 1000 : click;

  await sleep(recMs);
  await ev(`window.__bench.length = 0, 1`);
  const stop = await ev(`(() => { const t = Date.now(); window.__stopRes = null;
    ${inv("stop_recording")}
      .then((r) => { window.__stopRes = { t: Date.now(), ms: r.info.durationMs, bytes: r.info.bytes, v: r.info.videoCodec }; })
      .catch((e) => { window.__stopRes = { err: e }; });
    return t; })()`);
  let res;
  for (;;) {
    await sleep(20);
    res = await ev(`window.__stopRes`);
    if (res) break;
    if (Date.now() - stop > 180000) throw new Error("stop took more than 180 s");
  }
  if (res.err) throw new Error(JSON.stringify(res.err));
  const fin = (await ev(`window.__bench`)).find((e) => e.name === "recorder://state" && e.p.phase === "finalizing");
  const row = {
    run: r + 1,
    countdown_end_to_recording_ms: recAt - end,
    click_to_recording_ms: recAt - click,
    stop_click_to_saving_ui_ms: fin ? fin.t - stop : null,
    stop_click_to_file_ms: res.t - stop,
    wanted_ms: stop - end,
    file_ms: res.ms,
    missing_ms: stop - end - res.ms,
    mb: +(res.bytes / 1e6).toFixed(2),
    codec: res.v,
  };
  rows.push(row);
  console.log(JSON.stringify(row));
}
const med = (k) => median(rows.map((x) => x[k]));
console.log(`SUMMARY ${label}: median countdown_end_to_recording ${med("countdown_end_to_recording_ms")} ms, ` +
  `stop_click_to_file ${med("stop_click_to_file_ms")} ms, missing ${med("missing_ms")} ms (runs ${rows.length})`);
close();
process.exit(0);
