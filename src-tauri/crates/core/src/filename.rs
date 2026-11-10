//! File-name template: "{date} {time} {name}" → "2026-11-04 1430 Weekly sync.mp4".
//!
//! Three steps, each pure: [`render`] expands the template, [`sanitize`] makes the result legal on
//! macOS and Windows at once, [`unique_path`] dodges collisions with ` (2)`. [`build_stem`] runs
//! the first two in the right order for the common case.
use chrono::{DateTime, Local};
use std::path::{Path, PathBuf};

/// Characters that are illegal in a file name on macOS or Windows (the union of both).
pub const ILLEGAL_FILENAME_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
/// Longest stem we produce, in characters (not bytes: 120 CJK characters are 360 bytes and
/// still well inside the 255-byte limit of APFS, NTFS and exFAT).
pub const MAX_STEM_CHARS: usize = 120;
/// How many ` (n)` suffixes [`unique_path`] tries before giving up.
pub const MAX_UNIQUE_ATTEMPTS: u32 = 9_999;

/// Render the template. Unknown tokens are left verbatim. Result has no extension.
/// Tokens: {date}=YYYY-MM-DD {time}=HHmm {datetime}=YYYY-MM-DD_HHmm {name} {yyyy} {mm} {dd} {hh} {min}.
///
/// Tokens are matched exactly (lowercase). `name` is inserted as-is and never re-scanned, so a
/// meeting called "{date}" stays "{date}". The result is NOT sanitised; see [`build_stem`].
pub fn render(template: &str, name: &str, at: DateTime<Local>) -> String {
    let mut out = String::with_capacity(template.len() + name.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after_brace = &rest[open + 1..];
        match after_brace.find('}') {
            Some(close) => match expand_token(&after_brace[..close], name, &at) {
                Some(value) => {
                    out.push_str(&value);
                    rest = &after_brace[close + 1..];
                }
                None => {
                    // Unknown token: keep the brace and keep scanning after it.
                    out.push('{');
                    rest = after_brace;
                }
            },
            None => {
                out.push('{');
                rest = after_brace;
            }
        }
    }
    out.push_str(rest);
    out
}

fn expand_token(token: &str, name: &str, at: &DateTime<Local>) -> Option<String> {
    let format = match token {
        "name" => return Some(name.to_string()),
        "date" => "%Y-%m-%d",
        "time" => "%H%M",
        "datetime" => "%Y-%m-%d_%H%M",
        "yyyy" => "%Y",
        "mm" => "%m",
        "dd" => "%d",
        "hh" => "%H",
        "min" => "%M",
        _ => return None,
    };
    Some(at.format(format).to_string())
}

/// Remove characters illegal on macOS/Windows (/ \ : * ? " < > |), trim, collapse spaces,
/// cap at 120 chars; empty → "录像" fallback given by caller.
///
/// Also: control characters are dropped, any whitespace (tabs, full-width spaces) becomes one
/// ASCII space, leading and trailing dots and spaces go (Windows rejects trailing ones, macOS hides
/// files with a leading dot), and a Windows device name (CON, NUL, COM1 …, with or without an
/// extension) gets an underscore so `CON.mp4` cannot become the console.
pub fn sanitize(name: &str, fallback: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !ILLEGAL_FILENAME_CHARS.contains(c) && !c.is_control())
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let capped: String = collapsed.chars().take(MAX_STEM_CHARS).collect();
    let trimmed = capped.trim_matches(|c| c == '.' || c == ' ');
    if trimmed.is_empty() {
        return fallback.to_string();
    }
    guard_reserved_windows_name(trimmed)
}

/// `CON` → `CON_`, `con.backup` → `con_.backup`; anything else unchanged.
fn guard_reserved_windows_name(name: &str) -> String {
    let (base, rest) = match name.find('.') {
        Some(dot) => (&name[..dot], &name[dot..]),
        None => (name, ""),
    };
    if is_reserved_windows_name(base) {
        format!("{base}_{rest}")
    } else {
        name.to_string()
    }
}

/// Device names Windows refuses regardless of case or extension.
pub fn is_reserved_windows_name(base: &str) -> bool {
    let upper = base.trim().to_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$") {
        return true;
    }
    match upper.strip_prefix("COM").or_else(|| upper.strip_prefix("LPT")) {
        Some(digit) => {
            let mut chars = digit.chars();
            matches!((chars.next(), chars.next()), (Some('0'..='9' | '¹' | '²' | '³'), None))
        }
        None => false,
    }
}

