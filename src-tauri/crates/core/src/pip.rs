//! Picture-in-picture geometry and the filter_complex fragment.
//!
//! Labels are passed WITHOUT brackets (`"1:v"`, `"pip"`); the functions add them.
use crate::types::*;

/// Resolve corner/margin/size%/offset into absolute output pixels; clamps inside the frame.
///
/// The side is `size_pct` % of the output height (10–50 %), rounded down to an even number and
/// never larger than the frame. The anchor is the chosen corner minus `margin`; `offset_x/y`
/// (a drag) then moves it, and the final rectangle is clamped so it never leaves the frame.
pub fn layout(cam: &CameraSettings, out_w: u32, out_h: u32) -> PipLayout {
    let pct = cam.size_pct.clamp(10, 50);
    let max_side = out_w.min(out_h);
    let size = ((out_h * pct / 100) & !1).clamp(2, (max_side & !1).max(2));

    let margin = i64::from(cam.margin);
    let (w, h, s) = (i64::from(out_w), i64::from(out_h), i64::from(size));
    let left = margin;
    let right = w - s - margin;
    let top = margin;
    let bottom = h - s - margin;
    let (ax, ay) = match cam.corner {
        Corner::TopLeft => (left, top),
        Corner::TopRight => (right, top),
        Corner::BottomLeft => (left, bottom),
        Corner::BottomRight => (right, bottom),
    };
    let x = (ax + i64::from(cam.offset_x)).clamp(0, (w - s).max(0));
    let y = (ay + i64::from(cam.offset_y)).clamp(0, (h - s).max(0));
    PipLayout { size, x: x as u32, y: y as u32, shape: cam.shape, mirror: cam.mirror }
}

/// Filter fragment that turns input label `cam` into `pip`:
///   crop=ih:ih (square) → hflip if mirror → scale=S:S → format=rgba, then for rounded / circle
///   an alpha mask (corner radius S/8, or S/2 for a circle, one-pixel anti-aliased edge) is merged
///   with `alphamerge`.
///
/// The mask is drawn by `geq` on ONE frame and looped (`trim` + `loop`), because running `geq`
/// on every camera frame cost ~37 ms CPU per 264 px frame in a measurement (11.5 s vs 0.5 s for
/// 300 frames, ffmpeg 7.1.1, M1 Pro) — see docs/BENCH.md.
/// Returns the chain WITHOUT the overlay step.
pub fn pip_filter(layout: &PipLayout, in_label: &str, out_label: &str) -> String {
    let s = layout.size;
    let mut chain = format!("[{in_label}]crop=ih:ih");
    if layout.mirror {
        chain.push_str(",hflip");
    }
    chain.push_str(&format!(",scale={s}:{s}:flags=bicubic,format=rgba"));
    let radius = match layout.shape {
        PipShape::Square => {
            chain.push_str(&format!("[{out_label}]"));
            return chain;
        }
        PipShape::Rounded => s / 8,
        PipShape::Circle => s / 2,
    };
    chain.push_str(&format!("[{out_label}_c];{}[{out_label}_m];[{out_label}_c][{out_label}_m]alphamerge[{out_label}]", mask_source(s, radius)));
    chain
}

/// A one-frame gray mask (255 inside a rounded square of corner radius `r`, linear falloff over
/// one pixel at the edge), looped forever at 30 fps.
fn mask_source(side: u32, r: u32) -> String {
    let half = f64::from(side) / 2.0;
    let r = f64::from(r);
    let inner = half - r;
    let dx = format!("max(abs(X-{half}+0.5)-{inner},0)");
    let dy = format!("max(abs(Y-{half}+0.5)-{inner},0)");
    format!(
        "nullsrc=s={side}x{side}:r=30,format=gray,geq=lum='clip(({r}-hypot({dx},{dy}))*255+127.5,0,255)',trim=end_frame=1,loop=loop=-1:size=1:start=0,setpts=N/(30*TB)"
    )
}

