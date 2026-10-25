//! Operation logs.
//!
//! Every install / upgrade / uninstall the app runs is appended to a per-day
//! file under `dirs::data_dir()/pakkit/logs/` (`~/.local/share/pakkit/logs`
//! on Linux). Files are named `pakkit-YYYYMMDD.log`; anything dated before
//! today is pruned on the next write, giving the 1-day retention the
//! settings screen advertises. All I/O is best-effort — logging must never
//! crash the app, mirroring `config.rs`.

use std::path::{Path, PathBuf};

/// The directory log files live in, falling back to the config dir on
/// platforms without a data dir.
pub fn log_dir() -> Option<PathBuf> {
    dirs::data_dir()
        .or_else(dirs::config_dir)
        .map(|dir| dir.join("pakkit").join("logs"))
}

/// Today's log file — the path the settings screen shows and opens.
pub fn current_log_file() -> Option<PathBuf> {
    Some(log_dir()?.join(file_name(&today_stamp())))
}

/// Append one record to today's log, pruning expired files first.
///
/// `event` is a short machine-ish tag (`start` / `done` / `error`);
/// `message` may span lines, in which case only the first carries the
/// timestamp prefix and the rest are indented to read as one record.
pub fn append(event: &str, message: &str) {
    let Some(dir) = log_dir() else { return };
    prune_in(&dir);
    append_to(&dir, event, message);
}

/// Open a log file with the OS default handler.
pub fn open_with_default_app(path: &Path) {
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .spawn();
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let result: std::io::Result<std::process::Child> =
        Err(std::io::Error::other("no opener on this platform"));
    let _ = result;
}

fn file_name(date: &str) -> String {
    format!("pakkit-{date}.log")
}

fn append_to(dir: &Path, event: &str, message: &str) {
    use std::io::Write as _;
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(file_name(&today_stamp())))
    else {
        return;
    };
    let time = now_time();
    let mut lines = message.lines();
    if let Some(first) = lines.next() {
        let _ = writeln!(file, "[{time}] [{event}] {first}");
        for line in lines {
            let _ = writeln!(file, "    {line}");
        }
    }
}

/// Delete every dated log older than today. The `YYYYMMDD` names compare
/// lexicographically, so this is a plain string compare.
fn prune_in(dir: &Path) {
    let today = today_stamp();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(date) = name
            .strip_prefix("pakkit-")
            .and_then(|rest| rest.strip_suffix(".log"))
        else {
            continue;
        };
        if date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()) && date < today.as_str() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// `YYYYMMDD` for the current instant, without a date crate.
fn today_stamp() -> String {
    let days = unix_now() / 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}{month:02}{day:02}")
}

/// `HH:MM:SS` of the current instant (UTC), as the log line prefix.
fn now_time() -> String {
    let secs = unix_now() % 86_400;
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Days since the epoch → `(year, month, day)`, Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pakkit-log-test-{}-{}",
            std::process::id(),
            tag
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn known_dates_convert() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_926), (2024, 7, 22));
        // 2000-02-29: a leap day the naive math often misses.
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn append_creates_todays_file_and_indents_continuation_lines() {
        let dir = temp_dir("append");
        append_to(&dir, "error", "first line\nsecond line");

        let path = dir.join(file_name(&today_stamp()));
        let text = std::fs::read_to_string(&path).expect("the log file exists");
        assert!(text.contains("[error] first line"), "got: {text}");
        assert!(text.contains("    second line"), "got: {text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_removes_only_older_dated_logs() {
        let dir = temp_dir("prune");
        let old = dir.join("pakkit-20200101.log");
        let today = dir.join(file_name(&today_stamp()));
        let unrelated = dir.join("settings.json");
        let malformed = dir.join("pakkit-latest.log");
        for path in [&old, &today, &unrelated, &malformed] {
            std::fs::write(path, "x").expect("seed file");
        }

        prune_in(&dir);

        assert!(!old.exists(), "yesterday's log is gone");
        assert!(today.exists(), "today's log stays");
        assert!(unrelated.exists(), "non-log files stay");
        assert!(malformed.exists(), "non-dated names stay");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_dir_prunes_to_nothing_without_panicking() {
        prune_in(Path::new("/definitely/not/a/real/dir"));
    }
}
