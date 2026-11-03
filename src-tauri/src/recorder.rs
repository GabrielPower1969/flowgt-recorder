//! The ffmpeg process supervisor.
//!
//! One recording session is a list of segments (D4). Each segment is one ffmpeg process writing
//! a fragmented MP4:
//!
//! * **pause**  = ask the running segment to finish (`q` on stdin), keep the session open;
//! * **resume** = start the next segment;
//! * **stop**   = finish the running segment, then remux (one segment) or concat (several)
//!   with `-c copy` into the final file;
//! * **camera lost** = ffmpeg exits by itself; a new segment without the camera starts at once.
//!
//! Every state change rewrites `manifest.json` atomically (temp file + rename), so a crash at any
//! moment leaves something `recovery::decide` can finish.

use crate::error::{CmdResult, CommandError};
use crate::sidecar::{self, Tools};
use chrono::Utc;
use recorder_core::{budget, command, progress::{classify_stderr_line, ProgressParser}, recovery, *};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex};

/// Where the supervisor reports to. The Tauri app turns these into events; tests collect them.
pub trait EventSink: Send + Sync + 'static {
    fn state(&self, state: &RecorderState);
    fn progress(&self, progress: &Progress);
    fn warning(&self, warning: &Warning);
    fn error(&self, code: &str, detail: &str);
    fn finished(&self, result: &RecordingResult);
}

pub struct NullSink;
impl EventSink for NullSink {
    fn state(&self, _: &RecorderState) {}
    fn progress(&self, _: &Progress) {}
    fn warning(&self, _: &Warning) {}
    fn error(&self, _: &str, _: &str) {}
    fn finished(&self, _: &RecordingResult) {}
}

/// Everything a session needs; built by the commands layer (or the self-test).
#[derive(Clone)]
pub struct StartParams {
    /// Template for every segment; `output` is replaced per segment.
    pub plan: RecordingPlan,
    pub session_id: String,
    pub meeting_name: String,
    pub final_output: PathBuf,
    pub work_dir: PathBuf,
    pub target_mb_per_hour: u32,
    pub low_disk_warn_gb: u32,
    pub tools: Tools,
    /// The sck-audio helper, when `plan.system_audio` is a `PcmPipe` fed by it.
    pub helper: Option<PathBuf>,
}

const STOP_GRACE: Duration = Duration::from_secs(8);
const START_TIMEOUT: Duration = Duration::from_secs(12);
/// A segment that dies sooner than this after (re)starting counts as a failed start.
const QUICK_FAIL: Duration = Duration::from_secs(3);
const EMIT_EVERY: Duration = Duration::from_millis(500);
const MONITOR_EVERY: Duration = Duration::from_secs(5);
/// Below this much free space the recording is finished early instead of corrupting the disk.
const HARD_FLOOR_BYTES: u64 = 300 * 1_000_000;

struct Live {
    index: u32,
    stop_tx: Option<oneshot::Sender<()>>,
    done_rx: oneshot::Receiver<SegmentOutcome>,
    started: Instant,
}

#[derive(Debug, Default)]
struct SegmentOutcome {
    success: bool,
    code: Option<i32>,
    stderr_tail: Vec<String>,
    classes: Vec<&'static str>,
}

struct Session {
    params: StartParams,
    manifest: SessionManifest,
    live: Option<Live>,
    first_progress: Option<oneshot::Sender<()>>,
    seg_started: Option<Instant>,
    elapsed_before_ms: u64,
    closed_bytes: u64,
    live_bytes: u64,
    camera_connected: bool,
    warnings: Vec<Warning>,
    last_emit: Instant,
    last_error_tail: Vec<String>,
    monitor: Option<tokio::task::JoinHandle<()>>,
    helper_child: Option<Child>,
}

struct Inner {
    phase: Phase,
    /// A stop/pause/resume is in flight; other transitions are refused.
    transition: bool,
    session: Option<Session>,
}

struct Shared {
    inner: Mutex<Inner>,
    sink: Arc<dyn EventSink>,
}

#[derive(Clone)]
pub struct Recorder {
    shared: Arc<Shared>,
}

fn busy(what: &str) -> CommandError {
    CommandError::busy(what.to_string())
}

