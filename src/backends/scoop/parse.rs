//! Pure parsers for Scoop's human-readable output.
//!
//! Scoop is Windows-only, so these parsers target captured output rather
//! than a live CLI; the shapes come from Scoop's own sources
//! (github.com/ScoopInstaller/Scoop):
//!
//! - `scoop search` prints one section per bucket — a `'name' bucket:`
//!   header followed by indented `name (version)` rows, optionally suffixed
//!   with ` --> includes 'bin'` — as emitted by `libexec/scoop-search.ps1`
//!   before the PSObject-table refactor (commit 86e3efba, parent of #4997).
//! - `scoop list` prints an `Installed apps:` prologue, then a Format-Table
//!   with columns `Name Version Source Updated Info`; `Updated` renders as
//!   `yyyy-MM-dd HH:mm:ss` (view `ScoopApps` in
//!   `supporting/formats/ScoopTypes.Format.ps1xml`), so it contains a space
//!   and a row parser must never read past the third token.
//! - `scoop status` prints informational lines (`Scoop is up to date.`,
//!   `Everything is ok!`) plus, when apps are stale, the `ScoopStatus` table
//!   `Name / Installed Version / Latest Version / Missing Dependencies /
//!   Info` (same format file).
//! - `scoop info` emits a Format-List of `Key : value` pairs
//!   (`libexec/scoop-info.ps1`).
//! - `scoop bucket list` emits a Format-Table with columns
//!   `Name Source Updated Manifests` (`list_buckets` in `lib/buckets.ps1`).

use crate::backends::types::{ListRow, PackageDetail, SearchRow};

/// A table divider token is nothing but dashes (`----`).
fn is_divider(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|c| c == '-')
}

/// Informational lines `scoop status` prints around (or instead of) its
/// out-of-date table; they tokenize like data rows, so they are matched
/// explicitly instead of by shape.
fn is_status_noise(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("Scoop is up to date")
        || trimmed.starts_with("Scoop out of date")
        || trimmed.starts_with("Scoop bucket(s) out of date")
        || trimmed.starts_with("Everything is ok")
}

/// The rows of a whitespace-column table (Format-Table output): the header
/// (`Name` in the first column) and the all-dashes divider are dropped.
fn table_rows(stdout: &str) -> impl Iterator<Item = &str> {
    stdout.lines().filter(|line| {
        let mut tokens = line.split_whitespace();
        match tokens.next() {
            None => false,
            Some(first) => first != "Name" && !is_divider(first),
        }
    })
}

/// `search <query>`: per-bucket sections of indented `name (version)` rows.
/// Rows may carry a ` --> includes 'bin'` suffix; everything else (prose,
/// blank lines) is dropped.
pub fn parse_search(stdout: &str) -> Vec<SearchRow> {
    let mut rows = Vec::new();
    let mut bucket = String::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('\'')
            && let Some((name, tail)) = rest.split_once("' bucket:")
            && tail.trim().is_empty()
        {
            bucket = name.to_owned();
            continue;
        }
        // Data rows are indented under their bucket header.
        if !line.starts_with([' ', '\t']) {
            continue;
        }
        let Some((name, rest)) = trimmed.split_once(" (") else {
            continue;
        };
        let Some((version, _suffix)) = rest.split_once(')') else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        rows.push(SearchRow {
            name: name.to_owned(),
            id: name.to_owned(),
            version: version.to_owned(),
            match_kind: None,
            source: bucket.clone(),
            description: None,
        });
    }
    rows
}

/// `list`: `Name Version Source Updated Info`. The `Updated` timestamp
/// contains a space, so only the first three tokens of a row are read.
pub fn parse_list(stdout: &str) -> Vec<ListRow> {
    table_rows(stdout)
        .filter_map(|line| {
            let mut tokens = line.split_whitespace();
            let name = tokens.next()?;
            let version = tokens.next()?;
            let source = tokens.next()?;
            Some(ListRow {
                name: name.to_owned(),
                id: name.to_owned(),
                version: version.to_owned(),
                available: None,
                source: source.to_owned(),
            })
        })
        .collect()
}

