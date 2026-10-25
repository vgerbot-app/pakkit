//! Parsers for flatpak's machine-readable output.
//!
//! Every listing runs with an explicit `--columns=` list, so rows are
//! tab-separated and locale-independent — except that *trailing* empty
//! columns are simply missing, so fields are read with `.get(n)` semantics.
//! `info` / `remote-info` print aligned `Key: value` pairs instead, with
//! gettext-localized keys; the backend spawns them with `LC_ALL=C`, so the
//! keys matched below are the English ones.

use crate::backends::types::{ListRow, PackageDetail, SearchRow, SourceRow};

/// Ids with these prefixes are runtimes or runtime extensions, not
/// applications; `flatpak search` mixes them into its results.
const RUNTIME_PREFIXES: &[&str] = &[
    "org.freedesktop.Platform",
    "org.freedesktop.Sdk",
    "org.gnome.Platform",
    "org.gnome.Sdk",
    "org.kde.Platform",
    "org.kde.Sdk",
];

/// Whether `id` is a runtime or runtime extension (`org.gnome.Platform` and
/// `org.gnome.Platform.Locale` are; a hypothetical `org.gnome.Platformer`
/// game is not).
fn is_runtime(id: &str) -> bool {
    RUNTIME_PREFIXES.iter().any(|prefix| {
        id == *prefix
            || id
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('.'))
    })
}

/// `search --columns=application,name,version,remotes,description`
///
/// The `remotes` column is a comma list (`flathub,gnome-nightly`) and is
/// kept verbatim in `source`; `search_packages` filters it per remote.
/// Identical rows repeat across branches, so exact duplicates are dropped.
pub fn parse_search(stdout: &str) -> Vec<SearchRow> {
    let mut rows: Vec<SearchRow> = Vec::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut cols = line.split('\t');
        let id = cols.next().unwrap_or_default().trim();
        if id.is_empty() || is_runtime(id) {
            continue;
        }
        let name = cols.next().unwrap_or_default().trim();
        let row = SearchRow {
            name: if name.is_empty() {
                id.to_owned()
            } else {
                name.to_owned()
            },
            id: id.to_owned(),
            version: cols.next().unwrap_or_default().trim().to_owned(),
            match_kind: None,
            source: cols.next().unwrap_or_default().trim().to_owned(),
            description: cols
                .next()
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .map(str::to_owned),
        };
        if !rows.contains(&row) {
            rows.push(row);
        }
    }
    rows
}

/// `list --app --columns=application,name,version,origin`
pub fn parse_list(stdout: &str) -> Vec<ListRow> {
    stdout.lines().filter_map(parse_list_row).collect()
}

fn parse_list_row(line: &str) -> Option<ListRow> {
    if line.trim().is_empty() {
        return None;
    }
    let mut cols = line.split('\t');
    let id = cols.next().unwrap_or_default().trim();
    if id.is_empty() {
        return None;
    }
    let name = cols.next().unwrap_or_default().trim();
    Some(ListRow {
        name: if name.is_empty() {
            id.to_owned()
        } else {
            name.to_owned()
        },
        id: id.to_owned(),
        version: cols.next().unwrap_or_default().trim().to_owned(),
        available: None,
        source: cols.next().unwrap_or_default().trim().to_owned(),
    })
}

/// `remote-ls --updates --app --columns=application,name,version,origin`
///
/// The version column names the *new* version, so it moves to `available`.
/// When it is empty the row still represents an update; the placeholder
/// keeps it visible in the updates view.
pub fn parse_updates(stdout: &str) -> Vec<ListRow> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut row = parse_list_row(line)?;
            row.available = Some(if row.version.is_empty() {
                "(update)".to_owned()
            } else {
                row.version.clone()
            });
            Some(row)
        })
        .collect()
}

/// `remotes --columns=name,url`
pub fn parse_remotes(stdout: &str) -> Vec<SourceRow> {
    stdout
        .lines()
        .filter_map(|line| {
            if line.trim().is_empty() {
                return None;
            }
            let mut cols = line.split('\t');
            let name = cols.next().unwrap_or_default().trim();
            if name.is_empty() {
                return None;
            }
            Some(SourceRow {
                name: name.to_owned(),
                argument: cols.next().unwrap_or_default().trim().to_owned(),
                explicit: true,
            })
        })
        .collect()
}

