//! Scrolling screenshot. The user drags a rectangle over a still of the screen, then scrolls the
//! page by hand; the frames of that rectangle are joined into one tall picture by
//! `recorder_core::stitch` and saved as a PNG.
//!
//! The shell's part is the session: the windows (selection, frame, control bar), the frame
//! source, the events, the file. The frame source is the only platform-specific piece: on macOS
//! the `sck-record --scroll` helper; other platforms report `scroll_capture: false` until they
//! have one.

use crate::error::{CmdResult, CommandError};
use recorder_core::stitch::{Push, StitchOptions, Stitcher};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// The rectangle the user dragged, in logical points from the top-left corner of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicalRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrollProgress {
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    /// The picture so far, 96 px wide, as a PNG data URL (the last part when it is very tall).
    pub thumbnail: Option<String>,
    /// Row of `thumbnail`, 0 to 1, where the newest frame starts.
    pub newest_at: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrollShot {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub copied: bool,
}

/// What the selection window needs to draw itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrollBackdrop {
    pub path: String,
}

struct Selecting {
    screen: usize,
    backdrop: PathBuf,
}

struct Capturing {
    child: Child,
    stdin: Option<ChildStdin>,
    stitcher: Arc<Mutex<Stitcher>>,
    stopped: Arc<AtomicBool>,
    reader: Option<std::thread::JoinHandle<()>>,
    scale: f64,
}

