//! Parse `ffmpeg -progress pipe:1 -nostats` output. One block per update, keys like
//! frame=123\nfps=30.0\nout_time_us=…\ntotal_size=…\ndrop_frames=…\nprogress=continue
use crate::types::Progress;

/// Incremental parser: feed chunks, get completed Progress blocks.
#[derive(Debug, Default)]
pub struct ProgressParser {
    buf: String,
    current: Progress,
}

impl ProgressParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed raw bytes/text; returns every block completed by this chunk (ending in `progress=…`).
    /// A partial trailing line is kept until its newline arrives; CRLF is accepted.
    pub fn feed(&mut self, chunk: &str) -> Vec<Progress> {
        self.buf.push_str(chunk);
        let mut done = Vec::new();
        while let Some(pos) = self.buf.find('\n') {
            let line: String = self.buf.drain(..=pos).collect();
            if apply_line(&mut self.current, line.trim_end_matches(['\n', '\r'])) {
                done.push(std::mem::take(&mut self.current));
            }
        }
        done
    }
}

/// Apply one `key=value` line. Returns true when it was the `progress=` terminator.
fn apply_line(p: &mut Progress, line: &str) -> bool {
    let Some((key, value)) = line.split_once('=') else {
        return false;
    };
    let value = value.trim();
    let na = value.eq_ignore_ascii_case("n/a");
    match key.trim() {
        "frame" => p.frame = value.parse().unwrap_or(p.frame),
        "fps" => p.fps = value.parse().unwrap_or(0.0),
        "bitrate" => {
            p.bitrate_kbps = if na { 0.0 } else { value.trim_end_matches("kbits/s").trim().parse().unwrap_or(0.0) }
        }
        "total_size" => p.total_size = value.parse().unwrap_or(p.total_size),
        // `out_time_ms` is (historically) also microseconds in ffmpeg, so only `out_time_us` is used.
        "out_time_us" => {
            if let Ok(us) = value.parse::<i64>() {
                p.out_time_ms = u64::try_from(us.max(0)).unwrap_or(0) / 1000;
            }
        }
        "out_time" => {
            if p.out_time_ms == 0 {
                if let Some(ms) = parse_clock(value) {
                    p.out_time_ms = ms;
                }
            }
        }
        "drop_frames" => p.drop_frames = value.parse().unwrap_or(p.drop_frames),
        "dup_frames" => p.dup_frames = value.parse().unwrap_or(p.dup_frames),
        "speed" => p.speed = if na { 0.0 } else { value.trim_end_matches('x').trim().parse().unwrap_or(0.0) },
        "progress" => {
            p.finished = value == "end";
            return true;
        }
        _ => {}
    }
    false
}

/// "00:01:02.500000" → milliseconds.
fn parse_clock(s: &str) -> Option<u64> {
    let mut parts = s.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let sec: f64 = parts.next()?.parse().ok()?;
    Some(((h * 3600.0 + m * 60.0 + sec) * 1000.0).round().max(0.0) as u64)
}

/// Parse a single complete block (used by tests and by feed).
pub fn parse_block(block: &str) -> Progress {
    let mut p = Progress::default();
    for line in block.lines() {
        apply_line(&mut p, line.trim_end_matches('\r'));
    }
    p
}

