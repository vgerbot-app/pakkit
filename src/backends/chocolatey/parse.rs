//! Parsers for Chocolatey's pipe-delimited and `key: value` output.
//!
//! `--limit-output` (`-r`) is the stable surface: `search` and `list` print
//! `name|version`, `outdated` prints `name|current|available|pinned`, and
//! `source list` prints `name|url|disabled|…`. `choco info` has no useful
//! pipe form, so its human output is parsed instead: a `Chocolatey v…`
//! banner, one `<id> <version>` line, then `Key: value` pairs where
//! `Description:` and `Release Notes:` may run onto the following lines up
//! to a blank line.

use crate::backends::types::{ListRow, PackageDetail, SearchRow};
use crate::model::Source;

/// Split one pipe-delimited row, tolerating a trailing newline.
fn fields(line: &str) -> Vec<&str> {
    line.trim_end().split('|').map(str::trim).collect()
}

/// The data rows of a `--limit-output` listing: pipe rows with at least
/// `min` fields; banner and prose lines fall out here.
fn pipe_rows(stdout: &str, min: usize) -> impl Iterator<Item = Vec<&str>> {
    stdout
        .lines()
        .map(fields)
        .filter(move |cols| cols.len() >= min && !cols[0].is_empty())
}

/// `search <query> -r`: `name|version`. choco's id *is* the name; the pipe
/// form carries no summary, so `description` stays `None`.
pub fn parse_search(stdout: &str) -> Vec<SearchRow> {
    pipe_rows(stdout, 2)
        .map(|cols| SearchRow {
            name: cols[0].to_owned(),
            id: cols[0].to_owned(),
            version: cols[1].to_owned(),
            match_kind: None,
            source: String::new(),
            description: None,
        })
        .collect()
}

/// `list -r` (v2: installed only): `name|version`.
pub fn parse_list(stdout: &str) -> Vec<ListRow> {
    pipe_rows(stdout, 2)
        .map(|cols| ListRow {
            name: cols[0].to_owned(),
            id: cols[0].to_owned(),
            version: cols[1].to_owned(),
            available: None,
            source: String::new(),
        })
        .collect()
}

/// `outdated -r`: `name|current|available|pinned`. Pinned packages never
/// move under `upgrade all`, so listing them as updatable would queue work
/// that cannot run; they are dropped here.
pub fn parse_upgrades(stdout: &str) -> Vec<ListRow> {
    pipe_rows(stdout, 4)
        .filter(|cols| !cols[3].eq_ignore_ascii_case("true"))
        .map(|cols| ListRow {
            name: cols[0].to_owned(),
            id: cols[0].to_owned(),
            version: cols[1].to_owned(),
            available: Some(cols[2].to_owned()),
            source: String::new(),
        })
        .collect()
}

/// `source list -r`: `name|url|disabled|…`. A disabled feed maps to
/// unverified; extra trailing fields (self-service, admin-only, priority)
/// are store internals the UI does not surface.
pub fn parse_source_list(stdout: &str) -> Vec<Source> {
    pipe_rows(stdout, 2)
        .map(|cols| Source {
            name: cols[0].to_owned(),
            url: cols[1].to_owned(),
            ty: "chocolatey".to_owned(),
            trusted: !cols.get(2).is_some_and(|d| d.eq_ignore_ascii_case("true")),
            explicit: true,
            packages: None,
            updated: None,
        })
        .collect()
}

