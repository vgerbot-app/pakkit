//! Parsers for apt's human-oriented output.
//!
//! The backend spawns everything with `LC_ALL=C`, so the labels matched
//! below are the English ones. `apt list`/`apt search` share a row shape —
//! `name/suite[,suite…] version arch [marker]` — tokenized once by
//! [`parse_apt_line`]; header lines (`Listing...`, `Sorting...`,
//! `Full Text Search...`) carry no `/` and fall out naturally. `apt show`
//! prints `Key: value` stanzas separated by blank lines, of which only the
//! first is read.

use crate::backends::types::{ListRow, PackageDetail, SearchRow};

/// One tokenized `apt list`-family row.
struct AptLine<'a> {
    id: &'a str,
    suites: &'a str,
    version: &'a str,
    /// Everything from the `[` marker on, e.g. `[installed,upgradable to: 1.2]`.
    bracket: Option<&'a str>,
}

/// Tokenize one row; lines without a `name/suites` first token (headers,
/// blanks) are not package rows.
fn parse_apt_line(line: &str) -> Option<AptLine<'_>> {
    let mut tokens = line.split_whitespace();
    let (id, suites) = tokens.next()?.split_once('/')?;
    if id.is_empty() {
        return None;
    }
    let version = tokens.next().unwrap_or_default();
    // The arch token is skipped; the marker may span several tokens
    // (`[upgradable from: 1.2]`), so it is located by offset instead.
    let bracket = line.find('[').map(|ix| &line[ix..]);
    Some(AptLine {
        id,
        suites,
        version,
        bracket,
    })
}

/// The suites a row belongs to, for display: the trailing `,now` membership
/// marker is dropped, and a bare `now` (a locally installed deb with no repo)
/// leaves nothing at all.
fn display_suites(suites: &str) -> &str {
    let stripped = suites.strip_suffix(",now").unwrap_or(suites);
    if stripped == "now" { "" } else { stripped }
}

/// The new version in an `[installed,upgradable to: X]` marker.
fn upgradable_to(bracket: &str) -> Option<&str> {
    bracket
        .strip_prefix("[installed,upgradable to: ")?
        .strip_suffix(']')
}

/// The installed version in an `[upgradable from: X]` marker.
fn upgradable_from(bracket: &str) -> Option<&str> {
    bracket
        .strip_prefix("[upgradable from: ")?
        .strip_suffix(']')
}

/// `search <query>`: blank-separated blocks of a
/// `name/suites version arch [marker]` line followed by space-indented
/// description lines. Rows without a description line keep `None`.
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
        let Some(parsed) = parse_apt_line(line) else {
            continue;
        };
        rows.push(SearchRow {
            name: parsed.id.to_owned(),
            id: parsed.id.to_owned(),
            version: parsed.version.to_owned(),
            match_kind: None,
            source: display_suites(parsed.suites).to_owned(),
            description: None,
        });
    }
    rows
}

/// `list --installed`; an `[installed,upgradable to: X]` marker doubles as
/// the row's available version.
pub fn parse_list(stdout: &str) -> Vec<ListRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let parsed = parse_apt_line(line)?;
            Some(ListRow {
                name: parsed.id.to_owned(),
                id: parsed.id.to_owned(),
                version: parsed.version.to_owned(),
                available: parsed.bracket.and_then(upgradable_to).map(str::to_owned),
                source: display_suites(parsed.suites).to_owned(),
            })
        })
        .collect()
}

/// `list --upgradable`: the token version is the *new* one, so it moves to
/// `available` and the marker's `from:` version fills `version`.
pub fn parse_upgrades(stdout: &str) -> Vec<ListRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let parsed = parse_apt_line(line)?;
            let installed = parsed.bracket.and_then(upgradable_from).unwrap_or_default();
            Some(ListRow {
                name: parsed.id.to_owned(),
                id: parsed.id.to_owned(),
                version: installed.to_owned(),
                available: Some(parsed.version.to_owned()),
                source: display_suites(parsed.suites).to_owned(),
            })
        })
        .collect()
}

