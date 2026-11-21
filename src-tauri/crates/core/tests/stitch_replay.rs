//! Replays frames captured from a real screen through the stitcher, one verdict per frame.
//! Not part of the normal run (needs a capture file):
//!   FLOWGT_REPLAY=<frames.bgra>,<width>,<height> cargo test -p recorder-core --test stitch_replay -- --ignored --nocapture
use recorder_core::stitch::{Push, StitchOptions, Stitcher};
use std::io::Read;

#[test]
#[ignore]
fn replay_captured_frames() {
    let spec = std::env::var("FLOWGT_REPLAY").expect("FLOWGT_REPLAY=<file>,<w>,<h>");
    let parts: Vec<&str> = spec.split(',').collect();
    let (w, h): (u32, u32) = (parts[1].parse().unwrap(), parts[2].parse().unwrap());
    let mut file = std::fs::File::open(parts[0]).unwrap();
    let settle: u8 = std::env::var("FLOWGT_SETTLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut st = Stitcher::new(
        w,
        h,
        StitchOptions {
            settle,
            ..StitchOptions::default()
        },
    );
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let mut i = 0;
    while file.read_exact(&mut buf).is_ok() {
        let r = st.push(&buf);
        if r != Push::Unchanged {
            println!("frame {i:3}: {r:?} last_top={}", st.last_frame_top());
        }
        i += 1;
    }
    println!("final height {}", st.height());
}
