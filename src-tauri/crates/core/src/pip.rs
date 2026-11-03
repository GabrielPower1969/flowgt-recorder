//! Picture-in-picture geometry and the filter_complex fragment.
use crate::types::*;

/// Resolve corner/margin/size%/offset into absolute output pixels; clamps inside the frame.
pub fn layout(cam: &CameraSettings, out_w: u32, out_h: u32) -> PipLayout {
    todo!()
}

/// Filter fragment that turns input label `[cam]` into `[pip]`:
///   crop=ih:ih (square) → hflip if mirror → scale=S:S → shape mask via geq alpha (rounded r=S/8 or circle) → format=rgba
/// Returns the chain WITHOUT the overlay step.
pub fn pip_filter(layout: &PipLayout, in_label: &str, out_label: &str) -> String {
    todo!()
}

/// `[base][pip]overlay=x:y:format=auto[out]`
pub fn overlay_filter(layout: &PipLayout, base_label: &str, pip_label: &str, out_label: &str) -> String {
    todo!()
}