/// `info` / `remote-info`, spawned with `LC_ALL=C`: a `Name - Summary` line
/// followed by aligned `Key: value` pairs. Both commands share the shape, so
/// one parser covers installed and remote packages alike.
pub fn parse_show(stdout: &str) -> PackageDetail {
    let mut detail = PackageDetail::default();
    let mut lines = stdout.lines().filter(|line| !line.trim().is_empty());
    if let Some(summary) = lines.next() {
        let summary = summary.trim();
        if let Some((name, description)) = summary.split_once(" - ") {
            detail.name = name.trim().to_owned();
            detail.description = description.trim().to_owned();
        } else {
            detail.name = summary.to_owned();
        }
    }
    for line in lines {
        let Some((key, value)) = line.trim().split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "ID" => detail.id = value.to_owned(),
            "Version" => detail.version = value.to_owned(),
            "License" => detail.license = value.to_owned(),
            "Date" => detail.release_date = value.to_owned(),
            _ => {}
        }
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../../../tests/fixtures/flatpak_search.txt");
    const LIST: &str = include_str!("../../../tests/fixtures/flatpak_list.txt");
    const UPDATES: &str = include_str!("../../../tests/fixtures/flatpak_updates.txt");
    const REMOTES: &str = include_str!("../../../tests/fixtures/flatpak_remotes.txt");
    const INFO: &str = include_str!("../../../tests/fixtures/flatpak_info.txt");
    const REMOTE_INFO: &str = include_str!("../../../tests/fixtures/flatpak_remote_info.txt");

    #[test]
    fn search_drops_runtime_rows_and_duplicates() {
        let rows = parse_search(SEARCH);
        let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
        assert!(
            ids.iter().all(|id| !is_runtime(id)),
            "runtime rows leaked: {ids:?}"
        );
        // The fixture's repeated Floorp row is parsed once.
        assert_eq!(
            ids.iter().filter(|id| **id == "one.ablaze.floorp").count(),
            1
        );
        assert!(ids.contains(&"org.mozilla.firefox"));
    }

    #[test]
    fn search_keeps_comma_remotes_and_cjk_text() {
        let rows = parse_search(SEARCH);
        let firedragon = rows
            .iter()
            .find(|row| row.id == "org.garudalinux.firedragon")
            .expect("firedragon row");
        assert_eq!(firedragon.source, "flathub,gnome-nightly");

        let bianjiqi = rows
            .iter()
            .find(|row| row.id == "com.example.bianjiqi")
            .expect("CJK row");
        assert_eq!(bianjiqi.name, "编辑器");
        assert_eq!(
            bianjiqi.description.as_deref(),
            Some("一个简单的文本编辑器")
        );
    }

    #[test]
    fn search_fills_the_description_column() {
        let rows = parse_search(SEARCH);
        let firefox = rows
            .iter()
            .find(|row| row.id == "org.mozilla.firefox")
            .expect("firefox row");
        assert_eq!(firefox.version, "158.0.1");
        assert_eq!(
            firefox.description.as_deref(),
            Some("Fast, Private & Safe Web Browser")
        );
    }

    #[test]
    fn list_reads_four_columns() {
        let rows = parse_list(LIST);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].id, "com.dbxio.dbx");
        assert_eq!(rows[0].name, "DBX");
        assert_eq!(rows[0].version, "0.6.32");
        assert_eq!(rows[0].source, "flatpark");
        assert_eq!(rows[0].available, None);
    }

    #[test]
    fn updates_move_the_version_to_available() {
        let rows = parse_updates(UPDATES);
        assert_eq!(rows.len(), 5);
        let dbx = &rows[0];
        assert_eq!(dbx.id, "com.dbxio.dbx");
        assert_eq!(dbx.available.as_deref(), Some("0.6.51"));

        // An empty version still names an update.
        let mystery = rows
            .iter()
            .find(|row| row.id == "org.example.mystery")
            .expect("empty-version row");
        assert_eq!(mystery.available.as_deref(), Some("(update)"));

        // A missing trailing origin column parses as an empty source.
        let noremote = rows
            .iter()
            .find(|row| row.id == "org.example.noremote")
            .expect("missing-origin row");
        assert_eq!(noremote.source, "");
    }

    #[test]
    fn remotes_read_name_and_url() {
        let rows = parse_remotes(REMOTES);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].name, "flathub");
        assert_eq!(rows[0].argument, "https://dl.flathub.org/repo/");
        assert!(rows[0].explicit);
    }

    #[test]
    fn info_reads_the_summary_line_and_keys() {
        let detail = parse_show(INFO);
        assert_eq!(detail.name, "Web");
        assert_eq!(detail.description, "Browse the web");
        assert_eq!(detail.id, "org.gnome.Epiphany.Devel");
        assert_eq!(detail.version, "51.1");
        assert_eq!(detail.license, "GPL-3.0+");
        assert_eq!(detail.release_date, "2026-10-02 14:49:38 +0000");
    }

    #[test]
    fn remote_info_shares_the_info_shape() {
        let detail = parse_show(REMOTE_INFO);
        assert_eq!(detail.name, "Firefox");
        assert_eq!(detail.description, "Fast, Private & Safe Web Browser");
        assert_eq!(detail.id, "org.mozilla.firefox");
        assert_eq!(detail.version, "158.0.1");
        assert_eq!(detail.license, "MPL-2.0");
    }

    #[test]
    fn runtime_detection_requires_a_dot_boundary() {
        assert!(is_runtime("org.gnome.Platform"));
        assert!(is_runtime("org.freedesktop.Platform.VAAPI.nvidia"));
        assert!(is_runtime("org.kde.Sdk"));
        assert!(!is_runtime("org.gnome.Platformer"));
        assert!(!is_runtime("org.mozilla.firefox"));
    }
}