/// `[base][pip]overlay=x:y:format=auto[out]`
pub fn overlay_filter(layout: &PipLayout, base_label: &str, pip_label: &str, out_label: &str) -> String {
    format!("[{base_label}][{pip_label}]overlay={}:{}:format=auto[{out_label}]", layout.x, layout.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::camera_settings;
    use pretty_assertions::assert_eq;

    #[test]
    fn bottom_right_default() {
        let l = layout(&camera_settings(), 1920, 1080);
        // 22 % of 1080 = 237.6 → 237 → even 236.
        assert_eq!(l.size, 236);
        assert_eq!((l.x, l.y), (1920 - 236 - 24, 1080 - 236 - 24));
        assert!(l.mirror);
    }

    #[test]
    fn all_four_corners() {
        let mut c = camera_settings();
        c.size_pct = 20; // 216
        c.margin = 10;
        let mut at = |corner| {
            c.corner = corner;
            let l = layout(&c, 1920, 1080);
            (l.x, l.y)
        };
        assert_eq!(at(Corner::TopLeft), (10, 10));
        assert_eq!(at(Corner::TopRight), (1920 - 216 - 10, 10));
        assert_eq!(at(Corner::BottomLeft), (10, 1080 - 216 - 10));
        assert_eq!(at(Corner::BottomRight), (1920 - 216 - 10, 1080 - 216 - 10));
    }

    #[test]
    fn offset_moves_and_clamps() {
        let mut c = camera_settings();
        c.size_pct = 20;
        c.margin = 10;
        c.offset_x = -100;
        c.offset_y = -50;
        let l = layout(&c, 1920, 1080);
        assert_eq!((l.x, l.y), (1920 - 216 - 10 - 100, 1080 - 216 - 10 - 50));
        c.offset_x = 5000;
        c.offset_y = 5000;
        let l = layout(&c, 1920, 1080);
        assert_eq!((l.x, l.y), (1920 - 216, 1080 - 216));
        c.offset_x = -5000;
        c.offset_y = -5000;
        let l = layout(&c, 1920, 1080);
        assert_eq!((l.x, l.y), (0, 0));
    }

    #[test]
    fn size_pct_is_clamped_and_even() {
        let mut c = camera_settings();
        c.size_pct = 5;
        assert_eq!(layout(&c, 1920, 1080).size, 108);
        c.size_pct = 99;
        assert_eq!(layout(&c, 1920, 1080).size, 540);
        c.size_pct = 33;
        assert_eq!(layout(&c, 1280, 720).size % 2, 0);
        // A tiny, odd frame still yields a rectangle inside it.
        let l = layout(&c, 100, 61);
        assert!(l.x + l.size <= 100 && l.y + l.size <= 61);
    }

    fn lay(shape: PipShape, mirror: bool) -> PipLayout {
        PipLayout { size: 240, x: 100, y: 50, shape, mirror }
    }

    #[test]
    fn filter_chain_order_and_labels() {
        let f = pip_filter(&lay(PipShape::Rounded, true), "1:v", "pip");
        assert!(f.starts_with("[1:v]crop=ih:ih,hflip,scale=240:240"));
        assert!(f.contains("format=rgba[pip_c];nullsrc=s=240x240"));
        assert!(f.contains("[pip_c][pip_m]alphamerge[pip]"));
        assert!(f.ends_with("[pip]"));
        let hflip = f.find("hflip").unwrap();
        let scale = f.find("scale=").unwrap();
        let geq = f.find("geq=").unwrap();
        assert!(hflip < scale && scale < geq);
        // The mask is one looped frame, never a per-frame geq on the camera picture.
        assert!(f.contains("trim=end_frame=1,loop=loop=-1:size=1"));
    }

    #[test]
    fn mirror_and_square_variants() {
        let f = pip_filter(&lay(PipShape::Square, false), "cam", "pip");
        assert_eq!(f, "[cam]crop=ih:ih,scale=240:240:flags=bicubic,format=rgba[pip]");
        assert!(!f.contains("hflip") && !f.contains("geq"));
    }

    #[test]
    fn rounded_radius_is_an_eighth_and_circle_is_half() {
        let r = pip_filter(&lay(PipShape::Rounded, false), "c", "p");
        assert!(r.contains("(30-hypot("), "{r}");
        assert!(r.contains("-90,0)"), "inner half-extent 120-30 = 90: {r}");
        let c = pip_filter(&lay(PipShape::Circle, false), "c", "p");
        assert!(c.contains("(120-hypot("), "{c}");
        assert!(c.contains("-0,0)"), "{c}");
        assert!(c.ends_with("alphamerge[p]"));
    }

    #[test]
    fn overlay_string() {
        assert_eq!(overlay_filter(&lay(PipShape::Square, false), "scr", "pip", "vout"), "[scr][pip]overlay=100:50:format=auto[vout]");
    }
}