fn snapshot(inner: &Inner) -> RecorderState {
    let Some(s) = &inner.session else {
        return RecorderState { phase: inner.phase, ..RecorderState::default() };
    };
    let live_ms = s.seg_started.map_or(0, |t| u64::try_from(t.elapsed().as_millis()).unwrap_or(0));
    let elapsed_ms = s.elapsed_before_ms + live_ms;
    let bytes = s.closed_bytes + s.live_bytes;
    RecorderState {
        phase: inner.phase,
        session_id: Some(s.params.session_id.clone()),
        meeting_name: Some(s.params.meeting_name.clone()),
        elapsed_ms,
        bytes_written: bytes,
        estimated_total_bytes: budget::estimate_bytes(&s.params.plan.budget, elapsed_ms),
        segments: u32::try_from(s.manifest.segments.len()).unwrap_or(u32::MAX),
        camera_connected: s.camera_connected,
        warnings: s.warnings.clone(),
    }
}

async fn write_manifest(m: &SessionManifest) -> CmdResult<()> {
    let path = recovery::manifest_path(&m.work_dir);
    let tmp = path.with_extension("json.tmp");
    tokio::fs::write(&tmp, recovery::manifest_to_json(m)?).await?;
    tokio::fs::rename(&tmp, &path).await?;
    Ok(())
}