/// "x.mp4" exists → "x (2).mp4", "x (3).mp4" …  `exists` is injected for testability.
///
/// `ext` may be given with or without the leading dot; empty means no extension. After
/// [`MAX_UNIQUE_ATTEMPTS`] collisions the last candidate is returned and the caller's
/// create-new open reports the real problem.
pub fn unique_path(dir: &Path, stem: &str, ext: &str, exists: &dyn Fn(&Path) -> bool) -> PathBuf {
    let ext = ext.trim_start_matches('.');
    let file_name = |suffix: &str| {
        if ext.is_empty() {
            format!("{stem}{suffix}")
        } else {
            format!("{stem}{suffix}.{ext}")
        }
    };
    let first = dir.join(file_name(""));
    if !exists(&first) {
        return first;
    }
    for n in 2..=MAX_UNIQUE_ATTEMPTS {
        let candidate = dir.join(file_name(&format!(" ({n})")));
        if !exists(&candidate) {
            return candidate;
        }
    }
    dir.join(file_name(&format!(" ({MAX_UNIQUE_ATTEMPTS})")))
}

/// The stem for a new recording: sanitise the meeting name (empty → `fallback`), render the
/// template, sanitise the result again in case the template itself carried junk.
pub fn build_stem(template: &str, name: &str, fallback: &str, at: DateTime<Local>) -> String {
    let name = sanitize(name, fallback);
    sanitize(&render(template, &name, at), fallback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone};
    use pretty_assertions::assert_eq;
    use std::collections::HashSet;

    /// 2026-11-04 14:30:00 local time, whatever the machine's zone.
    fn at() -> DateTime<Local> {
        let naive = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap().and_hms_opt(14, 30, 0).unwrap();
        Local.from_local_datetime(&naive).unwrap()
    }

    #[test]
    fn default_template_with_chinese_name() {
        assert_eq!(render("{date} {time} {name}", "周会", at()), "2026-11-04 1430 周会");
    }

    #[test]
    fn every_token_renders() {
        assert_eq!(
            render("{yyyy}/{mm}/{dd} {hh}:{min} {datetime}", "x", at()),
            "2026/11/04 14:30 2026-11-04_1430"
        );
        assert_eq!(render("{date}", "x", at()), "2026-11-04");
        assert_eq!(render("{time}", "x", at()), "1430");
        assert_eq!(render("{name}", "Weekly sync", at()), "Weekly sync");
    }

    #[test]
    fn midnight_and_single_digit_fields_are_zero_padded() {
        let naive = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap().and_hms_opt(0, 7, 0).unwrap();
        let t = Local.from_local_datetime(&naive).unwrap();
        assert_eq!(render("{date} {time}", "", t), "2026-01-05 0007");
    }

    #[test]
    fn unknown_tokens_and_stray_braces_stay_verbatim() {
        assert_eq!(render("{foo} {name}", "周会", at()), "{foo} 周会");
        assert_eq!(render("{Date} {name}", "x", at()), "{Date} x", "tokens are case-sensitive");
        assert_eq!(render("a{b", "x", at()), "a{b");
        assert_eq!(render("a}b{name}", "x", at()), "a}bx");
        assert_eq!(render("{{date}}", "x", at()), "{2026-11-04}");
        assert_eq!(render("", "x", at()), "");
        assert_eq!(render("plain", "x", at()), "plain");
    }

    #[test]
    fn name_is_not_rescanned_for_tokens() {
        assert_eq!(render("{name}", "{date}", at()), "{date}");
        assert_eq!(render("{name}{name}", "{", at()), "{{");
    }

    #[test]
    fn empty_name_leaves_the_separator_for_sanitize() {
        assert_eq!(render("{date} {time} {name}", "", at()), "2026-11-04 1430 ");
        assert_eq!(build_stem("{date} {time} {name}", "", "录像", at()), "2026-11-04 1430 录像");
    }

    #[test]
    fn sanitize_strips_illegal_and_control_characters() {
        assert_eq!(sanitize(r#"a/b\c:d*e?f"g<h>i|j"#, "录像"), "abcdefghij");
        assert_eq!(sanitize("a\u{0}b\u{7f}c\u{9b}d", "录像"), "abcd");
        assert_eq!(sanitize("tab\tand\nnewline", "录像"), "tab and newline", "whitespace controls become spaces");
        assert_eq!(sanitize("周会 / 2026 第3次", "录像"), "周会 2026 第3次");
    }

    #[test]
    fn sanitize_collapses_and_trims_whitespace() {
        assert_eq!(sanitize("  hello    world  ", "录像"), "hello world");
        assert_eq!(sanitize("周\u{3000}会", "录像"), "周 会", "full-width space");
        assert_eq!(sanitize("a \u{a0} b", "录像"), "a b", "no-break space");
    }

    #[test]
    fn sanitize_removes_leading_and_trailing_dots_and_spaces() {
        assert_eq!(sanitize("meeting...", "录像"), "meeting");
        assert_eq!(sanitize(". hidden.", "录像"), "hidden");
        assert_eq!(sanitize("v1.2 notes", "录像"), "v1.2 notes", "inner dots stay");
        assert_eq!(sanitize("...", "录像"), "录像");
    }

    #[test]
    fn sanitize_caps_at_120_characters_not_bytes() {
        let long: String = "会".repeat(200);
        let out = sanitize(&long, "录像");
        assert_eq!(out.chars().count(), MAX_STEM_CHARS);
        assert_eq!(out.len(), MAX_STEM_CHARS * 3, "three bytes per character");
        let ascii = "a".repeat(300);
        assert_eq!(sanitize(&ascii, "录像").len(), MAX_STEM_CHARS);
        // A space exposed by the cut is trimmed, so the result can be shorter than the cap.
        let spaced = format!("{} {}", "a".repeat(119), "b".repeat(10));
        let out = sanitize(&spaced, "录像");
        assert_eq!(out, "a".repeat(119));
    }

    #[test]
    fn sanitize_guards_windows_device_names() {
        assert_eq!(sanitize("CON", "录像"), "CON_");
        assert_eq!(sanitize("nul", "录像"), "nul_");
        assert_eq!(sanitize("Aux", "录像"), "Aux_");
        assert_eq!(sanitize("COM1", "录像"), "COM1_");
        assert_eq!(sanitize("lpt9", "录像"), "lpt9_");
        assert_eq!(sanitize("con.backup", "录像"), "con_.backup", "the part before the first dot counts");
        assert_eq!(sanitize("console", "录像"), "console");
        assert_eq!(sanitize("COM10", "录像"), "COM10");
        assert_eq!(sanitize("COM", "录像"), "COM");
        assert_eq!(sanitize("PRN 2026", "录像"), "PRN 2026");
        assert!(is_reserved_windows_name("com¹"));
        assert!(!is_reserved_windows_name(""));
    }

    #[test]
    fn sanitize_falls_back_when_nothing_is_left() {
        assert_eq!(sanitize("", "录像"), "录像");
        assert_eq!(sanitize("???", "录像"), "录像");
        assert_eq!(sanitize("   ", "录像"), "录像");
        assert_eq!(sanitize("\u{1}\u{2}", "Recording"), "Recording");
    }

    #[test]
    fn unique_path_counts_up_from_two() {
        let dir = Path::new("/Users/g/Movies");
        let mut taken: HashSet<PathBuf> = HashSet::new();
        let exists = |p: &Path| taken.contains(p);
        assert_eq!(unique_path(dir, "x", "mp4", &exists), dir.join("x.mp4"));

        taken.insert(dir.join("x.mp4"));
        let exists = |p: &Path| taken.contains(p);
        assert_eq!(unique_path(dir, "x", "mp4", &exists), dir.join("x (2).mp4"));

        taken.insert(dir.join("x (2).mp4"));
        let exists = |p: &Path| taken.contains(p);
        assert_eq!(unique_path(dir, "x", "mp4", &exists), dir.join("x (3).mp4"));

        // A hole is reused: (2) free again while (3) is taken.
        taken.remove(&dir.join("x (2).mp4"));
        taken.insert(dir.join("x (3).mp4"));
        let exists = |p: &Path| taken.contains(p);
        assert_eq!(unique_path(dir, "x", "mp4", &exists), dir.join("x (2).mp4"));
    }

    #[test]
    fn unique_path_extension_forms() {
        let dir = Path::new("/out");
        let none = |_: &Path| false;
        assert_eq!(unique_path(dir, "x", ".mp4", &none), dir.join("x.mp4"), "leading dot tolerated");
        assert_eq!(unique_path(dir, "x", "", &none), dir.join("x"), "no extension");
        let all = |_: &Path| true;
        assert_eq!(unique_path(dir, "x", "", &all), dir.join("x (9999)"));
        assert_eq!(unique_path(dir, "2026-11-04 1430 周会", "mp4", &all), dir.join("2026-11-04 1430 周会 (9999).mp4"));
    }

    #[test]
    fn build_stem_sanitises_name_and_template_output() {
        assert_eq!(build_stem("{date} {time} {name}", "Q4 / 预算: 复盘?", "录像", at()), "2026-11-04 1430 Q4 预算 复盘");
        assert_eq!(build_stem("{hh}:{min} {name}", "x", "录像", at()), "1430 x", "junk from the template is removed too");
        assert_eq!(build_stem("{name}", "", "录像", at()), "录像");
        assert_eq!(build_stem("", "", "录像", at()), "录像");
    }
}
