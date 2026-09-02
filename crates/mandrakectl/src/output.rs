//! Human tables and key/value output. JSON output is the raw response.

use std::io::{IsTerminal, Write};

use mandrake_core::Timestamp;
use serde_json::Value;

/// Whether to print JSON: `--json`, or stdout is not a terminal.
pub fn json_wanted(flag: bool) -> bool {
    flag || !std::io::stdout().is_terminal()
}

/// Pretty JSON to stdout.
pub fn json(value: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = serde_json::to_writer_pretty(&mut out, value);
    let _ = out.write_all(b"\n");
}

/// A padded table.
pub fn table(headers: &[&str], rows: &[Vec<String>]) {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if let Some(w) = widths.get_mut(i) {
                *w = (*w).max(cell.chars().count());
            }
        }
    }
    let mut out = std::io::stdout().lock();
    let line = |cells: Vec<String>| {
        let mut s = String::new();
        for (i, cell) in cells.iter().enumerate() {
            if i > 0 {
                s.push_str("  ");
            }
            let width = widths.get(i).copied().unwrap_or(0);
            let pad = width.saturating_sub(cell.chars().count());
            s.push_str(cell);
            if i + 1 < cells.len() {
                s.push_str(&" ".repeat(pad));
            }
        }
        s
    };
    let _ = writeln!(
        out,
        "{}",
        line(headers.iter().map(|h| (*h).to_owned()).collect())
    );
    for row in rows {
        let _ = writeln!(out, "{}", line(row.clone()));
    }
}

/// Aligned `key: value` lines.
pub fn kv(pairs: &[(&str, String)]) {
    let width = pairs.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    let mut out = std::io::stdout().lock();
    for (k, v) in pairs {
        let _ = writeln!(out, "{k:<width$}  {v}");
    }
}

/// A timestamp or `-`.
pub fn ts(t: Option<Timestamp>) -> String {
    t.map_or_else(|| "-".to_owned(), Timestamp::to_rfc3339)
}

/// A string or `-`.
pub fn opt(s: Option<&str>) -> String {
    s.map_or_else(|| "-".to_owned(), str::to_owned)
}

/// Bytes as a human size.
#[allow(clippy::cast_precision_loss)] // display only
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Seconds as `3d 4h 5m`.
pub fn duration(secs: u64) -> String {
    let (d, rem) = (secs / 86_400, secs % 86_400);
    let (h, rem) = (rem / 3600, rem % 3600);
    let m = rem / 60;
    match (d, h) {
        (0, 0) => format!("{m}m"),
        (0, _) => format!("{h}h {m}m"),
        _ => format!("{d}d {h}h {m}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_durations() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(1536), "1.5 KiB");
        assert_eq!(size(3 * 1024 * 1024 * 1024), "3.0 GiB");
        assert_eq!(duration(59), "0m");
        assert_eq!(duration(3_700), "1h 1m");
        assert_eq!(duration(90_061), "1d 1h 1m");
    }
}