/// `info <id>` (human form): banner, `<id> <version>` line, then
/// `Key: value` pairs. `Description:` / `Release Notes:` values continue on
/// following lines until a blank line; unknown keys are ignored.
pub fn parse_show(stdout: &str) -> PackageDetail {
    let mut detail = PackageDetail::default();
    let mut summary = String::new();
    // Which multi-line field the continuation lines feed, if any.
    let mut description_block = false;
    let mut release_notes_block = false;
    for line in stdout.lines() {
        let line = line.trim_end();
        if description_block || release_notes_block {
            if line.trim().is_empty() {
                description_block = false;
                release_notes_block = false;
            } else {
                let field = if description_block {
                    &mut detail.description
                } else {
                    &mut detail.release_notes
                };
                if !field.is_empty() {
                    field.push('\n');
                }
                field.push_str(line.trim());
                continue;
            }
        }
        if line.trim().is_empty() || line.starts_with("Chocolatey ") {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            // `<id> <version>` — the one line with no colon.
            let mut parts = line.split_whitespace();
            if let (Some(id), Some(version)) = (parts.next(), parts.next())
                && detail.id.is_empty()
            {
                detail.id = id.to_owned();
                detail.name = id.to_owned();
                detail.version = version.to_owned();
            }
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Title" => detail.name = value.to_owned(),
            "Package url" | "Package Url" => detail.homepage = value.to_owned(),
            "Tags" => detail.tags = value.split_whitespace().map(str::to_owned).collect(),
            "Summary" => summary = value.to_owned(),
            "Description" => {
                detail.description = value.to_owned();
                description_block = true;
            }
            "Release Notes" => {
                detail.release_notes = value.to_owned();
                release_notes_block = true;
            }
            _ => {}
        }
    }
    if detail.description.is_empty() {
        detail.description = summary;
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../../../tests/fixtures/choco_search.txt");
    const LIST: &str = include_str!("../../../tests/fixtures/choco_list.txt");
    const OUTDATED: &str = include_str!("../../../tests/fixtures/choco_outdated.txt");
    const INFO: &str = include_str!("../../../tests/fixtures/choco_info.txt");
    const SOURCES: &str = include_str!("../../../tests/fixtures/choco_source_list.txt");

    #[test]
    fn search_reads_pipe_rows_and_skips_prose() {
        let rows = parse_search(SEARCH);
        assert_eq!(rows.len(), 5);
        let git = rows.iter().find(|row| row.id == "git").expect("row");
        assert_eq!(git.version, "2.45.2");
        assert_eq!(git.description, None);
    }

    #[test]
    fn list_reads_installed_versions() {
        let rows = parse_list(LIST);
        assert_eq!(rows.len(), 5);
        let vscode = rows.iter().find(|row| row.id == "vscode").expect("row");
        assert_eq!(vscode.version, "1.90.2");
        assert_eq!(vscode.available, None);
    }

    #[test]
    fn outdated_maps_the_candidate_and_drops_pinned_rows() {
        let rows = parse_upgrades(OUTDATED);
        assert_eq!(rows.len(), 2);
        let git = rows.iter().find(|row| row.id == "git").expect("row");
        assert_eq!(git.version, "2.44.0");
        assert_eq!(git.available.as_deref(), Some("2.45.2"));
        // `nodejs` is pinned and must not be queued for `upgrade all`.
        assert!(!rows.iter().any(|row| row.id == "nodejs"));
    }

    #[test]
    fn an_up_to_date_outdated_lists_nothing() {
        assert!(parse_upgrades("").is_empty());
        assert!(parse_upgrades("Chocolatey has determined 0 packages are outdated.\n").is_empty());
    }

    #[test]
    fn source_list_marks_disabled_feeds_unverified() {
        let sources = parse_source_list(SOURCES);
        assert_eq!(sources.len(), 2);
        let community = &sources[0];
        assert_eq!(community.name, "chocolatey");
        assert_eq!(community.url, "https://community.chocolatey.org/api/v2/");
        assert!(community.trusted);
        assert!(!sources[1].trusted);
    }

    #[test]
    fn info_reads_the_banner_the_pair_line_and_the_blocks() {
        let detail = parse_show(INFO);
        assert_eq!(detail.id, "git");
        assert_eq!(detail.name, "Git");
        assert_eq!(detail.version, "2.45.2");
        assert_eq!(
            detail.homepage,
            "https://community.chocolatey.org/packages/git"
        );
        assert_eq!(
            detail.tags,
            vec!["git", "vcs", "version-control", "dvcs", "admin"]
        );
        assert!(
            detail
                .description
                .starts_with("Git for Windows focuses on offering a lightweight")
        );
        // The wrapped continuation line joins the description.
        assert!(
            detail
                .description
                .contains("experienced Git users and novices alike")
        );
        assert!(
            detail
                .release_notes
                .contains("github.com/git-for-windows/git/releases")
        );
        // The blank line ends the block before later keys.
        assert!(!detail.description.contains("Published"));
    }
}
