//! Scrolling-screenshot stitcher: joins frames of a fixed screen rectangle, captured while the
//! user scrolls by hand, into one tall image.
//!
//! The approach follows `longstitch` from jietuba (<https://github.com/1003129155/jietuba>),
//! MIT License, Copyright (c) 2025 JYAARU. This is a smaller reimplementation of the same idea:
//!
//! 1. Every row gets two hashes that ignore a strip on the right (the scrollbar): an exact one
//!    over the pixels and a coarse one over the quantised mean colour.
//! 2. Leading and trailing rows equal to the previous frame at the same position are a fixed
//!    header and footer; they take no part in matching.
//! 3. The frame is looked for near the previous frame first, then in the whole image.
//! 4. Candidates are runs of equal row hashes. A candidate counts only when it holds enough rows
//!    whose hash occurs exactly once on both sides, which rules out blank areas and repeated
//!    list rows, and when a brightness comparison over the whole overlap agrees.
//! 5. The frame is then prepended, appended (a fixed footer is kept once, at the very end) or,
//!    when it lies inside the image, only remembered as the current position.
//!
//! A wrong join ruins the picture while a missed one only asks the user to scroll back a little,
//! so every doubtful case ends in [`Push::NoOverlap`].
//!
//! Pure and deterministic: no I/O, no threads, no dependencies beyond `std`.
use std::cmp::Reverse;
use std::collections::HashMap;

/// A placement is trusted when this many of its rows can be located on their own.
const CREDIBLE_UNIQUE_ROWS: usize = 8;
/// How many candidate placements are examined per search.
const CANDIDATES: usize = 32;
/// At least a tenth of the width is ignored on the right, whatever the option says.
const IGNORE_RIGHT_DIVISOR: u32 = 10;
/// The near search reaches this many frame heights above and below the previous frame.
const SEARCH_FRAMES: usize = 2;
/// Width in pixels of one bin of a row's brightness profile.
const PROFILE_BIN: usize = 8;
/// At most this many rows of a frame are compared by brightness.
const VERIFY_SAMPLE_ROWS: usize = 256;
/// Two rows are alike when their profiles differ by at most this much per channel on average.
const VERIFY_ROW_DIFF: u64 = 2;
/// A row whose bins all lie within this much per channel says nothing and is not compared.
const VERIFY_FLAT: u32 = 2;
/// A seam is put where this many consecutive rows agree …
const SEAM_ROWS: usize = 16;
/// … and are not all alike.
const SEAM_DISTINCT_ROWS: usize = 4;

const HASH_MULTIPLIER: u64 = 0x517c_c1b7_2722_0a95;
/// Keeps blue, green and red of two little-endian BGRA pixels; alpha means nothing on screen.
const COLOUR_MASK: u64 = 0x00ff_ffff_00ff_ffff;

/// Options; `Default` gives the documented constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StitchOptions {
    /// Width of the strip on the right that is not compared (scrollbar). Default 20; the
    /// effective value is `max(this, width / 10)`.
    pub ignore_right_px: u32,
    /// The image stops growing at this many rows. Default 30 000.
    pub max_height: u32,
    /// The image stops growing at this many bytes. Default 300 MB.
    pub max_bytes: usize,
}

impl Default for StitchOptions {
    fn default() -> Self {
        StitchOptions {
            ignore_right_px: 20,
            max_height: 30_000,
            max_bytes: 300 * 1024 * 1024,
        }
    }
}

/// What a pushed frame did to the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Push {
    /// First frame, or the frame extended the image. `added` rows were added (0 when the frame
    /// lies inside what is already there).
    Joined { added: u32, height: u32 },
    /// Pixel-identical to the previous frame.
    Unchanged,
    /// No trustworthy overlap with the image; nothing was changed. The UI tells the user to
    /// scroll back a little.
    NoOverlap,
    /// A limit was reached; the image is complete as far as it goes and nothing more is accepted.
    Full { height: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HashKind {
    Exact,
    Coarse,
}

/// The two hashes of every row of a frame or of the image.
#[derive(Debug, Default)]
struct RowHashes {
    exact: Vec<u64>,
    coarse: Vec<u64>,
}

impl RowHashes {
    fn len(&self) -> usize {
        self.exact.len()
    }

    fn of(&self, kind: HashKind) -> &[u64] {
        match kind {
            HashKind::Exact => &self.exact,
            HashKind::Coarse => &self.coarse,
        }
    }

    /// Replaces the first `cut` rows with rows `from..to` of `frame`.
    fn replace_head(&mut self, cut: usize, frame: &RowHashes, from: usize, to: usize) {
        self.exact
            .splice(..cut, frame.exact[from..to].iter().copied());
        self.coarse
            .splice(..cut, frame.coarse[from..to].iter().copied());
    }

    /// Keeps the first `keep` rows and appends rows `from..to` of `frame`.
    fn replace_tail(&mut self, keep: usize, frame: &RowHashes, from: usize, to: usize) {
        self.exact.truncate(keep);
        self.exact.extend_from_slice(&frame.exact[from..to]);
        self.coarse.truncate(keep);
        self.coarse.extend_from_slice(&frame.coarse[from..to]);
    }
}

/// A run of equal row hashes: `len` rows starting at row `image` of the image and row `frame`
/// of the frame, `support` of which occur exactly once on both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    image: usize,
    frame: usize,
    len: usize,
    support: usize,
}

