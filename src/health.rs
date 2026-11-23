//! Append-only log of what every data source did, written to `sources.log` next to the
//! settings.
//!
//! A line is only written when the outcome for one source changes, so the file reads as
//! "html stopped parsing at 13:42, innertube-about took over" instead of one line per poll.
//! Logging never panics: a failure here must not disturb the overlay.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const OK: &str = "OK";
pub const EMPTY: &str = "LEEG";
pub const FAILED: &str = "FOUT";

/// Rotate to `sources.log.1` once the log grows past this size.
const MAX_BYTES: u64 = 256 * 1024;

static LAST_OUTCOME: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Records what one source did. `kind` is "kanaal" or "video", `target` the handle or video
/// id, `source` the label used in the status line ("html", "innertube-about", ...).
pub fn record(kind: &str, target: &str, source: &str, outcome: &str, detail: &str) {
    let key = format!("{kind}|{target}|{source}");

    if let Ok(mut last) = LAST_OUTCOME.lock() {
        if last.get(&key).map(String::as_str) == Some(outcome) {
            return;
        }
        last.insert(key, outcome.to_string());
        drop(last);
        write_line(&format_line(kind, target, source, outcome, detail));
    }
}

fn format_line(kind: &str, target: &str, source: &str, outcome: &str, detail: &str) -> String {
    format!(
        "{}  {}  {}  {}  {}{}",
        timestamp(),
        pad(kind, 7),
        pad(target, 20),
        pad(source, 18),
        pad(outcome, 5),
        detail
    )
}

fn pad(value: &str, width: usize) -> String {
    let taken: String = value.chars().take(width).collect();
    format!("{taken:<width$}")
}

fn write_line(line: &str) {
    let path = crate::settings::data_dir().join("sources.log");
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(meta) = fs::metadata(&path) {
        if meta.len() > MAX_BYTES {
            let previous = path.with_extension("log.1");
            let _ = fs::remove_file(&previous);
            let _ = fs::rename(&path, &previous);
            if let Ok(mut last) = LAST_OUTCOME.lock() {
                last.clear();
            }
        }
    }

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{line}");
    }
}

/// `yyyy-MM-dd HH:mm:ss` in local time is not available without a date crate, so the log
/// uses UTC, which is what a diagnostics file wants anyway.
fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant's days-to-civil algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
