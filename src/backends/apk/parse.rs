//! Pure parsers for apk's human-readable output.
//!
//! The backend spawns everything with `LC_ALL=C`, so the section labels
//! matched below are the English ones. apk prints packages as
//! `name-version` atoms in which the name/version boundary is the LAST `-`
//! followed by an ASCII digit: `apk-tools-2.14.4-r0` splits into
//! `apk-tools` / `2.14.4-r0` because the later `-r0` dash precedes a
//! letter, and `musl-1.2.4_git20230717-r4` likewise keeps the `_git…`
//! fragment in the version. [`split_name_version`] tokenizes every row
//! shape below.

use crate::backends::types::{ListRow, PackageDetail, SearchRow};

/// Split a `name-version` atom at the last `-` followed by an ASCII digit.
///
/// Returns `None` when no such boundary exists (garbage lines, bare names)
/// or when the name part would be empty.
fn split_name_version(atom: &str) -> Option<(&str, &str)> {
    let bytes = atom.as_bytes();
    for ix in (1..bytes.len()).rev() {
        if bytes[ix] == b'-' && bytes.get(ix + 1).is_some_and(|b| b.is_ascii_digit()) {
            return Some((&atom[..ix], &atom[ix + 1..]));
        }
    }
    None
}

/// `search -v <query>`: rows of `name-version - description`, split at the
/// first `" - "`. Lines without that separator — or without a digit-dash
/// boundary on the left — are not package rows.
pub fn parse_search(stdout: &str) -> Vec<SearchRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let (atom, description) = line.split_once(" - ")?;
            let (name, version) = split_name_version(atom.trim_end())?;
            Some(SearchRow {
                name: name.to_owned(),
                id: name.to_owned(),
                version: version.to_owned(),
                match_kind: None,
                source: String::new(),
                description: Some(description.to_owned()),
            })
        })
        .collect()
}

/// `list --installed`: the first whitespace token is a `name-version`
/// atom, followed by arch/origin/license fields. apk's list carries no
/// repository name, so `source` stays empty and `available` is `None`.
pub fn parse_list(stdout: &str) -> Vec<ListRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let atom = line.split_whitespace().next()?;
            let (name, version) = split_name_version(atom)?;
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

/// `version -l '<'`: column-padded rows of `name-oldversion < newversion`
/// under an `Installed: / Available:` header. The header carries no
/// `" < "` and falls out naturally.
pub fn parse_upgrades(stdout: &str) -> Vec<ListRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let (left, right) = line.split_once(" < ")?;
            let (name, version) = split_name_version(left.trim_end())?;
            let available = right.trim();
            // Some apk builds repeat the atom on the right
            // (`name-newversion`); reduce it to the bare version.
            let available = available
                .strip_prefix(name)
                .and_then(|rest| rest.strip_prefix('-'))
                .unwrap_or(available);
            Some(ListRow {
                name: name.to_owned(),
                id: name.to_owned(),
                version: version.to_owned(),
                available: Some(available.to_owned()),
                source: String::new(),
            })
        })
        .collect()
}

/// The section an `info -a` header line opens, keyed off its trailing
/// label. `installed size:` matches [`Section::Size`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Description,
    Webpage,
    Size,
    License,
}

/// Match a `name-version <label>:` section header.
fn section_header(line: &str) -> Option<Section> {
    if line.ends_with("description:") {
        Some(Section::Description)
    } else if line.ends_with("webpage:") {
        Some(Section::Webpage)
    } else if line.ends_with("size:") {
        Some(Section::Size)
    } else if line.ends_with("license:") {
        Some(Section::License)
    } else {
        None
    }
}

