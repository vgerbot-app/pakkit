//! Parsers for zypper's human-readable table output.
//!
//! The backend spawns everything with `LC_ALL=C`, so the labels matched
//! below are the English ones. The column layouts were confirmed against
//! `zypper 1.14.101` on openSUSE Tumbleweed (the fixtures under
//! `tests/fixtures/` are real captures, trimmed to a few rows):
//!
//! - `search` prints `S | Name | Summary | Type`; `pattern` rows share the
//!   table and are dropped, keeping `package` and `srcpackage`.
//! - `packages --installed-only` prints `S | Repository | Name | Version |
//!   Arch` — note there is **no `Type` column** in this listing.
//! - `list-updates` prints `S | Repository | Name | Current Version |
//!   Available Version | Arch`.
//! - `info` prints an `Information for package X:` banner, a dash rule, then
//!   aligned `Key  : value` lines; `Description` continues on indented lines
//!   (blank lines included — it is the last field) to the end of the output.
//!
//! Before any table, zypper may print a repository-refresh preamble
//! (`Retrieving repository '…' metadata […done]`, `Loading repository
//! data...`, …); those lines carry no `|` and fall out naturally, as do the
//! header row (first cell `S`) and the `---+---` divider.

use crate::backends::types::{ListRow, PackageDetail, SearchRow};

/// Split one line of a zypper table into trimmed cells.
///
/// `None` for anything that is not a data row: blank lines, the refresh
/// preamble (no `|`), the header row (first cell `S`) and divider rows
/// (every cell made of `-`/`+` only).
fn table_cells(line: &str) -> Option<Vec<&str>> {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.contains('|') {
        return None;
    }
    let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
    if cells[0] == "S" {
        return None;
    }
    if cells
        .iter()
        .all(|cell| cell.chars().all(|c| c == '-' || c == '+'))
    {
        return None;
    }
    Some(cells)
}

/// `search <query>`: `[S, Name, Summary, Type]`. Only installable items are
/// kept — `package` and `srcpackage`; `pattern` (and any future type) rows
/// are dropped. zypper's search table carries no version or repository, so
/// both stay empty; the summary becomes the one-line description.
pub fn parse_search(stdout: &str) -> Vec<SearchRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let cells = table_cells(line)?;
            let [_, name, summary, kind, ..] = cells.as_slice() else {
                return None;
            };
            if *kind != "package" && *kind != "srcpackage" {
                return None;
            }
            Some(SearchRow {
                name: (*name).to_owned(),
                id: (*name).to_owned(),
                version: String::new(),
                match_kind: None,
                source: String::new(),
                description: Some((*summary).to_owned()),
            })
        })
        .collect()
}

/// `packages --installed-only`: `[S, Repository, Name, Version, Arch]`.
pub fn parse_list(stdout: &str) -> Vec<ListRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let cells = table_cells(line)?;
            let [_, repository, name, version, ..] = cells.as_slice() else {
                return None;
            };
            Some(ListRow {
                name: (*name).to_owned(),
                id: (*name).to_owned(),
                version: (*version).to_owned(),
                available: None,
                source: (*repository).to_owned(),
            })
        })
        .collect()
}

/// `list-updates`: `[S, Repository, Name, Current Version, Available
/// Version, Arch]` — the current version fills `version`, the newer one
/// `available`.
pub fn parse_upgrades(stdout: &str) -> Vec<ListRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let cells = table_cells(line)?;
            let [_, repository, name, current, available, ..] = cells.as_slice() else {
                return None;
            };
            Some(ListRow {
                name: (*name).to_owned(),
                id: (*name).to_owned(),
                version: (*current).to_owned(),
                available: Some((*available).to_owned()),
                source: (*repository).to_owned(),
            })
        })
        .collect()
}