impl Run {
    /// Row of the image where the frame's first row lands; negative above the image.
    fn top(&self) -> isize {
        self.image as isize - self.frame as isize
    }
}

/// Joins pushed frames into one tall BGRA image.
pub struct Stitcher {
    width: u32,
    frame_height: u32,
    options: StitchOptions,
    pixels: Vec<u8>,
    rows: RowHashes,
    previous_frame: Vec<u8>,
    previous_exact: Vec<u64>,
    last_top: usize,
    /// Header and footer heights seen the last time the content moved.
    settled_bars: Option<(usize, usize)>,
    started: bool,
    full: bool,
}

impl Stitcher {
    pub fn new(width: u32, height: u32, options: StitchOptions) -> Stitcher {
        Stitcher {
            width,
            frame_height: height,
            options,
            pixels: Vec::new(),
            rows: RowHashes::default(),
            previous_frame: Vec::new(),
            previous_exact: Vec::new(),
            last_top: 0,
            settled_bars: None,
            started: false,
            full: false,
        }
    }

    /// `bgra` is exactly `width * height * 4` bytes, rows top to bottom, no padding.
    /// Never panics: a wrong length returns [`Push::NoOverlap`].
    pub fn push(&mut self, bgra: &[u8]) -> Push {
        if self.full {
            return Push::Full {
                height: self.height(),
            };
        }
        let stride = self.stride();
        let frame_height = self.frame_height as usize;
        if stride == 0 || frame_height == 0 || bgra.len() != stride * frame_height {
            return Push::NoOverlap;
        }
        if !self.started {
            return self.start(bgra);
        }
        if self.previous_frame == bgra {
            return Push::Unchanged;
        }

        let frame = self.hash_rows(bgra);
        let bars = self.fixed_bars(&frame.exact);
        let mut found = self.locate(bgra, &frame, bars).map(|found| (found, bars));
        if let (None, Some(settled)) = (&found, self.settled_bars) {
            // Nothing scrolled and only part of the frame changed in place: the still part looks
            // like taller bars than the ones seen while scrolling. Try again with those.
            let narrower = (bars.0.min(settled.0), bars.1.min(settled.1));
            if narrower != bars {
                found = self
                    .locate(bgra, &frame, narrower)
                    .map(|found| (found, narrower));
            }
        }
        self.previous_frame.copy_from_slice(bgra);
        let Some(((run, kind), bars)) = found else {
            self.previous_exact = frame.exact;
            return Push::NoOverlap;
        };
        if run.top() != self.last_top as isize {
            self.settled_bars = Some(bars);
        }
        let outcome = self.commit(run, kind, bgra, &frame);
        self.previous_exact = frame.exact;
        outcome
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    /// Rows stitched so far (0 before the first frame).
    pub fn height(&self) -> u32 {
        self.rows.len() as u32
    }

    /// BGRA, `width * height() * 4` bytes, top to bottom.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Row in the image where the most recent joined frame starts (for the preview outline).
    pub fn last_frame_top(&self) -> u32 {
        self.last_top as u32
    }

    pub fn into_pixels(self) -> (u32, u32, Vec<u8>) {
        (self.width, self.rows.len() as u32, self.pixels)
    }

    fn stride(&self) -> usize {
        self.width as usize * 4
    }

    /// The most rows the image may hold under both limits.
    fn limit_rows(&self) -> usize {
        let by_bytes = self
            .options
            .max_bytes
            .checked_div(self.stride())
            .unwrap_or(0);
        by_bytes.min(self.options.max_height as usize)
    }

    /// Pixels of a row that take part in matching: everything left of the ignored strip.
    fn matched_width(&self) -> usize {
        let ignored = self
            .options
            .ignore_right_px
            .max(self.width / IGNORE_RIGHT_DIVISOR);
        let kept = if ignored < self.width {
            self.width - ignored
        } else {
            self.width
        };
        kept as usize
    }

    /// Rows `start..end` of the frame that take part in matching: everything between the bars.
    fn frame_range(&self, bars: (usize, usize)) -> (usize, usize) {
        let height = self.frame_height as usize;
        let start = bars.0.min(height);
        (start, height.saturating_sub(bars.1).max(start))
    }

    fn start(&mut self, bgra: &[u8]) -> Push {
        let stride = self.stride();
        let limit = self.limit_rows();
        let take = (self.frame_height as usize).min(limit);
        let frame = self.hash_rows(bgra);
        self.pixels.extend_from_slice(&bgra[..take * stride]);
        self.rows.replace_tail(0, &frame, 0, take);
        self.previous_frame = bgra.to_vec();
        self.previous_exact = frame.exact;
        self.last_top = 0;
        self.started = true;
        self.joined(0, limit)
    }

    /// The outcome of a join that started from `before` rows.
    fn joined(&mut self, before: usize, limit: usize) -> Push {
        let height = self.height();
        if height as usize >= limit {
            self.full = true;
            return Push::Full { height };
        }
        Push::Joined {
            added: height.saturating_sub(before as u32),
            height,
        }
    }

    fn hash_rows(&self, bgra: &[u8]) -> RowHashes {
        let matched = self.matched_width() * 4;
        let mut hashes = RowHashes::default();
        for row in bgra.chunks_exact(self.stride()) {
            let row = &row[..matched];
            hashes.exact.push(exact_hash(row));
            hashes.coarse.push(coarse_hash(row));
        }
        hashes
    }

    /// Leading and trailing rows equal to the previous frame at the same position: the content
    /// scrolled and these did not, so they are a fixed header and footer. They always match the
    /// bars at both ends of the image and would pull the frame there. Not counted when the two
    /// frames are the same (nothing scrolled) or when the bars cover half the frame or more.
    fn fixed_bars(&self, frame: &[u64]) -> (usize, usize) {
        let previous = &self.previous_exact;
        if previous.len() != frame.len() || previous.as_slice() == frame {
            return (0, 0);
        }
        let top = frame
            .iter()
            .zip(previous)
            .take_while(|(a, b)| a == b)
            .count();
        let bottom = frame
            .iter()
            .rev()
            .zip(previous.iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        if (top + bottom) * 2 >= frame.len() {
            return (0, 0);
        }
        (top, bottom)
    }

    /// Finds where the frame sits in the image: by exact hashes first, then by coarse ones for
    /// content that was rendered slightly differently.
    fn locate(
        &self,
        bgra: &[u8],
        frame: &RowHashes,
        bars: (usize, usize),
    ) -> Option<(Run, HashKind)> {
        [HashKind::Exact, HashKind::Coarse]
            .into_iter()
            .find_map(|kind| {
                self.locate_by(kind, bgra, frame, bars)
                    .map(|run| (run, kind))
            })
    }

    /// Searches near the previous frame, then the whole image, and takes the best supported
    /// candidate that also passes the brightness comparison.
    fn locate_by(
        &self,
        kind: HashKind,
        bgra: &[u8],
        frame: &RowHashes,
        bars: (usize, usize),
    ) -> Option<Run> {
        let image = self.rows.of(kind);
        let (start, end) = self.frame_range(bars);
        let part = &frame.of(kind)[start..end];
        let frame_counts = occurrences(part);
        let unique_in_frame = frame_counts
            .values()
            .filter(|(count, _)| *count == 1)
            .count();
        // A frame with few locatable rows (mostly blank) cannot be asked for the full number.
        // Coarse hashes collide often, so a handful of them proves nothing: no discount there.
        let needed = match kind {
            HashKind::Exact => CREDIBLE_UNIQUE_ROWS.min((unique_in_frame / 2).max(1)),
            HashKind::Coarse => CREDIBLE_UNIQUE_ROWS,
        };
        if unique_in_frame < needed {
            return None;
        }
        let reach = SEARCH_FRAMES * self.frame_height as usize;
        let near = (
            self.last_top.saturating_sub(reach),
            (self.last_top + reach).min(image.len()),
        );
        let whole = (0, image.len());
        let search = |(from, to): (usize, usize)| {
            let mut runs = common_runs(&image[from..to], part, &frame_counts);
            for run in &mut runs {
                run.image += from;
                run.frame += start;
            }
            let last_top = self.last_top as isize;
            runs.sort_unstable_by_key(|run| {
                (
                    Reverse(run.support),
                    Reverse(run.len),
                    run.top().abs_diff(last_top),
                    run.top(),
                )
            });
            runs.into_iter()
                .take(CANDIDATES)
                .filter(|run| run.support >= needed)
                .find(|run| self.looks_alike(bgra, run.top(), bars))
        };
        search(near).or_else(|| if near == whole { None } else { search(whole) })
    }

    /// With the frame placed at `top`, most of the overlap really looks the same. Row hashes can
    /// agree on a small part only (an element that does not scroll with the page); comparing the
    /// whole overlap exposes that. Rows of the image that hold its own header and footer are
    /// left out. With nothing to compare the hashes are believed.
    fn looks_alike(&self, bgra: &[u8], top: isize, bars: (usize, usize)) -> bool {
        let bins = self.matched_width() / PROFILE_BIN;
        let (start, end) = self.frame_range(bars);
        if bins == 0 || start >= end {
            return true;
        }
        let stride = self.stride();
        let first = bars.0 as isize;
        let last = self.rows.len() as isize - bars.1 as isize;
        let flat = VERIFY_FLAT * PROFILE_BIN as u32 * 3;
        let tolerance = VERIFY_ROW_DIFF * (PROFILE_BIN * 3 * bins) as u64;
        let (mut compared, mut alike) = (0usize, 0usize);
        let (mut ours, mut theirs) = (Vec::with_capacity(bins), Vec::with_capacity(bins));
        for row in (start..end).step_by((end - start).div_ceil(VERIFY_SAMPLE_ROWS)) {
            let at = top + row as isize;
            if at < first || at >= last {
                continue;
            }
            brightness_profile(&bgra[row * stride..(row + 1) * stride], bins, &mut ours);
            let brightest = ours.iter().max().copied().unwrap_or(0);
            let darkest = ours.iter().min().copied().unwrap_or(0);
            if brightest - darkest <= flat {
                continue;
            }
            let at = at as usize;
            brightness_profile(
                &self.pixels[at * stride..(at + 1) * stride],
                bins,
                &mut theirs,
            );
            let diff: u64 = ours
                .iter()
                .zip(&theirs)
                .map(|(a, b)| u64::from(a.abs_diff(*b)))
                .sum();
            compared += 1;
            if diff <= tolerance {
                alike += 1;
            }
        }
        compared == 0 || alike * 4 >= compared * 3
    }

    /// Writes the frame into the image at the place the run gives.
    fn commit(&mut self, run: Run, kind: HashKind, bgra: &[u8], frame: &RowHashes) -> Push {
        let stride = self.stride();
        let frame_height = self.frame_height as usize;
        let len = self.rows.len();
        let limit = self.limit_rows();
        let top = run.top();
        if top < 0 {
            // Join where image and frame agree nearest the top: what lies above that in the image
            // is its old header, replaced by the frame's rows down to the seam.
            let (cut, rows) = match self.seam(kind, frame, top, false) {
                Some(from) => (from, (from as isize - top) as usize),
                None => (run.image, run.frame),
            };
            // Rows at the frame's top that no longer fit under the limit.
            let skip = (len - cut + rows).saturating_sub(limit).min(rows);
            self.pixels.splice(
                ..cut * stride,
                bgra[skip * stride..rows * stride].iter().copied(),
            );
            self.rows.replace_head(cut, frame, skip, rows);
            self.last_top = 0;
        } else if top as usize + frame_height > len {
            let top = top as usize;
            // Join where image and frame agree nearest the end: what lies below that in the image
            // is its old footer, replaced by the rest of the frame, which ends in the new one.
            let (keep, skip) = match self.seam(kind, frame, top as isize, true) {
                Some(to) => (to, to - top),
                None => (run.image + run.len, run.frame + run.len),
            };
            let take = (frame_height - skip).min(limit.saturating_sub(keep));
            self.pixels.truncate(keep * stride);
            self.pixels
                .extend_from_slice(&bgra[skip * stride..(skip + take) * stride]);
            self.rows.replace_tail(keep, frame, skip, skip + take);
            self.last_top = top;
        } else {
            self.last_top = top as usize;
        }
        self.joined(len, limit)
    }

    /// With the frame placed at `top`, the row of the image where a seam can go: the nearest
    /// place to the image's end (`at_end`) or start where `SEAM_ROWS` consecutive rows agree
    /// with the frame and are not all alike. At the end the seam lies just below those rows,
    /// at the start on the first of them.
    fn seam(&self, kind: HashKind, frame: &RowHashes, top: isize, at_end: bool) -> Option<usize> {
        let image = self.rows.of(kind);
        let theirs = frame.of(kind);
        let low = top.max(0) as usize;
        let high = ((top + theirs.len() as isize).max(0) as usize).min(image.len());
        let last = high.checked_sub(SEAM_ROWS).filter(|last| *last >= low)?;
        let holds = |from: &usize| {
            let rows = &image[*from..*from + SEAM_ROWS];
            let start = (*from as isize - top) as usize;
            rows == &theirs[start..start + SEAM_ROWS] && distinct(rows) >= SEAM_DISTINCT_ROWS
        };
        if at_end {
            (low..=last).rev().find(holds).map(|from| from + SEAM_ROWS)
        } else {
            (low..=last).find(holds)
        }
    }
}

/// Hash of the pixels of a row; equal only for rows that are the same pixel for pixel.
fn exact_hash(row: &[u8]) -> u64 {
    let mut hash = 0u64;
    let (words, rest) = row.as_chunks::<8>();
    for w in words {
        let word = u64::from_le_bytes(*w) & COLOUR_MASK;
        hash = (hash.rotate_left(5) ^ word).wrapping_mul(HASH_MULTIPLIER);
    }
    if let [b, g, r, ..] = rest {
        let word = u64::from(*b) | u64::from(*g) << 8 | u64::from(*r) << 16;
        hash = (hash.rotate_left(5) ^ word).wrapping_mul(HASH_MULTIPLIER);
    }
    hash
}

/// Hash of the mean colour of a row in steps of 8; survives small rendering differences.
fn coarse_hash(row: &[u8]) -> u64 {
    let pixels = (row.len() / 4) as u64;
    if pixels == 0 {
        return 0;
    }
    let mut sums = [0u64; 3];
    for pixel in row.as_chunks::<4>().0 {
        sums[0] += u64::from(pixel[0]);
        sums[1] += u64::from(pixel[1]);
        sums[2] += u64::from(pixel[2]);
    }
    let step = |sum: u64| sum / pixels / 8;
    (step(sums[0]) | step(sums[1]) << 8 | step(sums[2]) << 16).wrapping_mul(HASH_MULTIPLIER)
}

/// Sum of blue, green and red over each `PROFILE_BIN` pixels of a row, `bins` of them from the left.
fn brightness_profile(row: &[u8], bins: usize, out: &mut Vec<u32>) {
    out.clear();
    out.extend(
        row.as_chunks::<{ PROFILE_BIN * 4 }>()
            .0
            .iter()
            .take(bins)
            .map(|bin| {
                bin.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]))
                    .sum::<u32>()
            }),
    );
}