impl Recorder {
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        Recorder { shared: Arc::new(Shared { inner: Mutex::new(Inner { phase: Phase::Idle, transition: false, session: None }), sink }) }
    }

    pub async fn state(&self) -> RecorderState {
        snapshot(&*self.shared.inner.lock().await)
    }

    pub async fn phase(&self) -> Phase {
        self.shared.inner.lock().await.phase
    }

    /// Mark the countdown phase (the commands layer ticks; the recorder only remembers).
    pub async fn begin_countdown(&self) -> CmdResult<RecorderState> {
        let mut inner = self.shared.inner.lock().await;
        if inner.phase != Phase::Idle || inner.transition {
            return Err(busy("a recording is already in progress"));
        }
        inner.phase = Phase::Countdown;
        let st = snapshot(&inner);
        self.shared.sink.state(&st);
        Ok(st)
    }

    /// Abort a countdown that has not started recording.
    pub async fn abort_countdown(&self) -> RecorderState {
        let mut inner = self.shared.inner.lock().await;
        if inner.phase == Phase::Countdown {
            inner.phase = Phase::Idle;
        }
        let st = snapshot(&inner);
        self.shared.sink.state(&st);
        st
    }

    // ───────────────────────────── start ─────────────────────────────

    pub async fn start(&self, params: StartParams) -> CmdResult<RecorderState> {
        {
            let mut inner = self.shared.inner.lock().await;
            if !matches!(inner.phase, Phase::Idle | Phase::Countdown) || inner.transition {
                return Err(busy("a recording is already in progress"));
            }
            inner.phase = Phase::Starting;
            inner.transition = true;
        }
        match self.start_inner(params).await {
            Ok(st) => Ok(st),
            Err(e) => {
                let mut inner = self.shared.inner.lock().await;
                if let Some(mut s) = inner.session.take() {
                    if let Some(m) = s.monitor.take() {
                        m.abort();
                    }
                    let _ = tokio::fs::remove_dir_all(&s.params.work_dir).await;
                }
                inner.phase = Phase::Idle;
                inner.transition = false;
                let st = snapshot(&inner);
                self.shared.sink.state(&st);
                Err(e)
            }
        }
    }

    async fn start_inner(&self, params: StartParams) -> CmdResult<RecorderState> {
        tokio::fs::create_dir_all(&params.work_dir).await?;
        let camera = params.plan.camera.is_some();
        let manifest = recovery::new_manifest(&params.session_id, &params.meeting_name, params.final_output.clone(), params.work_dir.clone(), params.plan.clone(), Utc::now());
        let (first_tx, first_rx) = oneshot::channel();
        {
            let mut inner = self.shared.inner.lock().await;
            inner.session = Some(Session {
                params,
                manifest,
                live: None,
                first_progress: Some(first_tx),
                seg_started: None,
                elapsed_before_ms: 0,
                closed_bytes: 0,
                live_bytes: 0,
                camera_connected: camera,
                warnings: Vec::new(),
                last_emit: Instant::now(),
                last_error_tail: Vec::new(),
                monitor: None,
                helper_child: None,
            });
            self.spawn_next_segment(&mut inner).await?;
        }
        self.wait_first_progress(first_rx).await?;
        let mut inner = self.shared.inner.lock().await;
        inner.phase = Phase::Recording;
        inner.transition = false;
        self.start_monitor(&mut inner);
        let st = snapshot(&inner);
        self.shared.sink.state(&st);
        Ok(st)
    }

    async fn wait_first_progress(&self, rx: oneshot::Receiver<()>) -> CmdResult<()> {
        match tokio::time::timeout(START_TIMEOUT, rx).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => {
                let tail = self.shared.inner.lock().await.session.as_ref().map(|s| s.last_error_tail.join("\n")).unwrap_or_default();
                Err(CommandError::new("error.start", format!("ffmpeg exited before recording started:\n{tail}")))
            }
            Err(_) => {
                self.kill_live().await;
                Err(CommandError::new("error.start", "ffmpeg produced no frames within 12 seconds"))
            }
        }
    }

    /// Open a new segment in the manifest, write it, and launch ffmpeg for it. Caller holds the lock.
    async fn spawn_next_segment(&self, inner: &mut Inner) -> CmdResult<()> {
        let s = inner.session.as_mut().ok_or_else(|| busy("no session"))?;
        let path = recovery::open_segment(&mut s.manifest, Utc::now());
        let index = s.manifest.segments.last().map_or(0, |g| g.index);
        write_manifest(&s.manifest).await?;

        let mut plan = s.params.plan.clone();
        plan.output = path;
        if !s.camera_connected {
            plan.camera = None;
            plan.pip = None;
        }

        // The ScreenCaptureKit helper feeds a FIFO that ffmpeg reads; one pair per segment.
        if let (Some(helper), Some(InputSource::PcmPipe { path: _, sample_rate, channels })) = (s.params.helper.clone(), plan.system_audio.clone()) {
            let fifo = s.params.work_dir.join(format!("sys-{index:03}.pcm"));
            let _ = tokio::fs::remove_file(&fifo).await;
            let made = Command::new("mkfifo").arg(&fifo).status().await.map(|st| st.success()).unwrap_or(false);
            if made {
                plan.system_audio = Some(InputSource::PcmPipe { path: fifo.to_string_lossy().into_owned(), sample_rate, channels });
                let child = Command::new(&helper).arg("--out").arg(&fifo).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).spawn();
                s.helper_child = child.ok();
            } else {
                plan.system_audio = None;
                s.warnings.push(Warning { code: "warn.systemAudioOff".into(), detail: "could not create the audio FIFO".into() });
            }
        }

        let mut cmd = command::recording_command(&s.params.tools.ffmpeg_str(), &plan);
        if has_lavfi(&plan) {
            command::realtime_lavfi(&mut cmd);
        }
        let mut c = Command::new(&cmd.program);
        c.args(&cmd.args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        #[cfg(windows)]
        c.creation_flags(0x0800_0000);
        let mut child = c.spawn().map_err(|e| CommandError::new("error.noFfmpeg", format!("cannot start ffmpeg: {e}")))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let stdin = child.stdin.take();
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let (done_tx, done_rx) = oneshot::channel::<SegmentOutcome>();
        s.live = Some(Live { index, stop_tx: Some(stop_tx), done_rx, started: Instant::now() });
        s.seg_started = Some(Instant::now());
        s.live_bytes = 0;

        self.supervise(child, stdout, stderr, stdin, stop_rx, done_tx, index);
        Ok(())
    }

    /// Watch one ffmpeg process. A plain (non-async) fn on purpose: the spawned task calls back
    /// into `on_unexpected_exit` → `spawn_next_segment`, and keeping the `tokio::spawn` out of an
    /// async fn body avoids a cyclic `Send` proof between those futures.
    #[allow(clippy::too_many_arguments)]
    fn supervise(
        &self,
        mut child: Child,
        stdout: Option<tokio::process::ChildStdout>,
        stderr: Option<tokio::process::ChildStderr>,
        mut stdin: Option<tokio::process::ChildStdin>,
        mut stop_rx: oneshot::Receiver<()>,
        done_tx: oneshot::Sender<SegmentOutcome>,
        index: u32,
    ) {
        let rec = self.clone();
        tokio::spawn(async move {
            let rec_out = rec.clone();
            let out_task = tokio::spawn(async move {
                let Some(mut out) = stdout else { return };
                let mut parser = ProgressParser::new();
                let mut buf = [0u8; 4096];
                while let Ok(n) = out.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    for p in parser.feed(&String::from_utf8_lossy(&buf[..n])) {
                        rec_out.on_progress(p).await;
                    }
                }
            });
            let rec_err = rec.clone();
            let err_task = tokio::spawn(async move {
                let mut tail: Vec<String> = Vec::new();
                let mut classes: Vec<&'static str> = Vec::new();
                if let Some(e) = stderr {
                    let mut lines = BufReader::new(e).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if let Some(code) = classify_stderr_line(&line) {
                            if !classes.contains(&code) {
                                classes.push(code);
                                if code.starts_with("warn.") && code != "warn.cameraLost" {
                                    rec_err.push_warning(code, &line).await;
                                }
                            }
                        }
                        tail.push(line);
                        if tail.len() > 30 {
                            tail.remove(0);
                        }
                    }
                }
                (tail, classes)
            });

            let mut requested = false;
            let status = tokio::select! {
                st = child.wait() => st,
                _ = &mut stop_rx => {
                    requested = true;
                    if let Some(mut si) = stdin.take() {
                        let _ = si.write_all(b"q").await;
                        let _ = si.flush().await;
                        drop(si);
                    }
                    match tokio::time::timeout(STOP_GRACE, child.wait()).await {
                        Ok(st) => st,
                        Err(_) => {
                            let _ = child.kill().await;
                            child.wait().await
                        }
                    }
                }
            };
            let _ = out_task.await;
            let (tail, classes) = err_task.await.unwrap_or_default();
            let outcome = SegmentOutcome { success: status.as_ref().is_ok_and(|s| s.success()), code: status.ok().and_then(|s| s.code()), stderr_tail: tail, classes };
            if requested {
                let _ = done_tx.send(outcome);
            } else {
                rec.on_unexpected_exit(index, outcome).await;
            }
        });
    }

    async fn on_progress(&self, p: Progress) {
        let mut inner = self.shared.inner.lock().await;
        let Some(s) = inner.session.as_mut() else { return };
        s.live_bytes = p.total_size;
        if let Some(tx) = s.first_progress.take() {
            let _ = tx.send(());
        }
        if s.last_emit.elapsed() >= EMIT_EVERY {
            s.last_emit = Instant::now();
            let st = snapshot(&inner);
            self.shared.sink.progress(&p);
            self.shared.sink.state(&st);
        }
    }

    async fn push_warning(&self, code: &str, detail: &str) {
        let mut inner = self.shared.inner.lock().await;
        let Some(s) = inner.session.as_mut() else { return };
        if s.warnings.iter().any(|w| w.code == code) {
            return;
        }
        let w = Warning { code: code.to_string(), detail: detail.to_string() };
        s.warnings.push(w.clone());
        self.shared.sink.warning(&w);
        let st = snapshot(&inner);
        self.shared.sink.state(&st);
    }

    // ─────────────────── closing segments, pause, resume ───────────────────

    /// Ask the live segment to stop and wait for it. Returns its outcome.
    async fn stop_live(live: Live) -> SegmentOutcome {
        let Live { stop_tx, done_rx, .. } = live;
        if let Some(tx) = stop_tx {
            let _ = tx.send(());
        }
        match tokio::time::timeout(STOP_GRACE + Duration::from_secs(4), done_rx).await {
            Ok(Ok(o)) => o,
            _ => SegmentOutcome::default(),
        }
    }

    async fn kill_live(&self) {
        let live = {
            let mut inner = self.shared.inner.lock().await;
            inner.session.as_mut().and_then(|s| s.live.take())
        };
        if let Some(l) = live {
            let _ = Self::stop_live(l).await;
        }
    }

    /// Record the finished segment in the manifest (bytes from disk, duration from ffprobe).
    async fn close_current(&self, index: u32) {
        let (path, tools) = {
            let inner = self.shared.inner.lock().await;
            let Some(s) = inner.session.as_ref() else { return };
            let Some(seg) = s.manifest.segments.iter().find(|g| g.index == index) else { return };
            (seg.path.clone(), s.params.tools.clone())
        };
        let bytes = tokio::fs::metadata(&path).await.map(|m| m.len()).ok();
        let duration = if bytes.unwrap_or(0) > 0 { sidecar::probe(&tools, &path).await.ok().map(|i| i.duration_ms) } else { None };
        let mut inner = self.shared.inner.lock().await;
        let Some(s) = inner.session.as_mut() else { return };
        recovery::close_segment(&mut s.manifest, Utc::now(), duration, bytes);
        s.closed_bytes += bytes.unwrap_or(0);
        s.live_bytes = 0;
        if let Some(t) = s.seg_started.take() {
            s.elapsed_before_ms += u64::try_from(t.elapsed().as_millis()).unwrap_or(0);
        }
        if let Some(mut h) = s.helper_child.take() {
            let _ = h.start_kill();
        }
        let _ = write_manifest(&s.manifest).await;
    }

    pub async fn pause(&self) -> CmdResult<RecorderState> {
        let live = {
            let mut inner = self.shared.inner.lock().await;
            if inner.phase != Phase::Recording || inner.transition {
                return Err(busy("pause is only possible while recording"));
            }
            inner.transition = true;
            inner.session.as_mut().and_then(|s| s.live.take())
        };
        let index = if let Some(live) = live {
            let index = live.index;
            let _ = Self::stop_live(live).await;
            Some(index)
        } else {
            None
        };
        if let Some(i) = index {
            self.close_current(i).await;
        }
        let mut inner = self.shared.inner.lock().await;
        inner.phase = Phase::Paused;
        inner.transition = false;
        if let Some(s) = inner.session.as_mut() {
            s.manifest.status = SessionStatus::Paused;
            let _ = write_manifest(&s.manifest).await;
        }
        let st = snapshot(&inner);
        self.shared.sink.state(&st);
        Ok(st)
    }

    pub async fn resume(&self) -> CmdResult<RecorderState> {
        let first_rx = {
            let mut inner = self.shared.inner.lock().await;
            if inner.phase != Phase::Paused || inner.transition {
                return Err(busy("resume is only possible while paused"));
            }
            inner.transition = true;
            let (tx, rx) = oneshot::channel();
            if let Some(s) = inner.session.as_mut() {
                s.first_progress = Some(tx);
                s.manifest.status = SessionStatus::Recording;
            }
            if let Err(e) = self.spawn_next_segment(&mut inner).await {
                inner.transition = false;
                return Err(e);
            }
            rx
        };
        let waited = self.wait_first_progress(first_rx).await;
        let mut inner = self.shared.inner.lock().await;
        inner.transition = false;
        match waited {
            Ok(()) => {
                inner.phase = Phase::Recording;
                let st = snapshot(&inner);
                self.shared.sink.state(&st);
                Ok(st)
            }
            Err(e) => {
                // Stay paused: what was recorded so far is safe in the manifest.
                if let Some(s) = inner.session.as_mut() {
                    s.live = None;
                    s.manifest.status = SessionStatus::Paused;
                }
                let st = snapshot(&inner);
                self.shared.sink.state(&st);
                Err(e)
            }
        }
    }

    // ───────────────────────────── unexpected exit ─────────────────────────────

    async fn on_unexpected_exit(&self, index: u32, outcome: SegmentOutcome) {
        let (starting, camera, quick) = {
            let mut inner = self.shared.inner.lock().await;
            let phase = inner.phase;
            let Some(s) = inner.session.as_mut() else { return };
            if s.live.as_ref().map(|l| l.index) != Some(index) {
                return; // a stale segment from before a restart
            }
            let quick = s.live.as_ref().is_some_and(|l| l.started.elapsed() < QUICK_FAIL);
            s.last_error_tail = outcome.stderr_tail.clone();
            s.live = None;
            if let Some(tx) = s.first_progress.take() {
                drop(tx); // wakes start()/resume() with an error
            }
            (phase == Phase::Starting || (inner.transition && phase != Phase::Recording), s_camera(&inner), quick)
        };
        if starting {
            self.close_current(index).await;
            return;
        }
        self.close_current(index).await;

        if outcome.success {
            // ffmpeg finished by itself (a `-t` limit or an input that ended): not an error.
            self.auto_finish().await;
            return;
        }
        if outcome.classes.contains(&"error.diskFull") {
            self.shared.sink.error("error.diskFull", "the disk is full; the recording so far is being saved");
            self.auto_finish().await;
            return;
        }
        if camera && !quick {
            // Most likely the camera was unplugged: carry on without it.
            let w = Warning { code: "warn.cameraLost".into(), detail: outcome.stderr_tail.last().cloned().unwrap_or_default() };
            let mut inner = self.shared.inner.lock().await;
            if let Some(s) = inner.session.as_mut() {
                s.camera_connected = false;
                s.warnings.push(w.clone());
            }
            self.shared.sink.warning(&w);
            let (tx, _rx) = oneshot::channel();
            if let Some(s) = inner.session.as_mut() {
                s.first_progress = Some(tx);
            }
            match self.spawn_next_segment(&mut inner).await {
                Ok(()) => {
                    let st = snapshot(&inner);
                    self.shared.sink.state(&st);
                    return;
                }
                Err(e) => {
                    drop(inner);
                    self.shared.sink.error(&e.code, &e.detail);
                }
            }
        } else {
            let detail = format!("ffmpeg exited with code {:?}: {}", outcome.code, outcome.stderr_tail.last().cloned().unwrap_or_default());
            self.shared.sink.error("error.ffmpegExit", &detail);
        }
        self.auto_finish().await;
    }

    async fn auto_finish(&self) {
        let rec = self.clone();
        tokio::spawn(async move {
            // Whatever was recorded is concatenated and announced through the `finished` event.
            if let Err(e) = rec.finish(false).await {
                rec.shared.sink.error(&e.code, &e.detail);
            }
        });
    }

    // ───────────────────────────── monitor ─────────────────────────────

    fn start_monitor(&self, inner: &mut Inner) {
        let rec = self.clone();
        let Some(s) = inner.session.as_mut() else { return };
        let dir = s.params.work_dir.clone();
        let low_gb = u64::from(s.params.low_disk_warn_gb);
        s.monitor = Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(MONITOR_EVERY).await;
                if let Ok((free, _)) = crate::disk::space(&dir) {
                    if free < HARD_FLOOR_BYTES {
                        rec.shared.sink.error("error.diskFull", "less than 300 MB free; finishing the recording now");
                        rec.auto_finish().await;
                        return;
                    }
                    if free < low_gb * disk_gib() {
                        rec.push_warning("warn.lowDisk", &format!("{} MB free", free / 1_000_000)).await;
                    }
                }
                let (over, phase) = {
                    let inner = rec.shared.inner.lock().await;
                    let st = snapshot(&inner);
                    let over = st.elapsed_ms > 120_000 && st.bytes_written as f64 > st.estimated_total_bytes as f64 * 1.25;
                    (over, inner.phase)
                };
                if over && phase == Phase::Recording {
                    rec.push_warning("warn.overBudget", "the file is growing faster than the size budget").await;
                }
            }
        }));
    }

    // ───────────────────────────── stop / cancel ─────────────────────────────

    /// Finish the session: stop the live segment, remux or concat, probe, clean up.
    pub async fn stop(&self) -> CmdResult<RecordingResult> {
        self.finish(true).await
    }

    async fn finish(&self, user_initiated: bool) -> CmdResult<RecordingResult> {
        let live = {
            let mut inner = self.shared.inner.lock().await;
            if !matches!(inner.phase, Phase::Recording | Phase::Paused) || inner.transition && user_initiated {
                return Err(busy("nothing to stop"));
            }
            if inner.phase == Phase::Finalizing {
                return Err(busy("already finishing"));
            }
            inner.transition = true;
            inner.phase = Phase::Finalizing;
            let st = snapshot(&inner);
            self.shared.sink.state(&st);
            if let Some(s) = inner.session.as_mut() {
                if let Some(m) = s.monitor.take() {
                    m.abort();
                }
                s.manifest.status = SessionStatus::Finalizing;
            }
            inner.session.as_mut().and_then(|s| s.live.take())
        };
        if let Some(live) = live {
            let index = live.index;
            let _ = Self::stop_live(live).await;
            self.close_current(index).await;
        }

        let (manifest, tools, target_mb, recovered_budget) = {
            let mut inner = self.shared.inner.lock().await;
            let s = inner.session.as_mut().ok_or_else(|| busy("no session"))?;
            let _ = write_manifest(&s.manifest).await;
            (s.manifest.clone(), s.params.tools.clone(), s.params.target_mb_per_hour, s.params.plan.budget.clone())
        };

        let result = assemble(&manifest, &tools, target_mb, recovered_budget, false).await;
        let mut inner = self.shared.inner.lock().await;
        match result {
            Ok(res) => {
                let work_dir = manifest.work_dir.clone();
                inner.session = None;
                inner.phase = Phase::Idle;
                inner.transition = false;
                let _ = tokio::fs::remove_dir_all(work_dir).await;
                let st = snapshot(&inner);
                self.shared.sink.state(&st);
                self.shared.sink.finished(&res);
                Ok(res)
            }
            Err(e) => {
                if let Some(s) = inner.session.as_mut() {
                    s.manifest.status = SessionStatus::Failed;
                    let _ = write_manifest(&s.manifest).await;
                }
                inner.session = None;
                inner.phase = Phase::Idle;
                inner.transition = false;
                let st = snapshot(&inner);
                self.shared.sink.state(&st);
                Err(e)
            }
        }
    }

    /// Stop and throw everything away.
    pub async fn cancel(&self) -> CmdResult<RecorderState> {
        let (live, work_dir) = {
            let mut inner = self.shared.inner.lock().await;
            if inner.phase == Phase::Countdown {
                inner.phase = Phase::Idle;
                let st = snapshot(&inner);
                self.shared.sink.state(&st);
                return Ok(st);
            }
            if !matches!(inner.phase, Phase::Recording | Phase::Paused | Phase::Starting) {
                return Err(busy("nothing to cancel"));
            }
            inner.transition = true;
            let Some(s) = inner.session.as_mut() else {
                inner.phase = Phase::Idle;
                inner.transition = false;
                return Ok(snapshot(&inner));
            };
            if let Some(m) = s.monitor.take() {
                m.abort();
            }
            (s.live.take(), s.params.work_dir.clone())
        };
        if let Some(l) = live {
            let _ = Self::stop_live(l).await;
        }
        let mut inner = self.shared.inner.lock().await;
        if let Some(mut s) = inner.session.take() {
            if let Some(mut h) = s.helper_child.take() {
                let _ = h.start_kill();
            }
        }
        let _ = tokio::fs::remove_dir_all(work_dir).await;
        inner.phase = Phase::Idle;
        inner.transition = false;
        let st = snapshot(&inner);
        self.shared.sink.state(&st);
        Ok(st)
    }
}