/// `status`: `Name Installed Version Latest Version …`. The table carries
/// no bucket column, so `source` stays empty. The informational lines Scoop
/// prints when nothing is pending tokenize like rows, hence `is_status_noise`.
pub fn parse_upgrades(stdout: &str) -> Vec<ListRow> {
    table_rows(stdout)
        .filter(|line| !is_status_noise(line))
        .filter_map(|line| {
            let mut tokens = line.split_whitespace();
            let name = tokens.next()?;
            let installed = tokens.next()?;
            let latest = tokens.next()?;
            Some(ListRow {
                name: name.to_owned(),
                id: name.to_owned(),
                version: installed.to_owned(),
                available: Some(latest.to_owned()),
                source: String::new(),
            })
        })
        .collect()
}

/// `info <id>`: Format-List `Key : value` pairs. Values are split on the
/// first colon only, so URLs survive intact.
pub fn parse_show(stdout: &str) -> PackageDetail {
    let mut detail = PackageDetail::default();
    for line in stdout.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Name" => {
                detail.name = value.to_owned();
                detail.id = value.to_owned();
            }
            "Description" => detail.description = value.to_owned(),
            "Version" => detail.version = value.to_owned(),
            "Website" => detail.homepage = value.to_owned(),
            "License" => detail.license = value.to_owned(),
            _ => {}
        }
    }
    detail
}

/// `bucket list`: `Name Source Updated Manifests` → `(name, url)` pairs.
/// `Updated` contains a space, so only the first two tokens of a row are
/// read; the `Source` column is the bucket's git remote (or a local path
/// for non-git buckets).
pub fn parse_bucket_list(stdout: &str) -> Vec<(String, String)> {
    table_rows(stdout)
        .filter_map(|line| {
            let mut tokens = line.split_whitespace();
            let name = tokens.next()?.to_owned();
            let url = tokens.next().unwrap_or_default().to_owned();
            Some((name, url))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../../../tests/fixtures/scoop_search.txt");
    const LIST: &str = include_str!("../../../tests/fixtures/scoop_list.txt");
    const STATUS: &str = include_str!("../../../tests/fixtures/scoop_status.txt");
    const INFO: &str = include_str!("../../../tests/fixtures/scoop_info.txt");
    const BUCKETS: &str = include_str!("../../../tests/fixtures/scoop_bucket_list.txt");

    #[test]
    fn search_tracks_the_current_bucket_header() {
        let rows = parse_search(SEARCH);
        assert_eq!(rows.len(), 5);
        let git = rows.iter().find(|row| row.id == "git").expect("row");
        assert_eq!(git.version, "2.41.0.windows.3");
        assert_eq!(git.source, "main");
        assert_eq!(git.description, None);
        let gitkraken = rows.iter().find(|row| row.id == "gitkraken").expect("row");
        assert_eq!(gitkraken.source, "extras");
    }

    #[test]
    fn list_stops_at_the_source_column() {
        let rows = parse_list(LIST);
        assert_eq!(rows.len(), 3);
        let git = rows.iter().find(|row| row.id == "git").expect("row");
        assert_eq!(git.version, "2.41.0.windows.3");
        assert_eq!(git.source, "main");
        assert_eq!(git.available, None);
        // The Updated timestamp must never leak into the source column.
        assert!(!rows.iter().any(|row| row.source.contains(':')));
    }

    #[test]
    fn status_reads_pending_updates() {
        let rows = parse_upgrades(STATUS);
        assert_eq!(rows.len(), 2);
        let sevenzip = rows.iter().find(|row| row.id == "7zip").expect("row");
        assert_eq!(sevenzip.version, "23.00");
        assert_eq!(sevenzip.available.as_deref(), Some("23.01"));
    }

    #[test]
    fn an_up_to_date_status_lists_nothing() {
        assert!(parse_upgrades("").is_empty());
        assert!(parse_upgrades("Scoop is up to date.\nEverything is ok!\n").is_empty());
    }

    #[test]
    fn info_reads_the_key_value_pairs() {
        let detail = parse_show(INFO);
        assert_eq!(detail.id, "git");
        assert_eq!(detail.name, "git");
        assert_eq!(detail.version, "2.41.0.windows.3");
        assert_eq!(detail.homepage, "https://gitforwindows.org");
        assert_eq!(detail.license, "GPL-2.0-only");
        assert!(detail.description.contains("version control"));
    }

    #[test]
    fn bucket_list_reads_names_and_urls() {
        let buckets = parse_bucket_list(BUCKETS);
        assert_eq!(buckets.len(), 2);
        assert_eq!(
            buckets[0],
            (
                "main".to_owned(),
                "https://github.com/ScoopInstaller/Main.git".to_owned()
            )
        );
        assert_eq!(buckets[1].0, "extras");
    }
}
