//! Parsers for snap's columnar and `key: value` output.
//!
//! Everything runs with `LC_ALL=C` pinned. `snap find` / `snap list` /
//! `snap refresh --list` print fixed-width tables whose columns are
//! separated by runs of two or more spaces; empty Notes cells print as `-`,
//! and verified publishers carry a trailing `*`/`**` (older snapd used
//! `✓`/`✅`). `snap info` prints padded `key: value` pairs plus a
//! `description: |` literal block.

use crate::backends::types::{ListRow, PackageDetail, SearchRow};

/// Split one table row into columns on runs of 2+ spaces. Cuts only ever
/// land on ASCII spaces, so the byte walk stays on char boundaries.
fn columns(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b' ' && i + 1 < bytes.len() && bytes[i + 1] == b' ' {
            let field = line[start..i].trim();
            if !field.is_empty() {
                out.push(field);
            }
            while i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
            start = i;
        } else {
            i += 1;
        }
    }
    let field = line[start..].trim();
    if !field.is_empty() {
        out.push(field);
    }
    out
}

/// Strip the store's validation marks (`canonical**`, `snapcrafters*`,
/// `canonical✓`) from a publisher cell.
fn clean_publisher(publisher: &str) -> &str {
    publisher.trim_end_matches(['*', '✓', '✅'])
}

/// The data rows of a table: header line (`Name …`) dropped, rows with
/// fewer than `min` columns skipped (footer messages like
/// `All snaps up to date.` collapse to one column and fall out here).
fn table_rows(stdout: &str, min: usize) -> impl Iterator<Item = Vec<&str>> {
    stdout.lines().filter_map(move |line| {
        let cols = columns(line);
        if cols.len() < min || cols.first() == Some(&"Name") {
            return None;
        }
        Some(cols)
    })
}

/// `find <query>`: `Name  Version  Publisher  Notes  Summary`. The summary
/// rejoins any 2+ spaces it contained, so columns past Notes merge back.
pub fn parse_search(stdout: &str) -> Vec<SearchRow> {
    table_rows(stdout, 2)
        .map(|cols| {
            let summary = if cols.len() > 4 {
                Some(cols[4..].join(" "))
            } else {
                // A truncated row may carry the summary in the Notes slot.
                cols.get(3).filter(|cell| **cell != "-").map(|s| (*s).to_owned())
            };
            SearchRow {
                name: cols[0].to_owned(),
                id: cols[0].to_owned(),
                version: cols[1].to_owned(),
                match_kind: None,
                source: String::new(),
                description: summary,
            }
        })
        .collect()
}

/// `list`: `Name  Version  Rev  Tracking  Publisher  Notes`. Rev, tracking
/// and publisher are store internals the UI does not surface.
pub fn parse_list(stdout: &str) -> Vec<ListRow> {
    table_rows(stdout, 2)
        .map(|cols| ListRow {
            name: cols[0].to_owned(),
            id: cols[0].to_owned(),
            version: cols[1].to_owned(),
            available: None,
            source: String::new(),
        })
        .collect()
}

/// `refresh --list`: `Name  Version  Rev  Size  Publisher  Notes`. The
/// Version column is the *candidate*; the installed version is not shown,
/// so `version` stays empty — `refresh_updates` matches these rows against
/// `snap list` rows by id and only lifts `available`.
pub fn parse_upgrades(stdout: &str) -> Vec<ListRow> {
    table_rows(stdout, 2)
        .map(|cols| ListRow {
            name: cols[0].to_owned(),
            id: cols[0].to_owned(),
            version: String::new(),
            available: Some(cols[1].to_owned()),
            source: String::new(),
        })
        .collect()
}

