//! Parsers for dnf's human-oriented output.
//!
//! The backend spawns everything with `LC_ALL=C`, so the labels matched
//! below are the English ones. Both dnf4 and dnf5 share the row shapes
//! parsed here:
//!
//! - `search` prints `name.arch : summary` rows under
//!   `==== … Matched: <term> ====` section headers (dnf5 collapses the
//!   headers to `Matched fields: …` lines and may drop the spaces around
//!   the colon — both fall out of the same rule: the text before the first
//!   `:` must be a single whitespace-free `name.arch` token).
//! - `list --installed` prints `name.arch version @repo` rows after an
//!   `Installed Packages` header.
//! - `check-update` prints `name.arch candidate-version repo` rows (no `@`)
//!   and exits 100 when any are present; a trailing `Obsoleting Packages`
//!   section is ignored.
//! - `info` prints aligned `Key : value` stanzas separated by blank lines,
//!   with multi-line values continued on `<padding> : text` lines; only the
//!   first stanza is read.
//!
//! Header lines (`Installed Packages`, `Last metadata expiration check: …`,
//! the `====` banners) never satisfy the row rules and are skipped.

use crate::backends::types::{ListRow, PackageDetail, SearchRow};

/// Split a `name.arch` token at the last dot. Both halves must be non-empty
/// (`nano.x86_64` → `("nano", "x86_64")`; `.x86_64` and `nano.` are not
/// package tokens).
fn split_name_arch(token: &str) -> Option<(&str, &str)> {
    let (name, arch) = token.rsplit_once('.')?;
    if name.is_empty() || arch.is_empty() {
        return None;
    }
    Some((name, arch))
}

/// One `dnf list`/`check-update` row: `name.arch version repo`. Row
/// detection is ≥3 whitespace-separated tokens with a `name.arch` first
/// token, which excludes the section headers and metadata-expiration lines.
fn parse_list_row(line: &str) -> Option<(&str, &str, &str)> {
    let mut tokens = line.split_whitespace();
    let nevra = tokens.next()?;
    let version = tokens.next()?;
    let repo = tokens.next()?;
    let (name, _arch) = split_name_arch(nevra)?;
    Some((name, version, repo))
}

/// `search <query>`: `name.arch : summary` rows. The summary becomes the
/// row description; dnf search output carries no version or repo, so those
/// stay empty.
pub fn parse_search(stdout: &str) -> Vec<SearchRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let (nevra, summary) = line.split_once(':')?;
            let token = nevra.trim();
            if token.contains(char::is_whitespace) {
                return None;
            }
            let (name, _arch) = split_name_arch(token)?;
            let summary = summary.trim();
            Some(SearchRow {
                name: name.to_owned(),
                id: name.to_owned(),
                version: String::new(),
                match_kind: None,
                source: String::new(),
                description: if summary.is_empty() {
                    None
                } else {
                    Some(summary.to_owned())
                },
            })
        })
        .collect()
}

/// `list --installed`: the repo token carries an `@` prefix (`@fedora`,
/// `@System`), stripped for display. `available` is always `None` — the
/// installed list does not report candidates.
pub fn parse_list(stdout: &str) -> Vec<ListRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let (name, version, repo) = parse_list_row(line)?;
            Some(ListRow {
                name: name.to_owned(),
                id: name.to_owned(),
                version: version.to_owned(),
                available: None,
                source: repo.strip_prefix('@').unwrap_or(repo).to_owned(),
            })
        })
        .collect()
}

