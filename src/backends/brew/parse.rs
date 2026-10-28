//! Pure parsers for brew's human-readable output.
//!
//! Every command here is captured with stdout piped, so brew falls back to
//! its non-TTY shapes: one name per line, no column padding, no colour. The
//! fixtures under `tests/fixtures/brew_*.txt` follow brew 4.x's documented
//! output formats — the `homebrew/brew` container pull was unavailable at
//! authoring time, so they were written from the documented forms rather
//! than captured from a live `brew`.
//!
//! `search` splits its hits into `==> Formulae` / `==> Casks` sections;
//! cask-section rows are id-prefixed here so search results line up with
//! the installed/upgrade listings the composites prefix. `list --versions`
//! and `outdated --verbose` share one shape across the formula and cask
//! variants (`--cask` only changes which universe is listed), so one parser
//! serves both and the caller applies the prefix. `info` differs between
//! the universes: a formula header carries `stable <ver>`, a cask header a
//! bare version token, and a cask's prose description lives in its `==>
//! Description` section rather than on the line after the header.

use super::{CASK_ID_PREFIX, CASK_SOURCE, FORMULA_SOURCE};
use crate::backends::types::{ListRow, PackageDetail, SearchRow};

/// `search <query>`: `==> Formulae` / `==> Casks` section headers, then one
/// bare name per line. Cask-section rows get the `cask/` id prefix and the
/// `"cask"` source; formula rows the `"formula"` source. The plain listing
/// carries neither versions nor summaries, so `version` stays empty and
/// `description` stays `None`.
pub fn parse_search(stdout: &str) -> Vec<SearchRow> {
    let mut rows = Vec::new();
    let mut cask_section = false;
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line.strip_prefix("==>") {
            cask_section = header.trim().eq_ignore_ascii_case("casks");
            continue;
        }
        let (id, source) = if cask_section {
            (format!("{CASK_ID_PREFIX}{line}"), CASK_SOURCE)
        } else {
            (line.to_owned(), FORMULA_SOURCE)
        };
        rows.push(SearchRow {
            name: line.to_owned(),
            id,
            version: String::new(),
            match_kind: None,
            source: source.to_owned(),
            description: None,
        });
    }
    rows
}

/// `list --versions` (and `list --cask --versions`): `name v1 [v2 …]`, one
/// package per line. The first whitespace token is the name, the second the
/// version the UI shows; extra tokens are the other versions still kegged
/// on disk and carry no meaning here. The cask/formula distinction is
/// applied by the caller, so `id` and `source` stay unprefixed/empty.
pub fn parse_list(stdout: &str) -> Vec<ListRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut tokens = line.split_whitespace();
            let name = tokens.next()?;
            let version = tokens.next()?;
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

/// `outdated --verbose` (and `outdated --cask --verbose`):
/// `name (installed) < candidate`, or — for casks that track upstream
/// `latest` — `name (installed) != latest`. The relation token (`<`/`!=`)
/// is ignored; token four is the available version either way.
pub fn parse_upgrades(stdout: &str) -> Vec<ListRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut tokens = line.split_whitespace();
            let name = tokens.next()?;
            let installed = tokens.next()?.strip_prefix('(')?.strip_suffix(')')?;
            let _relation = tokens.next()?;
            let candidate = tokens.next()?;
            Some(ListRow {
                name: name.to_owned(),
                id: name.to_owned(),
                version: installed.to_owned(),
                available: Some(candidate.to_owned()),
                source: String::new(),
            })
        })
        .collect()
}