/// `info <id>`: aligned `Key  : value` lines under an
/// `Information for package X:` banner. `Description` is the last field and
/// continues on indented lines — paragraph breaks included — to the end of
/// the output; the one-line `Summary` leads the combined description, like
/// apt's `Description` field does.
pub fn parse_show(stdout: &str) -> PackageDetail {
    let mut detail = PackageDetail::default();
    let mut summary = String::new();
    let mut body: Vec<&str> = Vec::new();
    let mut in_description = false;
    for line in stdout.lines() {
        if in_description {
            // The description runs to the end of the output; only a fresh
            // unindented `Key : value` line would end it.
            if line.starts_with(char::is_whitespace) || line.trim().is_empty() {
                body.push(line.trim());
                continue;
            }
            in_description = false;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim_end() {
            "Name" => {
                detail.name = value.to_owned();
                detail.id = value.to_owned();
            }
            "Version" => detail.version = value.to_owned(),
            "Vendor" => detail.publisher = value.to_owned(),
            "Upstream URL" => detail.homepage = value.to_owned(),
            "Summary" => summary = value.to_owned(),
            "Description" => {
                if !value.is_empty() {
                    body.push(value);
                }
                in_description = true;
            }
            _ => {}
        }
    }
    while body.last() == Some(&"") {
        body.pop();
    }
    detail.description = match (summary.is_empty(), body.is_empty()) {
        (true, true) => String::new(),
        (false, true) => summary,
        (true, false) => body.join("\n"),
        (false, false) => format!("{summary}\n{}", body.join("\n")),
    };
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../../../tests/fixtures/zypper_search.txt");
    const LIST: &str = include_str!("../../../tests/fixtures/zypper_packages_installed.txt");
    const UPGRADES: &str = include_str!("../../../tests/fixtures/zypper_list_updates.txt");
    const INFO: &str = include_str!("../../../tests/fixtures/zypper_info.txt");

    #[test]
    fn search_skips_preamble_header_and_divider() {
        let rows = parse_search(SEARCH);
        assert!(
            rows.iter().all(|row| !row.id.contains("repository")),
            "preamble must not become rows: {rows:?}"
        );
        assert!(
            rows.iter()
                .all(|row| row.id != "Name" && !row.id.chars().all(|c| c == '-' || c == '+')),
            "header and divider must not become rows: {rows:?}"
        );
        // 6 packages + 1 srcpackage; the pattern row is dropped.
        assert_eq!(rows.len(), 7);
    }

    #[test]
    fn search_keeps_packages_and_srcpackages_but_drops_patterns() {
        let rows = parse_search(SEARCH);
        assert!(rows.iter().all(|row| row.id != "wsl_base"));
        assert_eq!(
            rows.iter().filter(|row| row.id == "sudo").count(),
            2,
            "the package and srcpackage rows both survive"
        );
    }

    #[test]
    fn search_maps_columns_to_the_row() {
        let rows = parse_search(SEARCH);
        let sudo = rows.iter().find(|row| row.id == "sudo").expect("sudo row");
        assert_eq!(sudo.name, "sudo");
        assert_eq!(sudo.version, "");
        assert_eq!(sudo.source, "");
        assert_eq!(
            sudo.description.as_deref(),
            Some("Execute some commands as root")
        );
        let devel = rows.iter().find(|row| row.id == "sudo-devel").expect("row");
        assert_eq!(
            devel.description.as_deref(),
            Some("Header files needed for sudo plugin development")
        );
    }

    #[test]
    fn list_maps_the_installed_table() {
        let rows = parse_list(LIST);
        assert_eq!(rows.len(), 10, "preamble/header/divider skipped: {rows:?}");

        let bash = &rows[1];
        assert_eq!(bash.name, "bash");
        assert_eq!(bash.id, "bash");
        assert_eq!(bash.version, "5.3.20-10.1");
        assert_eq!(bash.available, None);
        assert_eq!(bash.source, "openSUSE-Tumbleweed-Oss");

        // `i` (installed as a dependency) and `i+` (user-selected) rows are
        // both listed.
        let gpg2 = rows.iter().find(|row| row.id == "gpg2").expect("row");
        assert_eq!(gpg2.version, "2.5.24-1.1");
    }

    #[test]
    fn upgrades_swap_current_and_available_versions() {
        let rows = parse_upgrades(UPGRADES);
        assert_eq!(rows.len(), 1, "preamble/header/divider skipped: {rows:?}");

        let key = &rows[0];
        assert_eq!(key.name, "openSUSE-build-key");
        assert_eq!(key.id, "openSUSE-build-key");
        assert_eq!(key.version, "1.0-68.1");
        assert_eq!(key.available.as_deref(), Some("1.0-69.1"));
        assert_eq!(key.source, "openSUSE-Tumbleweed-Update");
    }

    #[test]
    fn show_maps_the_key_value_lines() {
        let detail = parse_show(INFO);
        assert_eq!(detail.name, "sudo");
        assert_eq!(detail.id, "sudo");
        assert_eq!(detail.version, "1.9.17p2-4.1");
        assert_eq!(detail.publisher, "openSUSE");
        assert_eq!(detail.homepage, "https://www.sudo.ws/");
    }

    #[test]
    fn show_combines_summary_and_the_indented_description() {
        let detail = parse_show(INFO);
        assert!(
            detail
                .description
                .starts_with("Execute some commands as root\nSudo is a command"),
            "summary leads the description: {:?}",
            detail.description
        );
        assert!(
            detail
                .description
                .contains("Sudo reads either /etc/sudoers"),
            "continuation lines kept"
        );
        assert!(
            detail.description.ends_with("with 'visudo'."),
            "no trailing blank lines: {:?}",
            detail.description
        );
        // The banner and the dash rule are not keys.
        assert!(!detail.description.contains("Information for package"));
    }

    #[test]
    fn empty_output_yields_nothing() {
        assert!(parse_search("").is_empty());
        assert!(parse_list("Loading repository data...\n").is_empty());
        assert!(parse_upgrades("").is_empty());
        assert_eq!(parse_show(""), PackageDetail::default());
    }
}