/// `show <id>`: the first `Key: value` stanza only — later stanzas are other
/// versions of the same package. `Description` continues on space-indented
/// lines until the stanza's blank line.
pub fn parse_show(stdout: &str) -> PackageDetail {
    let mut detail = PackageDetail::default();
    let mut in_description = false;
    for line in stdout.lines() {
        if line.trim().is_empty() {
            break;
        }
        if let Some(text) = line.strip_prefix(' ') {
            if in_description {
                if !detail.description.is_empty() {
                    detail.description.push('\n');
                }
                detail.description.push_str(text.trim_end());
            }
            continue;
        }
        in_description = false;
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key {
            "Package" => {
                detail.name = value.to_owned();
                detail.id = value.to_owned();
            }
            "Version" => detail.version = value.to_owned(),
            "Maintainer" => detail.publisher = value.to_owned(),
            "Homepage" => detail.homepage = value.to_owned(),
            "Description" => {
                detail.description = value.to_owned();
                in_description = true;
            }
            _ => {}
        }
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../../../tests/fixtures/apt_search.txt");
    const LIST: &str = include_str!("../../../tests/fixtures/apt_list_installed.txt");
    const UPGRADES: &str = include_str!("../../../tests/fixtures/apt_list_upgradable.txt");
    const SHOW: &str = include_str!("../../../tests/fixtures/apt_show.txt");

    #[test]
    fn search_skips_headers_and_reads_blocks() {
        let rows = parse_search(SEARCH);
        assert!(rows.len() >= 9, "headers must not become rows: {rows:?}");
        assert!(rows.iter().all(|row| !row.id.contains('.')));

        let cbindgen = rows.iter().find(|row| row.id == "cbindgen").expect("row");
        assert_eq!(cbindgen.name, "cbindgen");
        assert_eq!(cbindgen.version, "0.29.2-3");
        assert_eq!(cbindgen.source, "resolute");
        assert_eq!(
            cbindgen.description.as_deref(),
            Some("Generates C bindings from Rust code")
        );
    }

    #[test]
    fn search_tolerates_markers_missing_descriptions_and_cjk() {
        let rows = parse_search(SEARCH);

        // The installed firefox row carries an [installed,automatic] marker.
        let firefox = rows.iter().find(|row| row.id == "firefox").expect("row");
        assert_eq!(firefox.version, "1:1snap1-0ubuntu9.1");
        assert_eq!(firefox.source, "resolute-updates");

        // A block without a description line keeps None.
        let nodesc = rows.iter().find(|row| row.id == "nodescpkg").expect("row");
        assert_eq!(nodesc.description, None);

        let cjk = rows.iter().find(|row| row.id == "cjk-editor").expect("row");
        assert_eq!(cjk.description.as_deref(), Some("简单的文本编辑器"));
    }

    #[test]
    fn a_headers_only_search_matches_nothing() {
        assert!(parse_search("Sorting...\nFull Text Search...\n").is_empty());
    }

    #[test]
    fn list_reads_rows_and_local_debs_have_no_source() {
        let rows = parse_list(LIST);
        assert_eq!(rows.len(), 9);

        let service = &rows[2];
        assert_eq!(service.id, "accountsservice");
        assert_eq!(service.version, "23.13.9-8ubuntu5.2");
        assert_eq!(service.source, "resolute-updates,resolute-security");
        assert_eq!(service.available, None);

        let local = rows.iter().find(|row| row.id == "aionui").expect("row");
        assert_eq!(local.source, "");
        assert_eq!(local.available, None);
    }

    #[test]
    fn list_fills_available_from_the_marker() {
        let rows = parse_list(LIST);
        let chatgpt = rows.iter().find(|row| row.id == "chatgpt").expect("row");
        assert_eq!(chatgpt.version, "26.1013.40934");
        assert_eq!(chatgpt.available.as_deref(), Some("26.1016.61520"));
        assert_eq!(chatgpt.source, "");
    }

    #[test]
    fn upgrades_swap_the_versions() {
        let rows = parse_upgrades(UPGRADES);
        assert_eq!(rows.len(), 7);

        let cursor = rows.iter().find(|row| row.id == "cursor").expect("row");
        assert_eq!(cursor.version, "3.25.6-1792019953");
        assert_eq!(cursor.available.as_deref(), Some("3.26.16-1792577839"));
        assert_eq!(cursor.source, "unknown");

        let multi = rows
            .iter()
            .find(|row| row.id == "distro-info-data")
            .expect("row");
        assert_eq!(
            multi.source,
            "resolute-updates,resolute-updates,resolute-security,resolute-security"
        );
    }

    #[test]
    fn show_reads_the_first_stanza_only() {
        let detail = parse_show(SHOW);
        assert_eq!(detail.name, "curl");
        assert_eq!(detail.id, "curl");
        assert_eq!(detail.version, "8.18.0-1ubuntu2.8");
        assert_eq!(
            detail.publisher,
            "Ubuntu Developers <ubuntu-devel-discuss@lists.ubuntu.com>"
        );
        assert_eq!(detail.homepage, "https://curl.se/");
        assert!(detail.description.starts_with("command line tool"));
        assert!(detail.description.contains('\n'), "continuation lines kept");
        assert!(!detail.description.contains("older stanza"));
    }
}