/// `info <id>` / `info --cask <name>`, kept to name/id/version/description/
/// homepage. The header line is `==> <name>: <rest>`; a formula's `rest`
/// carries `stable <ver>` (the token after `stable`), a cask's a bare
/// version token. The homepage is the first later line starting with
/// `http`. The description is the first later content line: brew's status
/// lines (`Installed`, `Not installed`, `From: …`) are not descriptions,
/// and a cask's prose sits in its `==> Description` section while the
/// `==> Name` section's display name is skipped.
pub fn parse_show(stdout: &str) -> PackageDetail {
    let mut detail = PackageDetail::default();
    let mut past_header = false;
    // The `==>` section the current line belongs to; `None` while still in
    // the preamble between the package header and the first section header.
    let mut section: Option<String> = None;
    for line in stdout.lines() {
        let line = line.trim();
        if !past_header {
            let Some(rest) = line.strip_prefix("==> ") else {
                continue;
            };
            let Some((name, info)) = rest.split_once(':') else {
                continue;
            };
            detail.name = name.trim().to_owned();
            detail.id = detail.name.clone();
            let info = info.trim();
            let version_part = match info.split_once("stable ") {
                Some((_, after)) => after,
                None => info,
            };
            detail.version = version_part
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned();
            past_header = true;
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line.strip_prefix("==> ") {
            section = Some(header.trim().to_owned());
            continue;
        }
        if detail.homepage.is_empty() && line.starts_with("http") {
            detail.homepage = line.to_owned();
            continue;
        }
        let description_slot = section
            .as_deref()
            .is_none_or(|s| s.eq_ignore_ascii_case("description"));
        if detail.description.is_empty()
            && description_slot
            && !matches!(line, "Installed" | "Not installed")
            && !line.starts_with("From:")
        {
            detail.description = line.to_owned();
        }
        if !detail.homepage.is_empty() && !detail.description.is_empty() {
            break;
        }
    }
    detail
}

/// `tap`: one tap name (`owner/repo`) per line.
pub fn parse_tap_list(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../../../tests/fixtures/brew_search.txt");
    const LIST: &str = include_str!("../../../tests/fixtures/brew_list.txt");
    const LIST_CASK: &str = include_str!("../../../tests/fixtures/brew_list_cask.txt");
    const OUTDATED: &str = include_str!("../../../tests/fixtures/brew_outdated.txt");
    const OUTDATED_CASK: &str = include_str!("../../../tests/fixtures/brew_outdated_cask.txt");
    const INFO: &str = include_str!("../../../tests/fixtures/brew_info.txt");
    const INFO_CASK: &str = include_str!("../../../tests/fixtures/brew_info_cask.txt");
    const TAP: &str = include_str!("../../../tests/fixtures/brew_tap.txt");

    #[test]
    fn search_switches_sections_and_prefixes_casks() {
        let rows = parse_search(SEARCH);
        assert_eq!(rows.len(), 4);
        let formula = rows
            .iter()
            .find(|row| row.name == "firefoxpwa")
            .expect("row");
        assert_eq!(formula.id, "firefoxpwa");
        assert_eq!(formula.source, "formula");
        assert_eq!(formula.version, "");
        assert_eq!(formula.description, None);
        let cask = rows.iter().find(|row| row.name == "firefox").expect("row");
        assert_eq!(cask.id, "cask/firefox");
        assert_eq!(cask.source, "cask");
        // Every row after the Casks header is a cask.
        assert!(rows[1..].iter().all(|row| row.id.starts_with("cask/")));
    }

    #[test]
    fn search_with_no_sections_yields_nothing() {
        assert!(parse_search("").is_empty());
        assert!(parse_search("==> Formulae\n\n==> Casks\n").is_empty());
    }

    #[test]
    fn list_takes_the_first_two_tokens_of_each_line() {
        let rows = parse_list(LIST);
        assert_eq!(rows.len(), 5);
        let bash = rows.iter().find(|row| row.id == "bash").expect("row");
        assert_eq!(bash.version, "5.2.26");
        assert_eq!(bash.available, None);
        // Multi-version lines keep the first version token only.
        let git = rows.iter().find(|row| row.id == "git").expect("row");
        assert_eq!(git.version, "2.43.0");
    }

    #[test]
    fn the_cask_listing_parses_identically() {
        let rows = parse_list(LIST_CASK);
        assert_eq!(rows.len(), 3);
        let firefox = rows.iter().find(|row| row.id == "firefox").expect("row");
        assert_eq!(firefox.version, "121.0");
    }

    #[test]
    fn upgrades_read_the_installed_and_candidate_versions() {
        let rows = parse_upgrades(OUTDATED);
        assert_eq!(rows.len(), 3);
        let git = rows.iter().find(|row| row.id == "git").expect("row");
        assert_eq!(git.version, "2.43.0");
        assert_eq!(git.available.as_deref(), Some("2.43.2"));
    }

    #[test]
    fn cask_upgrades_accept_the_not_latest_form() {
        let rows = parse_upgrades(OUTDATED_CASK);
        assert_eq!(rows.len(), 2);
        let firefox = rows.iter().find(|row| row.id == "firefox").expect("row");
        assert_eq!(firefox.version, "120.0.1");
        assert_eq!(firefox.available.as_deref(), Some("121.0"));
        let raycast = rows.iter().find(|row| row.id == "raycast").expect("row");
        assert_eq!(raycast.available.as_deref(), Some("latest"));
    }

    #[test]
    fn formula_info_reads_stable_version_description_and_homepage() {
        let detail = parse_show(INFO);
        assert_eq!(detail.id, "bash");
        assert_eq!(detail.name, "bash");
        assert_eq!(detail.version, "5.2.26");
        assert_eq!(
            detail.description,
            "Bourne-Again SHell, a UNIX command interpreter"
        );
        assert_eq!(detail.homepage, "https://www.gnu.org/software/bash/");
    }

    #[test]
    fn cask_info_reads_the_bare_version_and_the_description_section() {
        let detail = parse_show(INFO_CASK);
        assert_eq!(detail.id, "firefox");
        assert_eq!(detail.version, "121.0");
        assert_eq!(detail.homepage, "https://www.mozilla.org/firefox/");
        // Neither the status lines nor the Name section's display name may
        // win over the real Description section.
        assert_eq!(detail.description, "Web browser");
    }

    #[test]
    fn tap_lists_one_name_per_line() {
        let taps = parse_tap_list(TAP);
        assert_eq!(
            taps,
            vec!["homebrew/cask", "homebrew/core", "homebrew/services"]
        );
        assert!(parse_tap_list("\n\n").is_empty());
    }
}
