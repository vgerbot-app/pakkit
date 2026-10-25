//! Parsers for pacman's and yay's human-oriented output.
//!
//! Everything runs with `LC_ALL=C` pinned, so the labels matched below are
//! the English ones. The shapes handled:
//!
//! - `-Ss` search blocks: a `repo/name version [marker]` line followed by
//!   space-indented description lines. yay's AUR rows share the shape with
//!   repo `aur` and a `(+votes popularity)` marker.
//! - `-Q` list: `name version` per line.
//! - `-Qmq`: one foreign (AUR) package name per line.
//! - `-Qu` upgrades: `name old-version -> new-version`.
//! - `-Si`/`-Qi` details: `Key            : value` stanzas.

use std::collections::HashSet;

use crate::backends::types::{ListRow, PackageDetail, SearchRow};

use super::AUR_ID_PREFIX;

/// One tokenized search row: `repo/name version [marker…]`. Everything
/// after the version token (`[installed]`, `(+5 0.42)`) is display noise
/// the parser drops.
struct PackageLine<'a> {
    repo: &'a str,
    name: &'a str,
    version: &'a str,
}

/// Tokenize one search row; lines without a `repo/name` first token are
/// not package rows.
fn parse_package_line(line: &str) -> Option<PackageLine<'_>> {
    let mut tokens = line.split_whitespace();
    let (repo, name) = tokens.next()?.split_once('/')?;
    if name.is_empty() {
        return None;
    }
    let version = tokens.next().unwrap_or_default();
    Some(PackageLine { repo, name, version })
}

/// `-Ss <query>`: a `repo/name version [marker]` line per package, each
/// followed by space-indented description lines. yay's `--aur` output uses
/// repo `aur`; those rows get the `aur/`-prefixed id the rest of the
/// backend routes on.
pub fn parse_search(stdout: &str) -> Vec<SearchRow> {
    let mut rows: Vec<SearchRow> = Vec::new();
    for line in stdout.lines() {
        if line.starts_with(' ') {
            let text = line.trim();
            if text.is_empty() {
                continue;
            }
            if let Some(row) = rows.last_mut() {
                match &mut row.description {
                    Some(description) => {
                        description.push(' ');
                        description.push_str(text);
                    }
                    None => row.description = Some(text.to_owned()),
                }
            }
            continue;
        }
        let Some(parsed) = parse_package_line(line) else {
            continue;
        };
        let (id, source) = if parsed.repo == "aur" {
            (
                format!("{AUR_ID_PREFIX}{}", parsed.name),
                "aur".to_owned(),
            )
        } else {
            (parsed.name.to_owned(), parsed.repo.to_owned())
        };
        rows.push(SearchRow {
            name: parsed.name.to_owned(),
            id,
            version: parsed.version.to_owned(),
            match_kind: None,
            source,
            description: None,
        });
    }
    rows
}

/// `-Q`: `name version` per line (names cannot contain spaces). The local
/// database records no repo, so `source` stays empty and the shell labels
/// the rows with the backend's cli name.
pub fn parse_list(stdout: &str) -> Vec<ListRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut tokens = line.split_whitespace();
            let name = tokens.next()?;
            let version = tokens.next().unwrap_or_default();
            Some(ListRow {
                name: name.to_owned(),
                id: name.to_owned(),
                version: version.to_owned(),
                available: None,
                source: String::new(),
            })
        })
        .collect()
}

/// `-Qmq`: one foreign (AUR) package name per line.
pub fn parse_foreign_names(stdout: &str) -> HashSet<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `-Qu`: `name old-version -> new-version` per line. Lines without the
/// arrow (warnings pacman sometimes prints to stdout) are skipped.
pub fn parse_upgrades(stdout: &str) -> Vec<ListRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let (left, right) = line.split_once(" -> ")?;
            let mut tokens = left.split_whitespace();
            let name = tokens.next()?;
            let old = tokens.next().unwrap_or_default();
            let new = right.split_whitespace().next().unwrap_or_default();
            Some(ListRow {
                name: name.to_owned(),
                id: name.to_owned(),
                version: old.to_owned(),
                available: Some(new.to_owned()),
                source: String::new(),
            })
        })
        .collect()
}