enum Session {
    Selecting(Selecting),
    Capturing(Capturing),
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

/// A scrolling screenshot is being chosen or taken; a recording must not start meanwhile.
pub fn active() -> bool {
    SESSION.lock().expect("scroll lock").is_some()
}

/// Smallest rectangle worth stitching, in pixels.
const MIN_SIDE_PX: u32 = 64;
const THUMB_WIDTH: u32 = 96;
const THUMB_MAX_ROWS: u32 = 1400;

/// Whether this build and system can take a scrolling screenshot at all.
pub fn supported() -> bool {
    platform::supported()
}

/// Step 1: hide the main window, take a still of its screen and show the selection window on it.
pub async fn begin(app: &AppHandle) -> CmdResult<()> {
    if !supported() {
        return Err(CommandError::new(
            "error.unsupported",
            "scrolling screenshots are not available on this system",
        ));
    }
    if active() {
        return Err(CommandError::busy("a scrolling screenshot is in progress"));
    }
    let screen = crate::windows::screen_of_main(app);
    crate::windows::hide_main(app);
    // The window fades out; a still taken at once would show it.
    tokio::time::sleep(Duration::from_millis(350)).await;
    let backdrop = match platform::still(screen).await {
        Ok(p) => p,
        Err(e) => {
            crate::windows::show_main(app);
            return Err(e);
        }
    };
    *SESSION.lock().expect("scroll lock") =
        Some(Session::Selecting(Selecting { screen, backdrop }));
    if let Err(e) = crate::windows::open_scroll_select(app, screen) {
        cancel(app);
        return Err(e.into());
    }
    Ok(())
}

/// The still for the selection window.
pub fn backdrop() -> CmdResult<ScrollBackdrop> {
    match SESSION.lock().expect("scroll lock").as_ref() {
        Some(Session::Selecting(s)) => Ok(ScrollBackdrop {
            path: s.backdrop.to_string_lossy().into_owned(),
        }),
        _ => Err(CommandError::busy("no selection is open")),
    }
}

/// Step 2: the rectangle is chosen. Close the selection, show the frame and the bar, start
/// reading frames.
pub fn start(app: &AppHandle, rect: LogicalRect) -> CmdResult<()> {
    let selecting = match SESSION.lock().expect("scroll lock").take() {
        Some(Session::Selecting(s)) => s,
        other => {
            *SESSION.lock().expect("scroll lock") = other;
            return Err(CommandError::busy("no selection is open"));
        }
    };
    let _ = std::fs::remove_file(&selecting.backdrop);
    crate::windows::close_scroll_select(app);

    let started = (|| -> CmdResult<Capturing> {
        let display = platform::display(selecting.screen)?;
        let crop = pixel_rect(rect, display.scale, display.width, display.height)?;
        let (mut child, stdin, stdout) = platform::frames(&display, crop)?;
        let stitcher = Arc::new(Mutex::new(Stitcher::new(
            crop.2,
            crop.3,
            StitchOptions::default(),
        )));
        let stopped = Arc::new(AtomicBool::new(false));
        let reader = {
            let (app, stitcher, stopped) = (app.clone(), stitcher.clone(), stopped.clone());
            std::thread::Builder::new()
                .name("scroll-frames".into())
                .spawn(move || read_frames(app, stdout, stitcher, stopped))
        };
        let reader = match reader {
            Ok(r) => r,
            Err(e) => {
                let _ = child.kill();
                return Err(CommandError::new("error.internal", e.to_string()));
            }
        };
        Ok(Capturing {
            child,
            stdin: Some(stdin),
            stitcher,
            stopped,
            reader: Some(reader),
            scale: display.scale,
        })
    })();
    match started {
        Ok(c) => {
            *SESSION.lock().expect("scroll lock") = Some(Session::Capturing(c));
            if let Err(e) = crate::windows::open_scroll_frame(app, selecting.screen, rect) {
                cancel(app);
                return Err(e.into());
            }
            Ok(())
        }
        Err(e) => {
            crate::windows::show_main(app);
            Err(e)
        }
    }
}

/// Step 3: stop reading, save the picture, put it on the clipboard, bring the main window back.
pub fn finish(app: &AppHandle) -> CmdResult<ScrollShot> {
    let capturing = match SESSION.lock().expect("scroll lock").take() {
        Some(Session::Capturing(c)) => c,
        other => {
            *SESSION.lock().expect("scroll lock") = other;
            return Err(CommandError::busy("no scrolling screenshot is in progress"));
        }
    };
    let scale = capturing.scale;
    let stitcher = stop_capture(capturing);
    crate::windows::close_scroll_frame(app);
    let result = (|| -> CmdResult<ScrollShot> {
        let stitcher = Arc::try_unwrap(stitcher)
            .map_err(|_| CommandError::new("error.internal", "the frame reader is still running"))?
            .into_inner()
            .expect("stitcher lock");
        let (width, height, bgra) = stitcher.into_pixels();
        if height == 0 {
            return Err(CommandError::new(
                "error.scrollEmpty",
                "no frame arrived; nothing was saved",
            ));
        }
        let png = encode_png(width, height, &bgra, scale)?;
        let settings = crate::commands::load_settings(app)?;
        let dir = crate::commands::effective_output_dir(&settings).join("Screenshots");
        std::fs::create_dir_all(&dir)?;
        let stem = recorder_core::filename::build_stem(
            &settings.filename_template,
            "",
            "Screenshot",
            chrono::Local::now(),
        );
        let path = recorder_core::filename::unique_path(&dir, &stem, "png", &|p| p.exists());
        std::fs::write(&path, &png)?;
        let copied = platform::copy_png(app, png);
        Ok(ScrollShot {
            path: path.to_string_lossy().into_owned(),
            width,
            height,
            copied,
        })
    })();
    crate::windows::show_main(app);
    if let Ok(shot) = &result {
        let _ = app.emit("scroll://finished", shot.clone());
    }
    result
}

/// Drop whatever is going on and bring the main window back.
pub fn cancel(app: &AppHandle) {
    match SESSION.lock().expect("scroll lock").take() {
        Some(Session::Selecting(s)) => {
            let _ = std::fs::remove_file(&s.backdrop);
        }
        Some(Session::Capturing(c)) => {
            let _ = stop_capture(c);
        }
        None => {}
    }
    crate::windows::close_scroll_select(app);
    crate::windows::close_scroll_frame(app);
    crate::windows::show_main(app);
    let _ = app.emit("scroll://cancelled", ());
}

fn stop_capture(mut c: Capturing) -> Arc<Mutex<Stitcher>> {
    c.stopped.store(true, Ordering::SeqCst);
    if let Some(mut stdin) = c.stdin.take() {
        let _ = stdin.write_all(b"stop\n");
    }
    // The source exits on "stop"; do not wait for it longer than a moment.
    let deadline = Instant::now() + Duration::from_millis(1500);
    loop {
        match c.child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = c.child.kill();
                let _ = c.child.wait();
                break;
            }
        }
    }
    if let Some(reader) = c.reader.take() {
        let _ = reader.join();
    }
    c.stitcher
}