fn s_camera(inner: &Inner) -> bool {
    inner.session.as_ref().is_some_and(|s| s.camera_connected && s.params.plan.camera.is_some())
}

fn disk_gib() -> u64 {
    recorder_core::disk::GIB
}

fn has_lavfi(plan: &RecordingPlan) -> bool {
    let l = |s: &InputSource| matches!(s, InputSource::Lavfi { .. });
    l(&plan.screen) || plan.camera.as_ref().is_some_and(l) || plan.mic.as_ref().is_some_and(l) || plan.system_audio.as_ref().is_some_and(l)
}

/// Turn the segments of a manifest into the final file and describe it.
/// Used by stop and by crash recovery (`recovered = true`).
pub async fn assemble(manifest: &SessionManifest, tools: &Tools, target_mb_per_hour: u32, planned_budget: BitrateBudget, recovered: bool) -> CmdResult<RecordingResult> {
    let sizes = |p: &Path| std::fs::metadata(p).ok().map(|m| m.len());
    let segments = match recovery::decide(manifest, &sizes) {
        recovery::RecoveryAction::Concat { segments, .. } => segments,
        recovery::RecoveryAction::Discard => return Err(CommandError::new("error.recovery", "no recorded data to save")),
    };
    if let Some(parent) = manifest.final_output.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let ffmpeg = tools.ffmpeg_str();
    let cmd = if segments.len() == 1 {
        command::remux_command(&ffmpeg, &segments[0], &manifest.final_output)
    } else {
        let list = manifest.work_dir.join("concat.txt");
        tokio::fs::write(&list, recovery::concat_list(&segments)).await?;
        command::concat_command(&ffmpeg, &list, &manifest.final_output)
    };
    let out = sidecar::run(&cmd).await?;
    if !out.success {
        return Err(CommandError::new("error.ffmpegExit", format!("joining the segments failed: {}", out.stderr.trim())));
    }
    let info = sidecar::probe(tools, &manifest.final_output).await?;
    let ratio = budget::over_budget_ratio(info.bytes, info.duration_ms, target_mb_per_hour);
    Ok(RecordingResult { session_id: manifest.session_id.clone(), path: info.path.clone(), info, budget: planned_budget, over_budget_ratio: ratio, recovered })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    #[derive(Default)]
    struct Collect {
        warnings: StdMutex<Vec<String>>,
        errors: StdMutex<Vec<String>>,
        finished: StdMutex<Vec<RecordingResult>>,
        states: StdMutex<Vec<Phase>>,
    }
    impl EventSink for Collect {
        fn state(&self, s: &RecorderState) {
            self.states.lock().unwrap().push(s.phase);
        }
        fn progress(&self, _: &Progress) {}
        fn warning(&self, w: &Warning) {
            self.warnings.lock().unwrap().push(w.code.clone());
        }
        fn error(&self, code: &str, _: &str) {
            self.errors.lock().unwrap().push(code.to_string());
        }
        fn finished(&self, r: &RecordingResult) {
            self.finished.lock().unwrap().push(r.clone());
        }
    }

    use crate::selftest::lavfi_params;

    #[tokio::test]
    async fn start_pause_resume_stop_makes_one_playable_file() {
        let dir = tempfile::tempdir().unwrap();
        let Some(params) = lavfi_params(dir.path(), true) else {
            eprintln!("skipped: no ffmpeg");
            return;
        };
        let sink = Arc::new(Collect::default());
        let rec = Recorder::new(sink.clone());
        let st = rec.start(params).await.unwrap();
        assert_eq!(st.phase, Phase::Recording);
        tokio::time::sleep(Duration::from_millis(2200)).await;
        let st = rec.pause().await.unwrap();
        assert_eq!(st.phase, Phase::Paused);
        assert_eq!(st.segments, 1);
        assert!(rec.pause().await.is_err(), "pause while paused is refused");
        tokio::time::sleep(Duration::from_millis(500)).await;
        let st = rec.resume().await.unwrap();
        assert_eq!(st.phase, Phase::Recording);
        assert_eq!(st.segments, 2);
        tokio::time::sleep(Duration::from_millis(2200)).await;
        let res = rec.stop().await.unwrap();

        assert!(Path::new(&res.path).is_file());
        assert_eq!((res.info.width, res.info.height), (1920, 1080));
        // Each segment also contains ffmpeg's start-up lag (up to ~1.5 s before the first progress
        // block), so two segments of 2.2 s of sleeping land between 4.4 s and 9 s; the paused 0.5 s is absent.
        assert!((4_400..=9_000).contains(&res.info.duration_ms), "duration {}", res.info.duration_ms);
        assert_eq!(res.info.audio_tracks, 1);
        assert!(!dir.path().join("work").exists(), "work dir is removed after success");
        assert_eq!(rec.phase().await, Phase::Idle);
        let phases = sink.states.lock().unwrap().clone();
        assert!(phases.contains(&Phase::Paused) && phases.contains(&Phase::Finalizing));
        assert_eq!(sink.finished.lock().unwrap().len(), 1);
        assert!(sink.errors.lock().unwrap().is_empty(), "{:?}", sink.errors.lock().unwrap());
    }

    #[tokio::test]
    async fn manifest_tracks_segments_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let Some(params) = lavfi_params(dir.path(), false) else { return };
        let rec = Recorder::new(Arc::new(NullSink));
        rec.start(params).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1500)).await;
        // While recording, the manifest already lists the open segment (crash safety).
        let json = std::fs::read_to_string(dir.path().join("work/manifest.json")).unwrap();
        let m = recovery::manifest_from_json(&json).unwrap();
        assert_eq!(m.segments.len(), 1);
        assert_eq!(m.status, SessionStatus::Recording);
        assert!(dir.path().join("work/seg-000.mp4").is_file());
        rec.cancel().await.unwrap();
        assert!(!dir.path().join("work").exists(), "cancel deletes the work dir");
    }

    #[tokio::test]
    async fn killed_ffmpeg_with_camera_continues_without_it() {
        let dir = tempfile::tempdir().unwrap();
        let Some(params) = lavfi_params(dir.path(), true) else { return };
        let sink = Arc::new(Collect::default());
        let rec = Recorder::new(sink.clone());
        rec.start(params).await.unwrap();
        tokio::time::sleep(Duration::from_millis(4000)).await; // past the quick-failure window
        // Simulate the camera vanishing: ffmpeg dies by itself.
        let pids = std::process::Command::new("pgrep").args(["-f", "work/seg-000.mp4"]).output().unwrap();
        let pid = String::from_utf8_lossy(&pids.stdout).lines().next().unwrap_or("").trim().to_string();
        assert!(!pid.is_empty(), "no ffmpeg process found");
        std::process::Command::new("kill").args(["-9", &pid]).status().unwrap();
        tokio::time::sleep(Duration::from_millis(3500)).await;
        let st = rec.state().await;
        assert_eq!(st.phase, Phase::Recording);
        assert!(!st.camera_connected);
        assert_eq!(st.segments, 2);
        assert!(sink.warnings.lock().unwrap().contains(&"warn.cameraLost".to_string()));
        let res = rec.stop().await.unwrap();
        assert!(res.info.duration_ms >= 4_000, "{}", res.info.duration_ms);
    }

    #[tokio::test]
    async fn bad_input_fails_start_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let Some(mut params) = lavfi_params(dir.path(), false) else { return };
        params.plan.screen = InputSource::Lavfi { graph: "nosuchsource=1".into() };
        let rec = Recorder::new(Arc::new(NullSink));
        let err = rec.start(params).await.unwrap_err();
        assert_eq!(err.code, "error.start");
        assert_eq!(rec.phase().await, Phase::Idle);
        assert!(!dir.path().join("work").exists());
    }

    #[tokio::test]
    async fn busy_guards() {
        let rec = Recorder::new(Arc::new(NullSink));
        assert!(rec.pause().await.is_err());
        assert!(rec.resume().await.is_err());
        assert!(rec.stop().await.is_err());
        assert!(rec.cancel().await.is_err());
        let st = rec.begin_countdown().await.unwrap();
        assert_eq!(st.phase, Phase::Countdown);
        assert!(rec.begin_countdown().await.is_err());
        assert_eq!(rec.abort_countdown().await.phase, Phase::Idle);
    }
}
