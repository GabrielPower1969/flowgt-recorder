//! Scrolling screenshot. The user drags a rectangle over a still of the screen, then scrolls the
//! page by hand; the frames of that rectangle are joined into one tall picture by
//! `recorder_core::stitch` and saved as a PNG.
//!
//! The shell's part is the session: the windows (selection, frame, control bar), the frame
//! source, the events, the file. The frame source is the only platform-specific piece: on macOS
//! the `sck-record --scroll` helper; on Windows the bundled ffmpeg's Desktop Duplication input;
//! other platforms report `scroll_capture: false`.

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
    let backdrop = match platform::still(app, screen).await {
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
        let display = platform::display(app, selecting.screen)?;
        let crop = pixel_rect(rect, display.scale, display.width, display.height)?;
        let (mut child, stdin, stdout) = platform::frames(&display, crop)?;
        let stitcher = Arc::new(Mutex::new(Stitcher::new(
            crop.2,
            crop.3,
            StitchOptions {
                settle: platform::settle_frames(crop.2, crop.3),
                ..StitchOptions::default()
            },
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

/// Where the picture stands, for a window that opened after the first frame had arrived.
pub fn progress() -> Option<ScrollProgress> {
    let stitcher = match SESSION.lock().expect("scroll lock").as_ref() {
        Some(Session::Capturing(c)) => c.stitcher.clone(),
        _ => return None,
    };
    let st = stitcher.lock().expect("stitcher lock");
    if st.height() == 0 {
        return None;
    }
    let (thumbnail, newest_at) = thumbnail(&st);
    Some(ScrollProgress {
        width: st.width(),
        height: st.height(),
        frames: 0,
        thumbnail,
        newest_at,
    })
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
        let _ = stdin.write_all(platform::STOP);
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
    use std::process::{Command, Stdio};

    /// The helper ends on this line.
    pub const STOP: &[u8] = b"stop\n";
    /// The helper sends only frames that changed, so a frame never repeats to confirm itself.
    pub fn settle_frames(_w: u32, _h: u32) -> u8 {
        0
    }

    /// The helper is there and the system can take the still for the selection (macOS 14).
    pub fn supported() -> bool {
        let fourteen = match tauri_plugin_os::version() {
            tauri_plugin_os::Version::Semantic(major, _, _) => major >= 14,
            _ => false,
        };
        fourteen && crate::commands::find_record_helper().is_some()
    }

    pub fn display(_app: &AppHandle, screen: usize) -> CmdResult<SourceDisplay> {
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

    fn display_at(screen: usize) -> CmdResult<SourceDisplay> {
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

    /// A full-size still of the screen as a JPEG in the temp folder. The capture service can be
    /// busy for a moment (the monitor's own thumbnail), so one failure is tried again.
    pub async fn still(_app: &AppHandle, screen: usize) -> CmdResult<PathBuf> {
        match still_once(screen).await {
            Ok(p) => Ok(p),
            Err(first) => {
                log::warn!("the still for the selection failed once: {}", first.detail);
                tokio::time::sleep(Duration::from_millis(400)).await;
                still_once(screen).await
            }
        }
    }

    async fn still_once(screen: usize) -> CmdResult<PathBuf> {
        let d = display_at(screen)?;
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
    ) -> CmdResult<(Child, ChildStdin, Box<dyn Read + Send>)> {
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
        Ok((child, stdin, Box::new(stdout)))
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

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    //! No frame source on this system.
    use super::*;

    pub const STOP: &[u8] = b"";

    pub fn settle_frames(_w: u32, _h: u32) -> u8 {
        0
    }

    pub fn supported() -> bool {
        false
    }

    fn missing<T>() -> CmdResult<T> {
        Err(CommandError::new(
            "error.unsupported",
            "scrolling screenshots are not available on this system",
        ))
    }

    pub fn display(_app: &AppHandle, _screen: usize) -> CmdResult<SourceDisplay> {
        missing()
    }

    pub async fn still(_app: &AppHandle, _screen: usize) -> CmdResult<PathBuf> {
        missing()
    }

    pub fn frames(
        _d: &SourceDisplay,
        _crop: (u32, u32, u32, u32),
    ) -> CmdResult<(Child, ChildStdin, Box<dyn Read + Send>)> {
        missing()
    }

    pub fn copy_png(_app: &AppHandle, _png: Vec<u8>) -> bool {
        false
    }
}

/// The PNG as a CF_DIB block (BITMAPINFOHEADER + bottom-up 32-bit rows) for the Windows clipboard.
#[cfg(windows)]
fn dib_from_png(png_bytes: &[u8]) -> Option<Vec<u8>> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return None,
    };
    let mut out = Vec::with_capacity(40 + w * h * 4);
    out.extend_from_slice(&40u32.to_le_bytes()); // biSize
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes()); // positive: bottom-up
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&((w * h * 4) as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]); // resolution, colours used / important
    for y in (0..h).rev() {
        let row = &buf[y * info.line_size..y * info.line_size + w * channels];
        for p in row.chunks_exact(channels) {
            out.extend_from_slice(&[p[2], p[1], p[0], 255]);
        }
    }
    Some(out)
}

#[cfg(windows)]
mod platform {
    //! Windows: the bundled ffmpeg's Desktop Duplication input (`ddagrab`), the same one a
    //! recording uses. It crops the rectangle itself, in the output's pixels, and windows marked
    //! with `set_content_protected` (WDA_EXCLUDEFROMCAPTURE) stay out of it. Windows.Graphics.
    //! Capture has no crop and draws a yellow border unless the user agrees in a prompt.
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    /// ffmpeg ends cleanly on "q" from its standard input.
    pub const STOP: &[u8] = b"q";
    /// Desktop Duplication at a fixed rate can catch a window halfway through repainting, and a
    /// repaint can outlast one frame at 20 per second: a frame is used once the screen has held
    /// still for about 100 ms (`StitchOptions::settle`), at least two frames.
    pub fn settle_frames(w: u32, h: u32) -> u8 {
        let fps = fps_for(w, h);
        ((fps as f64 * 0.1).ceil() as u8 + 1).max(2)
    }
    /// Frames per second: as many as a data budget allows, between 5 and 20. A frame is used
    /// only when the next one agrees, so a higher rate keeps up with faster scrolling.
    const MAX_FPS: u32 = 20;
    const MIN_FPS: u32 = 5;
    const BYTES_PER_SECOND: u64 = 160 * 1024 * 1024;
    /// Seconds of frames dropped at the start: the selection window's fade-out is over by then.
    const SETTLE_SECONDS: f64 = 0.4;

    pub(super) fn fps_for(w: u32, h: u32) -> u32 {
        let frame = u64::from(w) * u64::from(h) * 4;
        ((BYTES_PER_SECOND / frame.max(1)) as u32).clamp(MIN_FPS, MAX_FPS)
    }

    pub fn supported() -> bool {
        crate::sidecar::Tools::discover().is_ok()
    }

    /// The nth screen as Tauri lists it; the screen list gives Desktop Duplication the same
    /// index (`devices::windows_screens`).
    pub fn display(app: &AppHandle, screen: usize) -> CmdResult<SourceDisplay> {
        let monitors = app
            .available_monitors()
            .map_err(|e| CommandError::new("error.sourceNotFound", e.to_string()))?;
        let m = monitors
            .get(screen)
            .ok_or_else(|| CommandError::new("error.sourceNotFound", "the screen is gone"))?;
        Ok(SourceDisplay {
            id: screen as u32,
            width: m.size().width,
            height: m.size().height,
            scale: m.scale_factor().max(1.0),
        })
    }

    fn ffmpeg() -> CmdResult<PathBuf> {
        crate::sidecar::Tools::discover().map(|t| t.ffmpeg)
    }

    /// A full-size still of the screen as a JPEG in the temp folder.
    pub async fn still(app: &AppHandle, screen: usize) -> CmdResult<PathBuf> {
        let d = display(app, screen)?;
        let file = std::env::temp_dir().join(format!(
            "flowgt-still-{}-{}.jpg",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let mut c = tokio::process::Command::new(ffmpeg()?);
        c.args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-init_hw_device", "d3d11va", "-f", "lavfi", "-i"])
            .arg(format!(
                "ddagrab=output_idx={}:framerate=30:draw_mouse=0",
                d.id
            ))
            // A few frames in, so the duplication has a fresh image.
            .args(["-vf", "hwdownload,format=bgra,select=gte(n\\,2)"])
            .args(["-frames:v", "1", "-c:v", "mjpeg", "-q:v", "3"])
            .arg(&file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .kill_on_drop(true);
        match tokio::time::timeout(Duration::from_secs(6), c.output()).await {
            Ok(Ok(out)) if out.status.success() && file.exists() => Ok(file),
            Ok(Ok(out)) => Err(CommandError::new(
                "error.captureFailed",
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            )),
            Ok(Err(e)) => Err(CommandError::new("error.captureFailed", e.to_string())),
            Err(_) => Err(CommandError::new(
                "error.captureFailed",
                "the still took more than 6 s",
            )),
        }
    }

    /// ffmpeg on the rectangle, raw BGRA on its stdout, wrapped into "FGTF" records.
    pub fn frames(
        d: &SourceDisplay,
        crop: (u32, u32, u32, u32),
    ) -> CmdResult<(Child, ChildStdin, Box<dyn Read + Send>)> {
        let (x, y, w, h) = crop;
        let fps = fps_for(w, h);
        let mut child = Command::new(ffmpeg()?)
            .args(["-hide_banner", "-loglevel", "error"])
            .args(["-init_hw_device", "d3d11va", "-f", "lavfi", "-i"])
            .arg(format!(
                "ddagrab=output_idx={}:framerate={fps}:draw_mouse=0:video_size={w}x{h}:offset_x={x}:offset_y={y}",
                d.id
            ))
            .args(["-vf", "hwdownload,format=bgra", "-f", "rawvideo", "-pix_fmt", "bgra", "pipe:1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| CommandError::new("error.captureFailed", e.to_string()))?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let mut records = Records::new(stdout, w, h);
        records.skip = (f64::from(fps) * SETTLE_SECONDS).ceil() as u32;
        Ok((child, stdin, Box::new(records)))
    }

    /// Put the PNG on the clipboard twice: as "PNG" (keeps the exact pixels, read by browsers,
    /// Office, chat apps) and as a device-independent bitmap for programs that know only that.
    pub fn copy_png(_app: &AppHandle, png: Vec<u8>) -> bool {
        let dib = super::dib_from_png(&png);
        clipboard::put(&png, dib.as_deref())
    }

    mod clipboard {
        use std::ffi::c_void;
        type Handle = *mut c_void;
        #[link(name = "user32")]
        extern "system" {
            fn OpenClipboard(owner: Handle) -> i32;
            fn CloseClipboard() -> i32;
            fn EmptyClipboard() -> i32;
            fn SetClipboardData(format: u32, mem: Handle) -> Handle;
            fn RegisterClipboardFormatW(name: *const u16) -> u32;
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GlobalAlloc(flags: u32, bytes: usize) -> Handle;
            fn GlobalLock(mem: Handle) -> *mut c_void;
            fn GlobalUnlock(mem: Handle) -> i32;
            fn GlobalFree(mem: Handle) -> Handle;
        }
        const GMEM_MOVEABLE: u32 = 0x0002;
        const CF_DIB: u32 = 8;

        /// A movable global block holding `bytes`, or null.
        unsafe fn global(bytes: &[u8]) -> Handle {
            let mem = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
            if mem.is_null() {
                return mem;
            }
            let p = GlobalLock(mem);
            if p.is_null() {
                GlobalFree(mem);
                return std::ptr::null_mut();
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.cast::<u8>(), bytes.len());
            GlobalUnlock(mem);
            mem
        }

        pub fn put(png: &[u8], dib: Option<&[u8]>) -> bool {
            // SAFETY: documented Win32 clipboard calls. Another program may hold the clipboard
            // for a moment, so opening is tried a few times. After SetClipboardData succeeds the
            // system owns the block; on failure it is freed here.
            unsafe {
                let mut open = false;
                for _ in 0..10 {
                    if OpenClipboard(std::ptr::null_mut()) != 0 {
                        open = true;
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(30));
                }
                if !open {
                    return false;
                }
                EmptyClipboard();
                let name: Vec<u16> = "PNG\0".encode_utf16().collect();
                let fmt = RegisterClipboardFormatW(name.as_ptr());
                let mut any = false;
                for (format, bytes) in [(fmt, Some(png)), (CF_DIB, dib)] {
                    let Some(bytes) = bytes else { continue };
                    if format == 0 {
                        continue;
                    }
                    let mem = global(bytes);
                    if mem.is_null() {
                        continue;
                    }
                    if SetClipboardData(format, mem).is_null() {
                        GlobalFree(mem);
                    } else {
                        any = true;
                    }
                }
                CloseClipboard();
                any
            }
        }
    }

    /// Raw frames of a known size, each given the 12-byte "FGTF" head `read_frames` expects.
    struct Records<R> {
        inner: R,
        head: [u8; 12],
        pending: Option<u8>,
        frame: usize,
        pos: usize,
        /// Frames still to drop at the start: windows that just closed may be fading out.
        skip: u32,
    }

    impl<R: Read> Records<R> {
        fn new(inner: R, w: u32, h: u32) -> Self {
            let mut head = [0u8; 12];
            head[..4].copy_from_slice(b"FGTF");
            head[4..8].copy_from_slice(&w.to_le_bytes());
            head[8..].copy_from_slice(&h.to_le_bytes());
            Records {
                inner,
                head,
                pending: None,
                frame: w as usize * h as usize * 4,
                pos: 0,
                skip: 0,
            }
        }
    }

    impl<R: Read> Read for Records<R> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if out.is_empty() {
                return Ok(0);
            }
            while self.skip > 0 && self.pos == 0 {
                let mut sink = vec![0u8; self.frame];
                self.inner.read_exact(&mut sink)?;
                self.skip -= 1;
            }
            if self.pos < 12 {
                // A head is written only when a frame follows: peek one byte first.
                if self.pos == 0 {
                    let mut first = [0u8; 1];
                    if self.inner.read(&mut first)? == 0 {
                        return Ok(0);
                    }
                    let n = out.len().min(12);
                    out[..n].copy_from_slice(&self.head[..n]);
                    self.pending = Some(first[0]);
                    self.pos = n;
                    return Ok(n);
                }
                let n = out.len().min(12 - self.pos);
                out[..n].copy_from_slice(&self.head[self.pos..self.pos + n]);
                self.pos += n;
                return Ok(n);
            }
            if let Some(b) = self.pending.take() {
                out[0] = b;
                self.pos += 1;
                if self.pos == 12 + self.frame {
                    self.pos = 0;
                }
                return Ok(1);
            }
            let left = 12 + self.frame - self.pos;
            let want = out.len().min(left);
            let n = self.inner.read(&mut out[..want])?;
            self.pos += n;
            if self.pos == 12 + self.frame {
                self.pos = 0;
            }
            Ok(n)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn frame_rate_follows_the_area() {
            assert_eq!(fps_for(760, 900), 20);
            assert_eq!(fps_for(1920, 1080), 20);
            assert_eq!(fps_for(3840, 2160), 5);
            assert_eq!(fps_for(2560, 1440), 11);
            assert_eq!(settle_frames(760, 900), 3);
            assert_eq!(settle_frames(3840, 2160), 2);
        }

        #[test]
        fn raw_frames_get_a_head_each() {
            let (w, h) = (2u32, 2u32);
            let raw: Vec<u8> = (0..32u8).collect(); // two frames of 16 bytes
            let mut r = Records::new(std::io::Cursor::new(raw), w, h);
            let mut all = Vec::new();
            let mut chunk = [0u8; 5]; // odd size, to cross the boundaries
            loop {
                let n = r.read(&mut chunk).unwrap();
                if n == 0 {
                    break;
                }
                all.extend_from_slice(&chunk[..n]);
            }
            assert_eq!(all.len(), 2 * (12 + 16));
            assert_eq!(&all[..4], b"FGTF");
            assert_eq!(&all[12..28], &(0..16u8).collect::<Vec<_>>()[..]);
            assert_eq!(&all[28..32], b"FGTF");
            assert_eq!(&all[40..], &(16..32u8).collect::<Vec<_>>()[..]);
        }
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

    #[cfg(windows)]
    #[test]
    fn a_png_becomes_a_bottom_up_dib() {
        let bgra = [10u8, 20, 30, 255, 40, 50, 60, 255]; // 1x2: top pixel, bottom pixel
        let png = encode_png(1, 2, &bgra, 1.0).unwrap();
        let dib = dib_from_png(&png).unwrap();
        assert_eq!(dib.len(), 40 + 8);
        assert_eq!(&dib[40..44], &[40, 50, 60, 255], "bottom row first");
        assert_eq!(&dib[44..48], &[10, 20, 30, 255]);
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