/// Parse ffmpeg stderr lines for events we care about: camera disappearing, device errors.
/// Returns an i18n warning code when a line matters, e.g. "warn.cameraLost".
pub fn classify_stderr_line(line: &str) -> Option<&'static str> {
    let l = line.to_ascii_lowercase();
    if l.contains("no space left on device") {
        return Some("error.diskFull");
    }
    if l.contains("disconnected") || l.contains("device has been removed") || l.contains("device not found") || l.contains("could not lock device") {
        // Audio devices are named in the line when ffmpeg knows; otherwise treat as camera.
        if l.contains("audio") || l.contains("microphone") {
            return Some("warn.audioDeviceLost");
        }
        return Some("warn.cameraLost");
    }
    if l.contains("input/output error") && (l.contains("av_read_frame") || l.contains("avfoundation") || l.contains("dshow")) {
        return Some("warn.cameraLost");
    }
    if l.contains("too many packets buffered") || l.contains("thread message queue blocking") || l.contains("dropping frame") || l.contains("frames dropped") {
        return Some("warn.frameDrop");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const BLOCK: &str = "frame=120\nfps=29.97\nstream_0_0_q=-0.0\nbitrate= 812.4kbits/s\ntotal_size=409600\nout_time_us=4004000\nout_time_ms=4004000\nout_time=00:00:04.004000\ndup_frames=1\ndrop_frames=2\nspeed=1.01x\nprogress=continue\n";

    #[test]
    fn parses_a_whole_block() {
        let p = parse_block(BLOCK);
        assert_eq!(p.frame, 120);
        assert!((p.fps - 29.97).abs() < 1e-9);
        assert!((p.bitrate_kbps - 812.4).abs() < 1e-9);
        assert_eq!(p.total_size, 409_600);
        assert_eq!(p.out_time_ms, 4004);
        assert_eq!(p.dup_frames, 1);
        assert_eq!(p.drop_frames, 2);
        assert!((p.speed - 1.01).abs() < 1e-9);
        assert!(!p.finished);
    }

    #[test]
    fn feed_handles_split_chunks_and_crlf() {
        let mut parser = ProgressParser::new();
        let crlf = BLOCK.replace('\n', "\r\n");
        let (a, b) = crlf.split_at(37);
        assert!(parser.feed(a).is_empty());
        let out = parser.feed(b);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].frame, 120);
        assert_eq!(out[0].total_size, 409_600);
    }

    #[test]
    fn feed_returns_several_blocks_and_marks_end() {
        let mut parser = ProgressParser::new();
        let two = format!("{BLOCK}frame=150\nprogress=end\n");
        let out = parser.feed(&two);
        assert_eq!(out.len(), 2);
        assert!(!out[0].finished);
        assert!(out[1].finished);
        assert_eq!(out[1].frame, 150);
        // State is reset between blocks: block two has no total_size of its own.
        assert_eq!(out[1].total_size, 0);
    }

    #[test]
    fn na_values_become_zero() {
        let p = parse_block("bitrate=N/A\nspeed=N/A\nout_time_us=N/A\ntotal_size=N/A\nprogress=continue\n");
        assert_eq!(p.bitrate_kbps, 0.0);
        assert_eq!(p.speed, 0.0);
        assert_eq!(p.out_time_ms, 0);
        assert_eq!(p.total_size, 0);
    }

    #[test]
    fn out_time_clock_is_a_fallback() {
        let p = parse_block("out_time=00:01:02.500000\nprogress=continue\n");
        assert_eq!(p.out_time_ms, 62_500);
    }

    #[test]
    fn negative_time_clamps_to_zero() {
        let p = parse_block("out_time_us=-9223372036854775807\nprogress=continue\n");
        assert_eq!(p.out_time_ms, 0);
    }

    #[test]
    fn stderr_classification() {
        assert_eq!(classify_stderr_line("[avfoundation @ 0x1] Input/output error av_read_frame"), Some("warn.cameraLost"));
        assert_eq!(classify_stderr_line("Video device was disconnected"), Some("warn.cameraLost"));
        assert_eq!(classify_stderr_line("Audio device disconnected"), Some("warn.audioDeviceLost"));
        assert_eq!(classify_stderr_line("av_interleaved_write_frame(): No space left on device"), Some("error.diskFull"));
        assert_eq!(classify_stderr_line("Thread message queue blocking; consider raising the thread_queue_size option"), Some("warn.frameDrop"));
        assert_eq!(classify_stderr_line("frame=  120 fps= 30 q=-0.0 size=    1024kB"), None);
        assert_eq!(classify_stderr_line(""), None);
    }
}