/// The dragged rectangle in the display's pixels: (x, y, w, h), even sizes, inside the display.
fn pixel_rect(
    r: LogicalRect,
    scale: f64,
    display_w: u32,
    display_h: u32,
) -> CmdResult<(u32, u32, u32, u32)> {
    let px = |v: f64| (v * scale).round().max(0.0) as u32;
    let x = px(r.x).min(display_w.saturating_sub(2));
    let y = px(r.y).min(display_h.saturating_sub(2));
    let w = (px(r.w).min(display_w - x)) & !1;
    let h = (px(r.h).min(display_h - y)) & !1;
    if w < MIN_SIDE_PX || h < MIN_SIDE_PX {
        return Err(CommandError::new(
            "error.scrollTooSmall",
            format!("the area is {w}×{h} pixels; drag a larger one"),
        ));
    }
    Ok((x, y, w, h))
}

/// Records on the pipe: "FGTF", width, height (little-endian u32), then BGRA rows.
fn read_frames(
    app: AppHandle,
    mut pipe: impl Read,
    stitcher: Arc<Mutex<Stitcher>>,
    stopped: Arc<AtomicBool>,
) {
    let mut frames = 0u32;
    let mut last_event = Instant::now() - Duration::from_secs(1);
    let mut last_warning: Option<&'static str> = None;
    let mut buf = Vec::new();
    loop {
        let mut head = [0u8; 12];
        if pipe.read_exact(&mut head).is_err() || &head[..4] != b"FGTF" {
            break;
        }
        let w = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        let h = u32::from_le_bytes([head[8], head[9], head[10], head[11]]) as usize;
        if w == 0 || h == 0 || w * h > 64_000_000 {
            break;
        }
        buf.resize(w * h * 4, 0);
        if pipe.read_exact(&mut buf).is_err() {
            break;
        }
        if stopped.load(Ordering::SeqCst) {
            continue; // drain, so the source can exit
        }
        let mut st = stitcher.lock().expect("stitcher lock");
        let warning = match st.push(&buf) {
            Push::Joined { .. } => {
                frames += 1;
                None
            }
            Push::Unchanged => continue,
            Push::NoOverlap => Some("noOverlap"),
            Push::Full { .. } => Some("full"),
        };
        if warning != last_warning {
            last_warning = warning;
            let _ = app.emit("scroll://warning", warning);
        }
        if last_event.elapsed() >= Duration::from_millis(200) {
            last_event = Instant::now();
            let (thumbnail, newest_at) = thumbnail(&st);
            let progress = ScrollProgress {
                width: st.width(),
                height: st.height(),
                frames,
                thumbnail,
                newest_at,
            };
            drop(st);
            let _ = app.emit("scroll://progress", progress);
        }
    }
    if !stopped.load(Ordering::SeqCst) {
        // The source ended by itself: permission withdrawn, display unplugged.
        let _ = app.emit("scroll://warning", Some("sourceEnded"));
    }
}

/// The picture so far, `THUMB_WIDTH` wide, nearest-neighbour; only the end when very tall.
fn thumbnail(st: &Stitcher) -> (Option<String>, f64) {
    use base64::Engine;
    let (w, h) = (st.width() as usize, st.height() as usize);
    if w == 0 || h == 0 {
        return (None, 0.0);
    }
    let step = (w as f64 / f64::from(THUMB_WIDTH)).max(1.0);
    let tw = ((w as f64 / step) as usize).max(1);
    let all_rows = ((h as f64 / step) as usize).max(1);
    let rows = all_rows.min(THUMB_MAX_ROWS as usize);
    let first = all_rows - rows;
    let px = st.pixels();
    let mut rgb = Vec::with_capacity(tw * rows * 3);
    for ty in 0..rows {
        let sy = (((first + ty) as f64 * step) as usize).min(h - 1);
        for tx in 0..tw {
            let sx = ((tx as f64 * step) as usize).min(w - 1);
            let i = (sy * w + sx) * 4;
            rgb.extend_from_slice(&[px[i + 2], px[i + 1], px[i]]);
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, tw as u32, rows as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let Ok(mut writer) = enc.write_header() else {
            return (None, 0.0);
        };
        if writer.write_image_data(&rgb).is_err() {
            return (None, 0.0);
        }
    }
    let newest_row = (f64::from(st.last_frame_top()) / step - first as f64).max(0.0);
    (
        Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(out)
        )),
        (newest_row / rows as f64).min(1.0),
    )
}

