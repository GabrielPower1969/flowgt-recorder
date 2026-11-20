// The other recording flows of the real app, checked through its floating window (Windows; see bench-app-win.ps1).
//
//   node scripts/bench-flows.mjs <ffprobe> [workRoot]
//
// ffprobe: any ffprobe (src-tauri/binaries/ffprobe-*.exe works); the ffmpeg next to it decodes the file.
// workRoot: the app's work folder, "<output folder>/.flowgt-recorder"; derived from the settings when left out.
// Camera and microphone are switched on. Prints one JSON object:
//   plain        3 s countdown, 5 s recording: where each stream starts, first packets (is the first video
//                packet a keyframe, "K"), largest gap between packets, decode errors over the whole file
//   pause        pause / resume / stop times and the file's streams after a pause in the middle
//   cancel       cancel during the countdown: ffmpeg children of the app before and after, phase, work folders
//   noCountdown  click to "recording" without a countdown
// Leaves the countdown at 3 s. Results used in docs/reports/12-Windows-开始与保存提速-2026-11-20.md.
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { join, dirname, basename } from "node:path";
import { connect, inv, applySettings, sleep } from "./bench-cdp.mjs";

const [probe, workRootArg] = process.argv.slice(2);
if (!probe) throw new Error("usage: node scripts/bench-flows.mjs <ffprobe> [workRoot]");
const ffmpeg = join(dirname(probe), basename(probe).replace("ffprobe", "ffmpeg"));
const { ev, close } = await connect();

const settings = await ev(inv("get_settings"));
const workRoot = workRootArg ||
  join(settings.outputDir || join(process.env.USERPROFILE || "", "Videos", "FlowGT Recorder"), ".flowgt-recorder");
const workFolders = () => (existsSync(workRoot) ? readdirSync(workRoot).length : 0);

const phase = async () => (await ev(inv("get_recorder_state"))).phase;
const until = async (want, ms = 30000) => {
  const t0 = Date.now();
  for (;;) {
    const p = await phase();
    if (want.includes(p)) return Date.now() - t0;
    if (Date.now() - t0 > ms) throw new Error(`phase ${p}, wanted ${want}`);
    await sleep(15);
  }
};
// ffmpeg processes started by the app (other programs' ffmpeg on the machine do not count).
const appFfmpegs = () => +execFileSync("powershell", ["-NoProfile", "-Command",
  "$app = @(Get-Process flowgt-recorder -ErrorAction SilentlyContinue).Id; " +
  "@(Get-CimInstance Win32_Process -Filter \"name='ffmpeg.exe'\" | Where-Object { $app -contains $_.ParentProcessId }).Count"]).toString().trim();
const probeFile = (f) => JSON.parse(execFileSync(probe,
  ["-v", "error", "-show_entries", "stream=codec_type,start_time,duration:format=duration", "-of", "json", f]).toString());
const packets = (f, stream, entries) => execFileSync(probe,
  ["-v", "error", "-select_streams", stream, "-show_entries", `packet=${entries}`, "-of", "csv=p=0", f]).toString().trim().split(/\s+/);
const maxGap = (f, stream) => {
  const t = packets(f, stream, "pts_time").map(Number).filter((x) => !isNaN(x)).sort((a, b) => a - b);
  let max = 0;
  for (let i = 1; i < t.length; i++) max = Math.max(max, t[i] - t[i - 1]);
  return +max.toFixed(3);
};
const stopAndGet = () => ev(`${inv("stop_recording")}.then((r) => ({ path: r.path, ms: r.info.durationMs }))`);
const camMic = { countdownSeconds: 3, audio: { micEnabled: true }, camera: { enabled: true } };
const result = {};

// Plain start after the countdown.
await applySettings(ev, camMic);
await sleep(3000);
await ev(`${inv("start_recording", "{ meetingName: 'flow-plain' }")}.then(() => 1)`);
await until(["recording"]);
await sleep(5000);
const r0 = await stopAndGet();
const dec = spawnSync(ffmpeg, ["-hide_banner", "-v", "error", "-i", r0.path, "-f", "null", "-"], { encoding: "utf8" });
result.plain = {
  fileMs: r0.ms,
  streams: probeFile(r0.path).streams,
  firstVideoPackets: packets(r0.path, "v:0", "pts_time,flags").slice(0, 3),
  firstAudioPackets: packets(r0.path, "a:0", "pts_time,flags").slice(0, 3),
  maxVideoGapS: maxGap(r0.path, "v:0"),
  maxAudioGapS: maxGap(r0.path, "a:0"),
  decodeErrorLines: dec.stderr.trim().split(/\r?\n/).filter(Boolean).length,
};

// Pause and resume.
await sleep(3000);
await ev(`${inv("start_recording", "{ meetingName: 'flow-pause' }")}.then(() => 1)`);
await until(["recording"]);
await sleep(4000);
let t0 = Date.now();
await ev(`${inv("pause_recording")}.then(() => 1)`);
const pauseMs = Date.now() - t0;
await sleep(3000);
t0 = Date.now();
await ev(`${inv("resume_recording")}.then(() => 1)`);
const resumeMs = Date.now() - t0;
await sleep(4000);
t0 = Date.now();
const r1 = await stopAndGet();
const stopMs = Date.now() - t0;
result.pause = { pauseMs, resumeMs, stopMs, fileMs: r1.ms, recordedMs: 8000, streams: probeFile(r1.path).streams,
  maxVideoGapS: maxGap(r1.path, "v:0"), maxAudioGapS: maxGap(r1.path, "a:0") };

// Cancel during the countdown.
await sleep(3000);
const before = workFolders();
await ev(`${inv("start_recording", "{ meetingName: 'flow-cancel' }")}.then(() => 1)`);
await sleep(2200);
const during = appFfmpegs();
await ev(`${inv("cancel_recording")}.then(() => 1)`);
await sleep(1500);
result.cancel = { ffmpegDuringCountdown: during, ffmpegAfterCancel: appFfmpegs(), phase: await phase(),
  workFoldersBefore: before, workFoldersAfter: workFolders() };

// No countdown.
await applySettings(ev, { countdownSeconds: 0 });
await sleep(3000);
t0 = Date.now();
await ev(`${inv("start_recording", "{ meetingName: 'flow-cd0' }")}.then(() => 1)`);
await until(["recording"]);
const startMs = Date.now() - t0;
await sleep(5000);
const r3 = await stopAndGet();
result.noCountdown = { clickToRecordingMs: startMs, fileMs: r3.ms, streams: probeFile(r3.path).streams };

await applySettings(ev, { countdownSeconds: 3 });
console.log(JSON.stringify(result, null, 1));
close();
process.exit(0);