/// `-Si`/`-Qi`: `Key : value` stanzas (keys padded with spaces). The same
/// parser serves `yay -Si`, whose extra AUR keys (`AUR URL`, `Votes`, …)
/// simply match no arm.
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
            "Version" => detail.version = value.to_owned(),
            "Description" => detail.description = value.to_owned(),
            "URL" => detail.homepage = value.to_owned(),
            "Licenses" | "License" => detail.license = value.to_owned(),
            "Packager" | "Maintainer" => detail.publisher = value.to_owned(),
            _ => {}
        }
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../../../tests/fixtures/pacman_search.txt");
    const YAY_SEARCH: &str = include_str!("../../../tests/fixtures/yay_search.txt");
    const LIST: &str = include_str!("../../../tests/fixtures/pacman_list.txt");
    const FOREIGN: &str = include_str!("../../../tests/fixtures/pacman_foreign.txt");
    const UPGRADES: &str = include_str!("../../../tests/fixtures/pacman_upgrades.txt");
    const SHOW: &str = include_str!("../../../tests/fixtures/pacman_show.txt");
    const YAY_SHOW: &str = include_str!("../../../tests/fixtures/yay_show.txt");

    #[test]
    fn search_reads_blocks_and_drops_markers() {
        let rows = parse_search(SEARCH);
        assert_eq!(rows.len(), 3);

        let firefox = rows.iter().find(|row| row.name == "firefox").expect("row");
        assert_eq!(firefox.id, "firefox");
        assert_eq!(firefox.version, "145.0-1");
        assert_eq!(firefox.source, "extra");
        assert_eq!(
            firefox.description.as_deref(),
            Some("Fast, Private & Safe Web Browser")
        );

        let nodesc = rows.iter().find(|row| row.name == "curl").expect("row");
        assert_eq!(nodesc.version, "8.16.0-1");
        assert_eq!(nodesc.source, "core");
        assert_eq!(nodesc.description, None);
    }

    #[test]
    fn yay_search_rows_get_prefixed_ids() {
        let rows = parse_search(YAY_SEARCH);
        assert_eq!(rows.len(), 2);

        let nightly = rows
            .iter()
            .find(|row| row.name == "firefox-nightly")
            .expect("row");
        assert_eq!(nightly.id, "aur/firefox-nightly");
        assert_eq!(nightly.source, "aur");
        assert_eq!(nightly.version, "147.0a1.20251021-1");
        assert!(nightly.description.is_some());

        // The `(+votes popularity)` marker must not leak into the version.
        assert!(rows.iter().all(|row| !row.version.contains('(')));
    }

    #[test]
    fn list_reads_two_column_rows() {
        let rows = parse_list(LIST);
        assert_eq!(rows.len(), 3);
        let firefox = rows.iter().find(|row| row.id == "firefox").expect("row");
        assert_eq!(firefox.version, "145.0-1");
        assert_eq!(firefox.source, "");
        assert_eq!(firefox.available, None);
    }

    #[test]
    fn foreign_names_collect_into_a_set() {
        let names = parse_foreign_names(FOREIGN);
        assert_eq!(names.len(), 2);
        assert!(names.contains("cool-retro-term"));
        assert!(names.contains("firefox-nightly"));
    }

    #[test]
    fn upgrades_split_around_the_arrow_and_skip_junk() {
        let rows = parse_upgrades(UPGRADES);
        assert_eq!(rows.len(), 2, "the warning line must drop out");

        let firefox = rows.iter().find(|row| row.id == "firefox").expect("row");
        assert_eq!(firefox.version, "144.0-1");
        assert_eq!(firefox.available.as_deref(), Some("145.0-1"));
    }

    #[test]
    fn show_reads_the_stanza() {
        let detail = parse_show(SHOW);
        assert_eq!(detail.name, "firefox");
        assert_eq!(detail.id, "firefox");
        assert_eq!(detail.version, "145.0-1");
        assert_eq!(detail.description, "Fast, Private & Safe Web Browser");
        assert_eq!(detail.homepage, "https://www.mozilla.org/firefox/");
        assert_eq!(detail.license, "MPL-2.0");
        assert_eq!(detail.publisher, "Arch Linux <build@archlinux.org>");
    }

    #[test]
    fn yay_show_ignores_aur_specific_keys() {
        let detail = parse_show(YAY_SHOW);
        assert_eq!(detail.name, "firefox-nightly");
        assert_eq!(detail.version, "147.0a1.20251021-1");
        assert_eq!(detail.publisher, "someuser");
        // `AUR URL` must not overwrite `URL`'s homepage.
        assert_eq!(detail.homepage, "https://www.mozilla.org/firefox/");
    }
}
