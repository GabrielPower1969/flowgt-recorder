//! File-name template: "{date} {time} {name}" → "2026-11-04 1430 Weekly sync.mp4".
use chrono::{DateTime, Local};

/// Render the template. Unknown tokens are left verbatim. Result has no extension.
/// Tokens: {date}=YYYY-MM-DD {time}=HHmm {datetime}=YYYY-MM-DD_HHmm {name} {yyyy} {mm} {dd} {hh} {min}.
pub fn render(template: &str, name: &str, at: DateTime<Local>) -> String {
    todo!()
}

/// Remove characters illegal on macOS/Windows (/ \ : * ? " < > |), trim, collapse spaces,
/// cap at 120 chars; empty → "录像" fallback given by caller.
pub fn sanitize(name: &str, fallback: &str) -> String {
    todo!()
}

/// "x.mp4" exists → "x (2).mp4", "x (3).mp4" …  `exists` is injected for testability.
pub fn unique_path(dir: &std::path::Path, stem: &str, ext: &str, exists: &dyn Fn(&std::path::Path) -> bool) -> std::path::PathBuf {
    todo!()
}