/// `info -a <id>`: `name-version <label>:` headers, each followed by a
/// blank-line-terminated block of value lines. The first header doubles as
/// the identity line (`redis-7.2.11-r0 description:`). Only description,
/// webpage (homepage) and license are read; `installed size:` and friends
/// are consumed so their values do not leak into the description.
pub fn parse_show(stdout: &str) -> PackageDetail {
    let mut detail = PackageDetail::default();
    let mut section: Option<Section> = None;
    for line in stdout.lines() {
        if let Some(kind) = section_header(line) {
            section = Some(kind);
            // The identity lives on the first header's leading atom.
            if detail.name.is_empty()
                && let Some(atom) = line.split_whitespace().next()
                && let Some((name, version)) = split_name_version(atom)
            {
                detail.name = name.to_owned();
                detail.id = name.to_owned();
                detail.version = version.to_owned();
            }
            continue;
        }
        if line.trim().is_empty() {
            section = None;
            continue;
        }
        match section {
            Some(Section::Description) => {
                if !detail.description.is_empty() {
                    detail.description.push('\n');
                }
                detail.description.push_str(line.trim_end());
            }
            Some(Section::Webpage) if detail.homepage.is_empty() => {
                detail.homepage = line.trim().to_owned();
            }
            Some(Section::License) if detail.license.is_empty() => {
                detail.license = line.trim().to_owned();
            }
            _ => {}
        }
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../../../tests/fixtures/apk_search.txt");
    const LIST: &str = include_str!("../../../tests/fixtures/apk_list_installed.txt");
    const VERSION: &str = include_str!("../../../tests/fixtures/apk_version.txt");
    const INFO: &str = include_str!("../../../tests/fixtures/apk_info.txt");

    #[test]
    fn name_version_splits_at_the_last_digit_dash() {
        assert_eq!(
            split_name_version("apk-tools-2.14.4-r0"),
            Some(("apk-tools", "2.14.4-r0"))
        );
        assert_eq!(
            split_name_version("musl-1.2.4_git20230717-r4"),
            Some(("musl", "1.2.4_git20230717-r4"))
        );
        assert_eq!(
            split_name_version("font-noto-emoji-2.042-r0"),
            Some(("font-noto-emoji", "2.042-r0"))
        );
        assert_eq!(
            split_name_version("sudo-1.9.17_p1-r0"),
            Some(("sudo", "1.9.17_p1-r0"))
        );
        assert_eq!(
            split_name_version("udis86-git-1.7.2_git20221013-r1"),
            Some(("udis86-git", "1.7.2_git20221013-r1"))
        );
    }

    #[test]
    fn name_version_rejects_garbage() {
        assert_eq!(split_name_version(""), None);
        assert_eq!(split_name_version("no-dash"), None);
        assert_eq!(split_name_version("only-letters-here"), None);
        assert_eq!(split_name_version("-1.0"), None);
        assert_eq!(split_name_version("trailing-"), None);
    }

    #[test]
    fn search_reads_atoms_and_descriptions() {
        let rows = parse_search(SEARCH);
        assert_eq!(rows.len(), 14);
        assert!(
            rows.iter()
                .all(|row| row.source.is_empty() && row.match_kind.is_none())
        );

        let sudo = rows.iter().find(|row| row.id == "sudo").expect("row");
        assert_eq!(sudo.name, "sudo");
        assert_eq!(sudo.version, "1.9.17_p1-r0");
        assert_eq!(
            sudo.description.as_deref(),
            Some("Give certain users the ability to run some commands as root")
        );

        let doc = rows
            .iter()
            .find(|row| row.id == "doas-sudo-shim-doc")
            .expect("row");
        assert_eq!(doc.version, "0.1.1-r1");
        assert_eq!(
            doc.description.as_deref(),
            Some("A shim for the sudo command that utilizes doas (documentation)")
        );
    }

    #[test]
    fn list_reads_installed_atoms_with_no_source() {
        let rows = parse_list(LIST);
        assert_eq!(rows.len(), 15);
        assert!(rows.iter().all(|row| row.source.is_empty()));
        assert!(rows.iter().all(|row| row.available.is_none()));

        let tools = rows.iter().find(|row| row.id == "apk-tools").expect("row");
        assert_eq!(tools.name, "apk-tools");
        assert_eq!(tools.version, "2.14.6-r3");

        let doc = rows.iter().find(|row| row.id == "sudo-doc").expect("row");
        assert_eq!(doc.version, "1.9.17_p1-r0");

        let git = rows.iter().find(|row| row.id == "udis86-git").expect("row");
        assert_eq!(git.version, "1.7.2_git20221013-r1");

        let rundeps = rows
            .iter()
            .find(|row| row.id == ".redis-rundeps")
            .expect("row");
        assert_eq!(rundeps.version, "20251103.174033");
    }

    #[test]
    fn upgrades_read_old_and_new_versions() {
        let rows = parse_upgrades(VERSION);
        // The `Installed: / Available:` header line is skipped.
        assert_eq!(rows.len(), 11);

        let musl = rows.iter().find(|row| row.id == "musl").expect("row");
        assert_eq!(musl.version, "1.2.5-r9");
        assert_eq!(musl.available.as_deref(), Some("1.2.5-r12"));

        let tzdata = rows.iter().find(|row| row.id == "tzdata").expect("row");
        assert_eq!(tzdata.version, "2025b-r0");
        assert_eq!(tzdata.available.as_deref(), Some("2026e-r0"));
    }

    #[test]
    fn upgrades_strip_a_repeated_name_prefix() {
        let rows = parse_upgrades("musl-1.2.5-r9 < musl-1.2.5-r12\n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].available.as_deref(), Some("1.2.5-r12"));
    }

    #[test]
    fn show_reads_identity_description_homepage_and_license() {
        let detail = parse_show(INFO);
        assert_eq!(detail.name, "redis");
        assert_eq!(detail.id, "redis");
        assert_eq!(detail.version, "7.2.11-r0");
        assert_eq!(detail.description, "Advanced key-value store");
        assert_eq!(detail.homepage, "https://redis.io/");
        assert_eq!(detail.license, "BSD-3-Clause");
        // The `installed size:` block must not leak into the description.
        assert!(!detail.description.contains("KiB"));
    }

    #[test]
    fn show_keeps_multiline_descriptions() {
        let stdout = "demopkg-1.0-r0 description:\nfirst line\nsecond line\n\ndemopkg-1.0-r0 license:\nMIT\n";
        let detail = parse_show(stdout);
        assert_eq!(detail.name, "demopkg");
        assert_eq!(detail.version, "1.0-r0");
        assert_eq!(detail.description, "first line\nsecond line");
        assert_eq!(detail.license, "MIT");
    }

    #[test]
    fn garbage_lines_are_rejected() {
        assert!(parse_search("random garbage\nno separator here\n").is_empty());
        assert!(parse_list("Listing...\nwarning: something\n").is_empty());
        assert!(parse_upgrades("Installed:  Available:\n").is_empty());
        assert_eq!(parse_show(""), PackageDetail::default());
    }
}