/// For every hash: how often it occurs and the index of its last occurrence.
fn occurrences(hashes: &[u64]) -> HashMap<u64, (u32, usize)> {
    let mut counts: HashMap<u64, (u32, usize)> = HashMap::with_capacity(hashes.len());
    for (index, hash) in hashes.iter().enumerate() {
        let entry = counts.entry(*hash).or_insert((0, index));
        entry.0 += 1;
        entry.1 = index;
    }
    counts
}

fn distinct(hashes: &[u64]) -> usize {
    let mut sorted = hashes.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.len()
}

/// Maximal runs of equal hashes between `window` and `part` that hold at least one row occurring
/// exactly once in both. Such a row matches in one place only, so the runs are found by growing
/// each of those matches up and down instead of comparing every row with every row.
fn common_runs(window: &[u64], part: &[u64], part_counts: &HashMap<u64, (u32, usize)>) -> Vec<Run> {
    let window_counts = occurrences(window);
    // (image row − frame row, frame row) of every row that is unique on both sides.
    let mut anchors: Vec<(isize, usize)> = part_counts
        .iter()
        .filter(|(_, (count, _))| *count == 1)
        .filter_map(|(hash, (_, j))| match window_counts.get(hash) {
            Some((1, i)) => Some((*i as isize - *j as isize, *j)),
            _ => None,
        })
        .collect();
    anchors.sort_unstable();

    let mut runs = Vec::new();
    let mut next = 0;
    while next < anchors.len() {
        let (shift, j) = anchors[next];
        let i = (j as isize + shift) as usize;
        let mut back = 0;
        while back < i.min(j) && window[i - back - 1] == part[j - back - 1] {
            back += 1;
        }
        let mut forward = 1;
        while i + forward < window.len()
            && j + forward < part.len()
            && window[i + forward] == part[j + forward]
        {
            forward += 1;
        }
        let mut support = 0;
        while next < anchors.len() && anchors[next].0 == shift && anchors[next].1 < j + forward {
            support += 1;
            next += 1;
        }
        runs.push(Run {
            image: i - back,
            frame: j - back,
            len: back + forward,
            support,
        });
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 400;
    const H: usize = 300;
    const PAGE_ROWS: usize = 6000;
    const HEADER: usize = 40;
    const FOOTER: usize = 30;
    const SCROLLBAR: usize = 12;

    /// xorshift64*, enough for test pictures.
    struct Rng(u64);

    impl Rng {
        fn new(seed: u64) -> Rng {
            Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
        }

        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
        }

        /// A number in `from..to`.
        fn range(&mut self, from: usize, to: usize) -> usize {
            from + (self.next() >> 33) as usize % (to - from)
        }
    }

    const PAPER: [u8; 4] = [250, 250, 250, 255];

    fn blank_row(width: usize) -> Vec<u8> {
        PAPER.repeat(width)
    }

    /// A row of text-like noise: dark cells of 4 px on paper. Practically never the same twice.
    fn text_row(rng: &mut Rng, width: usize) -> Vec<u8> {
        let mut row = Vec::with_capacity(width * 4);
        while row.len() < width * 4 {
            let bits = rng.next();
            let cell = if bits.is_multiple_of(3) {
                [
                    (bits >> 8) as u8 % 120,
                    (bits >> 16) as u8 % 120,
                    (bits >> 24) as u8 % 120,
                    255,
                ]
            } else {
                PAPER
            };
            for _ in 0..4 {
                row.extend_from_slice(&cell);
            }
        }
        row.truncate(width * 4);
        row
    }

    /// One list item of 12 rows: 8 rows of a fixed pattern and 4 blank ones. Every item is the same.
    fn list_item(width: usize) -> Vec<u8> {
        let mut rng = Rng::new(77);
        let mut item = Vec::new();
        for _ in 0..8 {
            item.extend(text_row(&mut rng, width));
        }
        for _ in 0..4 {
            item.extend(blank_row(width));
        }
        item
    }

    /// A page: stretches of text (40 to 100 unique rows) separated by a blank gap, a solid block,
    /// three identical list items or a ruled line. Gaps, blocks and lists are at most 36 rows.
    fn page_of(width: usize, seed: u64, rows: usize) -> Vec<u8> {
        let mut rng = Rng::new(seed);
        let mut page = Vec::with_capacity((rows + 200) * width * 4);
        while page.len() < rows * width * 4 {
            for _ in 0..rng.range(40, 100) {
                page.extend(text_row(&mut rng, width));
            }
            match rng.range(0, 4) {
                0 => {
                    for _ in 0..rng.range(12, 37) {
                        page.extend(blank_row(width));
                    }
                }
                1 => {
                    let colour = [
                        rng.range(0, 256) as u8,
                        rng.range(0, 256) as u8,
                        rng.range(0, 256) as u8,
                        255,
                    ];
                    let mut row = blank_row(width);
                    for x in width / 10..width * 3 / 4 {
                        row[x * 4..x * 4 + 4].copy_from_slice(&colour);
                    }
                    for _ in 0..rng.range(12, 37) {
                        page.extend_from_slice(&row);
                    }
                }
                2 => {
                    for _ in 0..3 {
                        page.extend(list_item(width));
                    }
                }
                _ => page.extend([128, 128, 128, 255].repeat(width)),
            }
        }
        page.truncate(rows * width * 4);
        page
    }

    fn page(seed: u64) -> Vec<u8> {
        page_of(W, seed, PAGE_ROWS)
    }

    fn rows(page: &[u8], from: usize, to: usize) -> &[u8] {
        &page[from * W * 4..to * W * 4]
    }

    fn frame(page: &[u8], top: usize) -> Vec<u8> {
        rows(page, top, top + H).to_vec()
    }

    fn stitcher() -> Stitcher {
        Stitcher::new(W as u32, H as u32, StitchOptions::default())
    }

    /// Tops of frames scrolling from `from` towards `to` in random steps of `steps.0..=steps.1`.
    fn scroll(rng: &mut Rng, from: usize, to: usize, steps: (usize, usize)) -> Vec<usize> {
        let mut tops = vec![from];
        let mut top = from;
        loop {
            let step = rng.range(steps.0, steps.1 + 1);
            let next = if to >= from {
                top + step
            } else {
                top.saturating_sub(step)
            };
            if (to >= from && next > to) || (to < from && next < to) {
                return tops;
            }
            top = next;
            tops.push(top);
        }
    }

    /// Pushes the frame at every top and checks each one joined exactly where it belongs.
    fn drive(s: &mut Stitcher, tops: &[usize], make: impl Fn(usize) -> Vec<u8>) {
        let (mut lowest, mut highest) = (tops[0], tops[0]);
        for &top in tops {
            let before = s.height() as usize;
            lowest = lowest.min(top);
            highest = highest.max(top);
            let height = highest + H - lowest;
            let outcome = s.push(&make(top));
            assert_eq!(
                outcome,
                Push::Joined {
                    added: (height - before) as u32,
                    height: height as u32
                },
                "frame at {top}"
            );
            assert_eq!(s.last_frame_top() as usize, top - lowest, "frame at {top}");
        }
    }

    #[test]
    fn scrolling_down_in_random_steps_rebuilds_the_page() {
        let p = page(1);
        let tops = scroll(&mut Rng::new(11), 0, PAGE_ROWS - H, (20, 250));
        assert!(tops.len() > 30);
        let mut s = stitcher();
        drive(&mut s, &tops, |top| frame(&p, top));
        let last = *tops.last().unwrap();
        assert_eq!(s.width(), W as u32);
        assert_eq!(s.height() as usize, last + H);
        assert!(s.pixels() == rows(&p, 0, last + H));
        let (width, height, pixels) = s.into_pixels();
        assert_eq!(
            (width as usize, height as usize, pixels.len()),
            (W, last + H, (last + H) * W * 4)
        );
    }

    #[test]
    fn scrolling_down_then_up_past_the_start_prepends() {
        let p = page(2);
        let mut rng = Rng::new(22);
        let mut tops = scroll(&mut rng, 2000, 3500, (20, 250));
        let turn = *tops.last().unwrap();
        tops.extend(scroll(&mut rng, turn, 1000, (20, 250)).into_iter().skip(1));
        let lowest = *tops.last().unwrap();
        assert!(lowest < 2000 && turn > 3000);
        let mut s = stitcher();
        drive(&mut s, &tops, |top| frame(&p, top));
        assert!(s.pixels() == rows(&p, lowest, turn + H));
    }

    #[test]
    fn a_fixed_header_and_footer_appear_once() {
        let p = page(3);
        let bars = page_of(W, 33, HEADER + FOOTER);
        let (header, footer) = bars.split_at(HEADER * W * 4);
        let make = |top: usize| {
            let mut f = header.to_vec();
            f.extend_from_slice(rows(&p, top + HEADER, top + H - FOOTER));
            f.extend_from_slice(footer);
            f
        };
        let tops = scroll(&mut Rng::new(34), 100, 4000, (20, 150));
        let mut s = stitcher();
        drive(&mut s, &tops, make);
        let last = *tops.last().unwrap();
        let mut expected = header.to_vec();
        expected.extend_from_slice(rows(&p, 100 + HEADER, last + H - FOOTER));
        expected.extend_from_slice(footer);
        assert!(s.pixels() == expected.as_slice());
    }

    #[test]
    fn a_fixed_header_survives_scrolling_back_up() {
        let p = page(3);
        let bars = page_of(W, 33, HEADER + FOOTER);
        let (header, footer) = bars.split_at(HEADER * W * 4);
        let make = |top: usize| {
            let mut f = header.to_vec();
            f.extend_from_slice(rows(&p, top + HEADER, top + H - FOOTER));
            f.extend_from_slice(footer);
            f
        };
        let mut rng = Rng::new(35);
        let mut tops = scroll(&mut rng, 1500, 2200, (20, 150));
        let turn = *tops.last().unwrap();
        tops.extend(scroll(&mut rng, turn, 900, (20, 150)).into_iter().skip(1));
        let lowest = *tops.last().unwrap();
        let mut s = stitcher();
        drive(&mut s, &tops, make);
        let mut expected = header.to_vec();
        expected.extend_from_slice(rows(&p, lowest + HEADER, turn + H - FOOTER));
        expected.extend_from_slice(footer);
        assert!(s.pixels() == expected.as_slice());
    }

    #[test]
    fn a_moving_scrollbar_thumb_is_ignored() {
        let p = page(4);
        let make = |top: usize| {
            let mut f = frame(&p, top);
            let thumb = top * (H - 40) / PAGE_ROWS;
            for y in 0..H {
                let shade = if (thumb..thumb + 40).contains(&y) {
                    80
                } else {
                    200
                };
                for x in W - SCROLLBAR..W {
                    f[(y * W + x) * 4..(y * W + x) * 4 + 4]
                        .copy_from_slice(&[shade, shade, shade, 255]);
                }
            }
            f
        };
        let tops = scroll(&mut Rng::new(44), 0, 4000, (20, 250));
        let mut s = stitcher();
        drive(&mut s, &tops, make);
        let last = *tops.last().unwrap();
        let kept = (W - SCROLLBAR) * 4;
        let same = s
            .pixels()
            .as_chunks::<{ W * 4 }>()
            .0
            .iter()
            .zip(rows(&p, 0, last + H).as_chunks::<{ W * 4 }>().0)
            .all(|(a, b)| a[..kept] == b[..kept]);
        assert!(same);
    }

    #[test]
    fn the_same_frame_twice_is_unchanged() {
        let p = page(5);
        let mut s = stitcher();
        assert_eq!(
            s.push(&frame(&p, 500)),
            Push::Joined {
                added: H as u32,
                height: H as u32
            }
        );
        assert_eq!(s.push(&frame(&p, 500)), Push::Unchanged);
        assert_eq!(
            s.push(&frame(&p, 560)),
            Push::Joined {
                added: 60,
                height: H as u32 + 60
            }
        );
        assert_eq!(s.push(&frame(&p, 560)), Push::Unchanged);
        assert_eq!(s.height() as usize, H + 60);
    }

    #[test]
    fn a_frame_inside_the_image_adds_nothing() {
        let p = page(5);
        let mut s = stitcher();
        drive(&mut s, &[500, 700, 900], |top| frame(&p, top));
        assert_eq!(
            s.push(&frame(&p, 620)),
            Push::Joined {
                added: 0,
                height: 700
            }
        );
        assert_eq!(s.last_frame_top(), 120);
        assert!(s.pixels() == rows(&p, 500, 1200));
    }

    #[test]
    fn a_jump_past_the_image_is_no_overlap_until_scrolling_back() {
        let p = page(6);
        let mut s = stitcher();
        drive(&mut s, &[1000, 1100], |top| frame(&p, top));
        let before = s.pixels().to_vec();
        assert_eq!(s.push(&frame(&p, 1800)), Push::NoOverlap);
        assert_eq!(s.height(), 400);
        assert_eq!(s.last_frame_top(), 100);
        assert!(s.pixels() == before.as_slice());
        assert_eq!(
            s.push(&frame(&p, 1250)),
            Push::Joined {
                added: 150,
                height: 550
            }
        );
        assert!(s.pixels() == rows(&p, 1000, 1550));
    }

    #[test]
    fn a_frame_from_another_page_is_no_overlap() {
        let (p, other) = (page(7), page(8));
        let mut s = stitcher();
        drive(&mut s, &[0, 150, 300, 450], |top| frame(&p, top));
        for top in [0, 150, 450, 2000, 5000] {
            assert_eq!(
                s.push(&frame(&other, top)),
                Push::NoOverlap,
                "other page at {top}"
            );
        }
        assert!(s.pixels() == rows(&p, 0, 750));
    }

    #[test]
    fn blank_frames_never_join() {
        let blank = |mark: usize| {
            let mut f = blank_row(W).repeat(H);
            // Something in the scrollbar strip moves so the frames are not identical.
            f[(mark * W + W - 3) * 4] = 0;
            f
        };
        let mut s = stitcher();
        assert_eq!(
            s.push(&blank(1)),
            Push::Joined {
                added: H as u32,
                height: H as u32
            }
        );
        for mark in 2..8 {
            assert_eq!(s.push(&blank(mark)), Push::NoOverlap);
        }
        assert_eq!(s.height() as usize, H);

        let p = page(9);
        let mut s = stitcher();
        drive(&mut s, &[0, 200, 400], |top| frame(&p, top));
        assert_eq!(s.push(&blank(1)), Push::NoOverlap);
        assert_eq!(s.push(&blank(2)), Push::NoOverlap);
        assert_eq!(
            s.push(&frame(&p, 500)),
            Push::Joined {
                added: 100,
                height: 800
            }
        );
        assert!(s.pixels() == rows(&p, 0, 800));
    }

    #[test]
    fn a_long_list_of_identical_items_is_never_joined_wrongly() {
        // 400 rows of text, 60 identical items (720 rows), 400 rows of text.
        let source = page(10);
        let mut p = rows(&source, 0, 400).to_vec();
        for _ in 0..60 {
            p.extend(list_item(W));
        }
        p.extend_from_slice(rows(&source, 400, 800));
        let mut s = stitcher();
        let mut refused = 0;
        for top in (0..=p.len() / (W * 4) - H).step_by(100) {
            match s.push(&frame(&p, top)) {
                Push::Joined { height, .. } => {
                    assert_eq!(s.last_frame_top() as usize, top, "frame at {top}");
                    assert_eq!(height as usize, top + H, "frame at {top}");
                }
                Push::NoOverlap => refused += 1,
                other => panic!("frame at {top}: {other:?}"),
            }
            let height = s.height() as usize;
            assert!(s.pixels() == rows(&p, 0, height), "frame at {top}");
        }
        // Inside the list nothing can be located; refusing is the right answer.
        assert!(refused > 0);
    }

    #[test]
    fn reaching_max_height_is_full_and_stays_full() {
        let p = page(11);
        let options = StitchOptions {
            max_height: 1000,
            ..StitchOptions::default()
        };
        let mut s = Stitcher::new(W as u32, H as u32, options);
        let mut outcomes = Vec::new();
        for top in (0..1200).step_by(120) {
            outcomes.push(s.push(&frame(&p, top)));
        }
        assert_eq!(
            outcomes[5],
            Push::Joined {
                added: 120,
                height: 900
            }
        );
        assert_eq!(outcomes[6], Push::Full { height: 1000 });
        assert!(outcomes[7..]
            .iter()
            .all(|o| *o == Push::Full { height: 1000 }));
        assert_eq!(s.push(&frame(&p, 0)), Push::Full { height: 1000 });
        assert!(s.pixels() == rows(&p, 0, 1000));
    }

    #[test]
    fn reaching_max_bytes_is_full() {
        let p = page(11);
        let options = StitchOptions {
            max_bytes: 450 * W * 4 + 7,
            ..StitchOptions::default()
        };
        let mut s = Stitcher::new(W as u32, H as u32, options);
        assert_eq!(
            s.push(&frame(&p, 100)),
            Push::Joined {
                added: 300,
                height: 300
            }
        );
        assert_eq!(s.push(&frame(&p, 300)), Push::Full { height: 450 });
        assert_eq!(s.push(&frame(&p, 320)), Push::Full { height: 450 });
        assert!(s.pixels() == rows(&p, 100, 550));
    }

    #[test]
    fn a_wrong_buffer_length_is_no_overlap() {
        let p = page(12);
        let mut s = stitcher();
        assert_eq!(s.push(&[0u8; 10]), Push::NoOverlap);
        assert_eq!(s.push(&[]), Push::NoOverlap);
        assert_eq!(s.height(), 0);
        assert!(s.pixels().is_empty());
        assert_eq!(
            s.push(&frame(&p, 0)),
            Push::Joined {
                added: H as u32,
                height: H as u32
            }
        );
        assert_eq!(s.push(rows(&p, 50, 50 + H + 1)), Push::NoOverlap);
        assert_eq!(s.push(rows(&p, 50, 50 + H - 1)), Push::NoOverlap);
        assert_eq!(
            s.push(&frame(&p, 50)),
            Push::Joined {
                added: 50,
                height: H as u32 + 50
            }
        );

        let mut empty = Stitcher::new(0, 0, StitchOptions::default());
        assert_eq!(empty.push(&[]), Push::NoOverlap);
    }

    /// Not a test of behaviour: prints how long one push takes. Run with
    /// `cargo test --release -p recorder-core stitch::tests::time_of_one_push -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn time_of_one_push() {
        let (width, height) = (1600usize, 1200usize);
        let p = page_of(width, 13, 30_000);
        let mut s = Stitcher::new(width as u32, height as u32, StitchOptions::default());
        let mut slowest = std::time::Duration::ZERO;
        let mut total = std::time::Duration::ZERO;
        let tops: Vec<usize> = (0..=30_000 - height).step_by(400).collect();
        for &top in &tops {
            let f = &p[top * width * 4..(top + height) * width * 4];
            let started = std::time::Instant::now();
            let outcome = s.push(f);
            let took = started.elapsed();
            assert!(
                matches!(outcome, Push::Joined { .. } | Push::Full { .. }),
                "{outcome:?}"
            );
            slowest = slowest.max(took);
            total += took;
        }
        let other = page_of(width, 14, height);
        let mut s = Stitcher::new(
            width as u32,
            height as u32,
            StitchOptions {
                max_height: 40_000,
                ..StitchOptions::default()
            },
        );
        for &top in &tops {
            s.push(&p[top * width * 4..(top + height) * width * 4]);
        }
        let started = std::time::Instant::now();
        assert_eq!(s.push(&other), Push::NoOverlap);
        println!(
            "1600x1200 push: mean {:?}, slowest {:?}; no overlap against {} rows: {:?}",
            total / tops.len() as u32,
            slowest,
            s.height(),
            started.elapsed()
        );
    }
}