/// `check-update`: the row version is the *candidate*, so it moves to
/// `available` and `version` stays empty. The trailing `Obsoleting
/// Packages` section (dnf4 prints one when an update replaces another
/// package) is not an upgrade list and ends the parse.
pub fn parse_upgrades(stdout: &str) -> Vec<ListRow> {
    let mut rows: Vec<ListRow> = Vec::new();
    for line in stdout.lines() {
        if line.split_whitespace().next() == Some("Obsoleting") {
            break;
        }
        let Some((name, candidate, repo)) = parse_list_row(line) else {
            continue;
        };
        rows.push(ListRow {
            name: name.to_owned(),
            id: name.to_owned(),
            version: String::new(),
            available: Some(candidate.to_owned()),
            source: repo.strip_prefix('@').unwrap_or(repo).to_owned(),
        });
    }
    rows
}

/// `info <id>`: the first `Key : value` stanza only — later stanzas are
/// other versions of the same package. The visible description is the
/// one-line `Summary` followed by the long `Description`, which continues
/// on space-indented lines (printed as `<padding> : text`) until the
/// stanza's blank line.
pub fn parse_show(stdout: &str) -> PackageDetail {
    let mut detail = PackageDetail::default();
    let mut summary = String::new();
    let mut description = String::new();
    let mut in_description = false;
    for line in stdout.lines() {
        if line.trim().is_empty() {
            break;
        }
        if line.starts_with(char::is_whitespace) {
            if in_description {
                // `<padding> : text`; the colon before the text is dnf's
                // continuation marker, not part of the prose.
                let text = match line.trim().strip_prefix(':') {
                    Some(rest) => rest.trim(),
                    None => line.trim(),
                };
                if !text.is_empty() {
                    if !description.is_empty() {
                        description.push('\n');
                    }
                    description.push_str(text);
                }
            }
            continue;
        }
        in_description = false;
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
            "Packager" => detail.publisher = value.to_owned(),
            "Vendor" => {
                if detail.publisher.is_empty() {
                    detail.publisher = value.to_owned();
                }
            }
            "URL" => detail.homepage = value.to_owned(),
            "License" => detail.license = value.to_owned(),
            "Summary" => summary = value.to_owned(),
            "Description" => {
                description = value.to_owned();
                in_description = true;
            }
            _ => {}
        }
    }
    detail.description = match (summary.is_empty(), description.is_empty()) {
        (false, false) => format!("{summary}\n{description}"),
        (false, true) => summary,
        (true, false) => description,
        (true, true) => String::new(),
    };
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../../../tests/fixtures/dnf_search.txt");
    const LIST: &str = include_str!("../../../tests/fixtures/dnf_list_installed.txt");
    const CHECK_UPDATE: &str = include_str!("../../../tests/fixtures/dnf_check_update.txt");
    const INFO: &str = include_str!("../../../tests/fixtures/dnf_info.txt");

    #[test]
    fn search_skips_headers_and_splits_name_arch() {
        let rows = parse_search(SEARCH);
        assert_eq!(rows.len(), 15, "headers must not become rows: {rows:?}");
        assert!(
            rows.iter()
                .all(|row| !row.id.contains(char::is_whitespace) && !row.id.contains('='))
        );

        let sudo = rows.iter().find(|row| row.id == "sudo").expect("row");
        assert_eq!(sudo.name, "sudo");
        assert_eq!(sudo.version, "");
        assert_eq!(sudo.source, "");
        assert_eq!(
            sudo.description.as_deref(),
            Some("Allows restricted root access for specified users")
        );

        // Both arches of sudo-devel collapse onto the same id once the
        // `.arch` segment is stripped.
        assert_eq!(
            rows.iter().filter(|row| row.id == "sudo-devel").count(),
            2
        );

        // A `+`-bearing rpm name keeps everything before the last dot.
        let plus = rows
            .iter()
            .find(|row| row.id == "rust-sudo+default-devel")
            .expect("row");
        assert_eq!(plus.name, "rust-sudo+default-devel");
    }

    #[test]
    fn search_tolerates_dnf5_spacing_and_matches_nothing_on_headers() {
        // dnf5 drops the spaces around the colon and prints `Matched
        // fields:` banners instead of the `====` sections.
        let rows = parse_search(
            "Matched fields: name, summary\n sudo.x86_64: Allows restricted root access\n",
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "sudo");

        assert!(parse_search("Last metadata expiration check: never\n").is_empty());
    }

    #[test]
    fn list_reads_rows_and_strips_the_repo_marker() {
        let rows = parse_list(LIST);
        assert_eq!(rows.len(), 11, "header must not become a row: {rows:?}");
        assert!(rows.iter().all(|row| row.available.is_none()));

        let bash = rows.iter().find(|row| row.id == "bash").expect("row");
        assert_eq!(bash.name, "bash");
        assert_eq!(bash.version, "5.2.26-3.fc40");
        assert_eq!(bash.source, "56c10ddcfae84956be9b699009cafa5b");

        let dnf = rows.iter().find(|row| row.id == "dnf").expect("row");
        assert_eq!(dnf.version, "4.23.0-1.fc40.1");
        assert_eq!(dnf.source, "b358bd3803b74c5ebcc5044135212d5a");
    }

    #[test]
    fn check_update_moves_the_candidate_to_available() {
        let rows = parse_upgrades(CHECK_UPDATE);
        assert_eq!(rows.len(), 3, "metadata line skipped: {rows:?}");

        let xz = rows.iter().find(|row| row.id == "xz").expect("row");
        assert_eq!(xz.name, "xz");
        assert_eq!(xz.version, "");
        assert_eq!(xz.available.as_deref(), Some("1:5.8.1-2.fc40"));
        assert_eq!(xz.source, "updates");

        let zstd = rows.iter().find(|row| row.id == "zstd").expect("row");
        assert_eq!(zstd.available.as_deref(), Some("1.5.7-1.fc40"));
    }

    #[test]
    fn check_update_stops_at_the_obsoleting_section() {
        let stdout = "Last metadata expiration check: 0:01:00 ago.\n\
                      \n\
                      grub2-efi-x64.x86_64  1:2.06-123.fc40  updates\n\
                      \n\
                      Obsoleting Packages\n\
                      grub2-efi-x64-cdboot.x86_64  1:2.06-123.fc40  updates\n";
        let rows = parse_upgrades(stdout);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "grub2-efi-x64");
    }

    #[test]
    fn info_reads_the_first_stanza_only() {
        let detail = parse_show(INFO);
        assert_eq!(detail.name, "xz");
        assert_eq!(detail.id, "xz");
        // The second stanza offers 5.8.1 under a different license; both
        // must stay out.
        assert_eq!(detail.version, "5.4.6");
        assert_eq!(detail.license, "GPLv2+ and Public Domain");
        assert_eq!(detail.homepage, "https://tukaani.org/xz/");
    }

    #[test]
    fn info_combines_summary_and_multiline_description() {
        let detail = parse_show(INFO);
        assert!(
            detail
                .description
                .starts_with("LZMA compression utilities\nXZ Utils are an attempt"),
            "{}",
            detail.description
        );
        assert!(
            detail
                .description
                .contains("compression algorithms.\nLZMA is a general purpose"),
            "empty continuation lines collapse: {}",
            detail.description
        );
        assert!(detail.description.contains("part of 7-Zip."));
    }

    #[test]
    fn info_maps_packager_or_vendor_to_publisher() {
        // The captured stanza carries neither key (dnf omits Packager when
        // it duplicates the buildsystem), so publisher stays empty there.
        assert_eq!(parse_show(INFO).publisher, "");

        let with_packager = "Name         : demo\n\
                             Version      : 1.0\n\
                             Packager     : Fedora Project\n\
                             Summary      : A demo\n";
        assert_eq!(parse_show(with_packager).publisher, "Fedora Project");

        let with_vendor = "Name         : demo\n\
                           Version      : 1.0\n\
                           Vendor       : Example Corp.\n\
                           Summary      : A demo\n";
        assert_eq!(parse_show(with_vendor).publisher, "Example Corp.");
    }
}