/// `info <name>`: padded `key: value` pairs plus a `description: |`
/// literal block. The `installed:` line (`2.77.1 (28254) 46.9MB snapd`)
/// supplies the version for installed snaps; store-only snaps keep it
/// empty. The publisher's validation mark is stripped.
pub fn parse_show(stdout: &str) -> PackageDetail {
    let mut detail = PackageDetail::default();
    let mut summary = String::new();
    let mut description = String::new();
    let mut in_description = false;
    for line in stdout.lines() {
        if in_description {
            // The literal block is space-indented; the first unindented
            // line ends it.
            if line.starts_with(' ') || line.trim().is_empty() {
                if !description.is_empty() {
                    description.push('\n');
                }
                description.push_str(line.trim_end().trim_start());
                continue;
            }
            in_description = false;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "name" => {
                detail.name = value.to_owned();
                detail.id = value.to_owned();
            }
            "summary" => summary = value.to_owned(),
            "publisher" => detail.publisher = clean_publisher(value).to_owned(),
            "contact" => detail.publisher_url = value.to_owned(),
            "store-url" => detail.homepage = value.to_owned(),
            "license" => detail.license = value.to_owned(),
            "description" if value == "|" => in_description = true,
            "description" => description = value.to_owned(),
            "installed" => {
                // `2.77.1 (28254) 46.9MB snapd` — first token is the version.
                detail.version = value.split_whitespace().next().unwrap_or_default().to_owned();
            }
            _ => {}
        }
    }
    detail.description = if summary.is_empty() {
        description
    } else if description.is_empty() {
        summary
    } else {
        format!("{summary}\n\n{description}")
    };
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIND: &str = include_str!("../../../tests/fixtures/snap_find.txt");
    const LIST: &str = include_str!("../../../tests/fixtures/snap_list.txt");
    const REFRESH_LIST: &str = include_str!("../../../tests/fixtures/snap_refresh_list.txt");
    const INFO: &str = include_str!("../../../tests/fixtures/snap_info.txt");

    #[test]
    fn find_reads_rows_and_skips_the_header() {
        let rows = parse_search(FIND);
        assert_eq!(rows.len(), 5);

        let hello = rows.iter().find(|row| row.id == "hello").expect("row");
        assert_eq!(hello.version, "2.10");
        assert_eq!(
            hello.description.as_deref(),
            Some("GNU Hello, the \"hello world\" snap")
        );

        // Summaries containing single spaces stay one description.
        let nginx = rows.iter().find(|row| row.id == "hello-nginx").expect("row");
        assert!(
            nginx
                .description
                .as_deref()
                .is_some_and(|d| d.contains("listening on port 8080"))
        );
    }

    #[test]
    fn list_reads_versions_past_wide_columns() {
        let rows = parse_list(LIST);
        assert_eq!(rows.len(), 6);

        let firefox = rows.iter().find(|row| row.id == "firefox").expect("row");
        assert_eq!(firefox.version, "158.0-2");
        assert_eq!(firefox.available, None);

        // The truncated tracking column (`latest/stable/…`) must not shift
        // the version parse.
        let theme = rows
            .iter()
            .find(|row| row.id == "gtk-theme-breeze")
            .expect("row");
        assert_eq!(theme.version, "1.5");
    }

    #[test]
    fn refresh_list_maps_the_candidate_version() {
        let rows = parse_upgrades(REFRESH_LIST);
        assert_eq!(rows.len(), 1);
        let firefox = &rows[0];
        assert_eq!(firefox.id, "firefox");
        assert_eq!(firefox.version, "");
        assert_eq!(firefox.available.as_deref(), Some("158.0.1-1"));
    }

    #[test]
    fn an_up_to_date_refresh_lists_nothing() {
        assert!(parse_upgrades("All snaps up to date.\n").is_empty());
    }

    #[test]
    fn publisher_marks_are_stripped() {
        assert_eq!(clean_publisher("canonical**"), "canonical");
        assert_eq!(clean_publisher("snapcrafters*"), "snapcrafters");
        assert_eq!(clean_publisher("canonical✓"), "canonical");
        assert_eq!(clean_publisher("someone"), "someone");
    }

    #[test]
    fn info_reads_pairs_the_literal_block_and_the_installed_version() {
        let detail = parse_show(INFO);
        assert_eq!(detail.name, "snapd");
        assert_eq!(detail.id, "snapd");
        assert_eq!(detail.publisher, "Canonical");
        assert_eq!(detail.license, "GPL-3.0");
        assert_eq!(detail.version, "2.77.1");
        assert!(
            detail
                .description
                .starts_with("Daemon and tooling that enable snap packages")
        );
        assert!(detail.description.contains("'universal' packages"));
        assert!(detail.description.contains("Start with 'snap list'"));
        // The block ends before the unindented keys that follow it.
        assert!(!detail.description.contains("tracking"));
    }
}