/// BGRA rows to an RGB PNG. `scale` 2 marks the file as 144 dpi so it opens at its size on screen.
fn encode_png(width: u32, height: u32, bgra: &[u8], scale: f64) -> CmdResult<Vec<u8>> {
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for p in bgra.as_chunks::<4>().0 {
        rgb.extend_from_slice(&[p[2], p[1], p[0]]);
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        // 72 dpi is 2835 pixels per metre.
        let ppm = (2834.65 * scale.max(1.0)).round() as u32;
        enc.set_pixel_dims(Some(png::PixelDimensions {
            xppu: ppm,
            yppu: ppm,
            unit: png::Unit::Meter,
        }));
        let mut writer = enc
            .write_header()
            .map_err(|e| CommandError::new("error.io", e.to_string()))?;
        writer
            .write_image_data(&rgb)
            .map_err(|e| CommandError::new("error.io", e.to_string()))?;
    }
    Ok(out)
}

/// A display as the frame source knows it.
pub struct SourceDisplay {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    /// Pixels per logical point.
    pub scale: f64,
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::process::{ChildStdout, Command, Stdio};

    /// The helper is there and the system can take the still for the selection (macOS 14).
    pub fn supported() -> bool {
        let fourteen = match tauri_plugin_os::version() {
            tauri_plugin_os::Version::Semantic(major, _, _) => major >= 14,
            _ => false,
        };
        fourteen && crate::commands::find_record_helper().is_some()
    }

    pub fn display(screen: usize) -> CmdResult<SourceDisplay> {
        let (id, width, height) = crate::windows::display_at(screen)
            .ok_or_else(|| CommandError::new("error.sourceNotFound", "the screen is gone"))?;
        let points = crate::windows::screen_size_points(screen)
            .unwrap_or((f64::from(width), f64::from(height)));
        Ok(SourceDisplay {
            id,
            width,
            height,
            scale: (f64::from(width) / points.0).max(1.0),
        })
    }

    fn helper() -> CmdResult<PathBuf> {
        crate::commands::find_record_helper()
            .ok_or_else(|| CommandError::new("error.unsupported", "the capture helper is missing"))
    }

    /// A full-size still of the screen as a JPEG in the temp folder.
    pub async fn still(screen: usize) -> CmdResult<PathBuf> {
        let d = display(screen)?;
        let file = std::env::temp_dir().join(format!(
            "flowgt-still-{}-{}.jpg",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let mut c = tokio::process::Command::new(helper()?);
        c.args(["--snapshot", "--display", &d.id.to_string(), "--out"])
            .arg(&file)
            .args(["--max-width", &d.width.min(8192).to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        match tokio::time::timeout(Duration::from_secs(6), c.output()).await {
            Ok(Ok(out)) if out.status.success() && file.exists() => Ok(file),
            Ok(Ok(out)) => Err(CommandError::new(
                if out.status.code() == Some(2) {
                    "error.permission"
                } else {
                    "error.captureFailed"
                },
                format!(
                    "{} {}",
                    String::from_utf8_lossy(&out.stdout).trim(),
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            )),
            Ok(Err(e)) => Err(CommandError::new("error.captureFailed", e.to_string())),
            Err(_) => Err(CommandError::new(
                "error.captureFailed",
                "the still took more than 6 s",
            )),
        }
    }

    /// Start the helper on the rectangle; its stdout carries the frames.
    pub fn frames(
        d: &SourceDisplay,
        crop: (u32, u32, u32, u32),
    ) -> CmdResult<(Child, ChildStdin, ChildStdout)> {
        let mut child = Command::new(helper()?)
            .args(["--scroll", "--display", &d.id.to_string(), "--crop"])
            .arg(format!("{},{},{},{}", crop.0, crop.1, crop.2, crop.3))
            .args(["--exclude-pid", &std::process::id().to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| CommandError::new("error.captureFailed", e.to_string()))?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        Ok((child, stdin, stdout))
    }

    /// Put the PNG on the general pasteboard. Returns whether the call was handed over.
    pub fn copy_png(app: &AppHandle, png: Vec<u8>) -> bool {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        use objc2_foundation::NSString;
        app.run_on_main_thread(move || {
            // SAFETY: documented NSPasteboard / NSData class and instance methods, on the main
            // thread; the data object copies the bytes before the vector is dropped.
            unsafe {
                let board: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
                if board.is_null() {
                    return;
                }
                let _: isize = msg_send![&*board, clearContents];
                let data: *mut AnyObject = msg_send![class!(NSData), dataWithBytes: png.as_ptr().cast::<std::ffi::c_void>(), length: png.len()];
                if data.is_null() {
                    return;
                }
                let kind = NSString::from_str("public.png");
                let _: bool = msg_send![&*board, setData: data, forType: &*kind];
            }
        })
        .is_ok()
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    //! No frame source yet. The Windows one goes here: a display rectangle as "FGTF" records
    //! (see `read_frames`), a still for the selection, and the clipboard.
    use super::*;
    use std::process::ChildStdout;

    pub fn supported() -> bool {
        false
    }

    fn missing<T>() -> CmdResult<T> {
        Err(CommandError::new(
            "error.unsupported",
            "scrolling screenshots are not available on this system",
        ))
    }

    pub fn display(_screen: usize) -> CmdResult<SourceDisplay> {
        missing()
    }

    pub async fn still(_screen: usize) -> CmdResult<PathBuf> {
        missing()
    }

    pub fn frames(
        _d: &SourceDisplay,
        _crop: (u32, u32, u32, u32),
    ) -> CmdResult<(Child, ChildStdin, ChildStdout)> {
        missing()
    }

    pub fn copy_png(_app: &AppHandle, _png: Vec<u8>) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_rect_scales_clamps_and_makes_even() {
        let r = LogicalRect {
            x: 10.0,
            y: 20.0,
            w: 400.5,
            h: 300.0,
        };
        assert_eq!(pixel_rect(r, 2.0, 3024, 1964).unwrap(), (20, 40, 800, 600));
        let edge = LogicalRect {
            x: 1400.0,
            y: 900.0,
            w: 400.0,
            h: 400.0,
        };
        assert_eq!(
            pixel_rect(edge, 2.0, 3024, 1964).unwrap(),
            (2800, 1800, 224, 164)
        );
        let tiny = LogicalRect {
            x: 0.0,
            y: 0.0,
            w: 20.0,
            h: 20.0,
        };
        assert_eq!(
            pixel_rect(tiny, 2.0, 3024, 1964).unwrap_err().code,
            "error.scrollTooSmall"
        );
    }

    #[test]
    fn png_has_the_size_and_the_density() {
        let bgra = [10u8, 20, 30, 255].repeat(4 * 2);
        let bytes = encode_png(4, 2, &bgra, 2.0).unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        let info = reader.info();
        assert_eq!((info.width, info.height), (4, 2));
        assert_eq!(info.pixel_dims.unwrap().xppu, 5669);
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut buf).unwrap();
        assert_eq!(&buf[..3], &[30, 20, 10], "BGRA became RGB");
    }

    #[test]
    fn frames_from_a_pipe_are_stitched() {
        // Two 64x64 frames of a page, the second scrolled by 16 rows.
        let (w, h) = (64u32, 64u32);
        let row = |y: u32| -> Vec<u8> {
            (0..w)
                .flat_map(|x| {
                    let v = ((x * 7 + y * 13) % 251) as u8;
                    [v, v.wrapping_mul(3), (y % 256) as u8, 255]
                })
                .collect()
        };
        let frame = |top: u32| -> Vec<u8> { (top..top + h).flat_map(row).collect() };
        let mut pipe = Vec::new();
        for top in [0u32, 16] {
            pipe.extend_from_slice(b"FGTF");
            pipe.extend_from_slice(&w.to_le_bytes());
            pipe.extend_from_slice(&h.to_le_bytes());
            pipe.extend_from_slice(&frame(top));
        }
        let mut st = Stitcher::new(w, h, StitchOptions::default());
        let mut cursor = std::io::Cursor::new(pipe);
        let mut head = [0u8; 12];
        while cursor.read_exact(&mut head).is_ok() {
            let mut buf = vec![0u8; (w * h * 4) as usize];
            cursor.read_exact(&mut buf).unwrap();
            st.push(&buf);
        }
        assert_eq!(st.height(), 80);
        assert!(thumbnail(&st)
            .0
            .unwrap()
            .starts_with("data:image/png;base64,"));
    }
}
