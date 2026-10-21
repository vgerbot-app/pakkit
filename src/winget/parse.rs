//! Pure parsers for winget's output formats.
//!
//! winget has no machine-readable table mode, so `search`, `list`, `upgrade`
//! and `show` are parsed from their human output. The output is padded to the
//! *display* width of the console, which matters here: this machine's
//! installed list has CJK package names, and a byte- or char-counted column
//! split lands in the wrong place for them. Every table parser therefore
//! splits on the header row's display columns.
//!
//! `export` and `source export` do have JSON output, and are parsed with
//! `serde_json` (`source export` emits JSON Lines, one source per line).

use serde::{Deserialize, Serialize};

use super::exit::WingetError;

/// One row of `winget search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRow {
    pub name: String,
    pub id: String,
    pub version: String,
    pub match_kind: Option<String>,
    pub source: String,
}

/// One row of `winget list` / `winget list --upgrade-available`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRow {
    pub name: String,
    pub id: String,
    pub version: String,
    /// The newer version winget can move to; `None` when nothing is available.
    pub available: Option<String>,
    pub source: String,
}

/// One row of `winget source list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRow {
    pub name: String,
    pub argument: String,
    pub explicit: bool,
}

/// One line of `winget source export`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceExport {
    #[serde(rename = "Arg", default)]
    pub argument: String,
    #[serde(rename = "Data", default)]
    pub data: String,
    #[serde(rename = "Explicit", default)]
    pub explicit: bool,
    #[serde(rename = "Identifier", default)]
    pub identifier: String,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "TrustLevel", default)]
    pub trust_level: Vec<String>,
    #[serde(rename = "Type", default)]
    pub ty: String,
}

impl SourceExport {
    /// A source is trusted when winget's own trust list says so.
    pub fn trusted(&self) -> bool {
        self.trust_level.iter().any(|t| t == "Trusted")
    }
}

/// The fields of `winget show` this UI renders.
///
/// `description` and `release_notes` keep their original line breaks: winget
/// prints both as a header line followed by indented prose lines.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageDetail {
    pub name: String,
    pub id: String,
    pub version: String,
    pub publisher: String,
    pub publisher_url: String,
    pub publisher_support_url: String,
    pub moniker: String,
    pub description: String,
    pub homepage: String,
    pub license: String,
    pub license_url: String,
    pub copyright: String,
    pub release_notes: String,
    pub release_notes_url: String,
    pub tags: Vec<String>,
    pub installer_type: String,
    pub installer_url: String,
    pub installer_sha256: String,
    pub release_date: String,
    pub offline_distribution: String,
}

/// Display width of a character, mirroring how the Windows console pads
/// tables: East Asian wide characters occupy two cells, everything else one.
fn char_width(c: char) -> usize {
    match c as u32 {
        // Zero-width and combining marks do not advance the cursor.
        0x0300..=0x036F | 0x200B..=0x200F | 0xFE00..=0xFE0F | 0xFEFF => 0,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

/// A character paired with the display column it starts at.
struct Positioned {
    col: usize,
    ch: char,
}

fn position(line: &str) -> (Vec<Positioned>, usize) {
    let mut out = Vec::with_capacity(line.chars().count());
    let mut col = 0usize;
    for ch in line.chars() {
        out.push(Positioned { col, ch });
        col += char_width(ch);
    }
    (out, col)
}

/// Header token spans: `(start_display_column, name)`.
fn header_spans(header: &str) -> Vec<(usize, String)> {
    let (positioned, _) = position(header);
    let mut spans = Vec::new();
    let mut current: Option<(usize, String)> = None;
    for p in positioned {
        if p.ch.is_whitespace() {
            if let Some(done) = current.take()
                && !done.1.is_empty()
            {
                spans.push(done);
            }
        } else {
            match &mut current {
                Some((_, text)) => text.push(p.ch),
                None => current = Some((p.col, p.ch.to_string())),
            }
        }
    }
    if let Some(done) = current.take()
        && !done.1.is_empty()
    {
        spans.push(done);
    }
    spans
}

/// Slice a data row at the header's display columns.
///
/// A cell holds every character that *starts* inside the column band, which is
/// how a CJK name of odd display width still lines its neighbour up: winget
/// pads to the boundary, not past it.
fn slice_row(line: &str, spans: &[(usize, String)]) -> Vec<String> {
    let (positioned, _) = position(line);
    let mut cells: Vec<String> = vec![String::new(); spans.len()];
    for p in positioned {
        let Some(index) = spans
            .iter()
            .enumerate()
            .rev()
            .find(|(_, (start, _))| p.col >= *start)
            .map(|(i, _)| i)
        else {
            continue;
        };
        cells[index].push(p.ch);
    }
    cells.into_iter().map(|c| c.trim().to_owned()).collect()
}

/// True for the `-----` rule winget prints under a header.
fn is_separator(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= 8 && trimmed.chars().all(|c| c == '-')
}

/// Split a winget table into its header names and data rows.
///
/// The header is the first line followed by a `-----` rule. Rows are sliced by
/// the header's display columns. When no rule is present (some winget builds
/// omit it in narrow consoles) the fallback splits on runs of two or more
/// spaces.
fn split_table(text: &str) -> Option<(Vec<String>, Vec<Vec<String>>)> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();

    let header_index = lines
        .iter()
        .enumerate()
        .find(|(i, line)| {
            !line.trim().is_empty() && lines.get(i + 1).is_some_and(|next| is_separator(next))
        })
        .map(|(i, _)| i);

    let (headers, rows) = match header_index {
        Some(index) => {
            let spans = header_spans(lines[index]);
            let headers = spans.iter().map(|(_, name)| name.clone()).collect();
            let rows = lines[index + 2..]
                .iter()
                .filter(|line| !line.trim().is_empty() && !is_separator(line))
                .map(|line| slice_row(line, &spans))
                .collect();
            (headers, rows)
        }
        None => {
            let first = lines.iter().find(|l| !l.trim().is_empty())?;
            let headers: Vec<String> = split_runs(first);
            let rows = lines
                .iter()
                .skip_while(|l| l.trim().is_empty())
                .skip(1)
                .filter(|line| !line.trim().is_empty() && !is_separator(line))
                .map(|line| split_runs(line))
                .collect();
            (headers, rows)
        }
    };

    Some((headers, rows))
}

/// Fallback split for output that has no separator rule.
fn split_runs(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut spaces = 0usize;
    for ch in line.chars() {
        if ch == ' ' {
            spaces += 1;
            continue;
        }
        if spaces >= 2 && !current.is_empty() {
            cells.push(std::mem::take(&mut current));
        } else if spaces > 0 {
            current.push(' ');
        }
        spaces = 0;
        current.push(ch);
    }
    if !current.is_empty() {
        cells.push(current);
    }
    cells
}

/// Zero-based index of a header by name, case-insensitively.
fn column(headers: &[String], name: &str) -> Option<usize> {
    headers.iter().position(|h| h.eq_ignore_ascii_case(name))
}

/// Drop rows winget appends as notices rather than data.
fn is_noise_row(row: &[String]) -> bool {
    let joined = row.join(" ").to_lowercase();
    joined.is_empty()
        || row.iter().all(String::is_empty)
        || joined.contains("no installed package found")
        || joined.contains("no package found")
        || joined.starts_with("...")
}

/// `winget search <query>`.
pub fn parse_search(text: &str) -> Vec<SearchRow> {
    let Some((headers, rows)) = split_table(text) else {
        return Vec::new();
    };
    let name = column(&headers, "Name");
    let id = column(&headers, "Id");
    let version = column(&headers, "Version");
    let source = column(&headers, "Source");
    let match_kind = column(&headers, "Match");

    let get = |row: &[String], index: Option<usize>| {
        index.and_then(|i| row.get(i)).cloned().unwrap_or_default()
    };

    rows.iter()
        .filter(|row| !is_noise_row(row))
        .filter_map(|row| {
            let id = get(row, id);
            if id.is_empty() {
                return None;
            }
            let match_kind =
                match_kind
                    .and_then(|i| row.get(i))
                    .and_then(|cell| match cell.split_once(':') {
                        Some((kind, value)) => Some(format!("{}: {}", kind.trim(), value.trim())),
                        None if cell.is_empty() => None,
                        None => Some(cell.clone()),
                    });
            Some(SearchRow {
                name: get(row, name),
                id,
                version: get(row, version),
                match_kind,
                source: get(row, source),
            })
        })
        .collect()
}

/// `winget list` and `winget list --upgrade-available`.
pub fn parse_list(text: &str) -> Vec<ListRow> {
    let Some((headers, rows)) = split_table(text) else {
        return Vec::new();
    };
    let name = column(&headers, "Name");
    let id = column(&headers, "Id");
    let version = column(&headers, "Version");
    let available = column(&headers, "Available");
    let source = column(&headers, "Source");

    let get = |row: &[String], index: Option<usize>| {
        index.and_then(|i| row.get(i)).cloned().unwrap_or_default()
    };

    rows.iter()
        .filter(|row| !is_noise_row(row))
        .filter_map(|row| {
            let id = get(row, id);
            if id.is_empty() {
                return None;
            }
            let available = available
                .and_then(|i| row.get(i))
                .map(|cell| cell.trim())
                .filter(|cell| !cell.is_empty())
                .map(str::to_owned);
            Some(ListRow {
                name: get(row, name),
                id,
                version: get(row, version),
                available,
                source: get(row, source),
            })
        })
        .collect()
}

/// `winget source list`.
pub fn parse_source_list(text: &str) -> Vec<SourceRow> {
    let Some((headers, rows)) = split_table(text) else {
        return Vec::new();
    };
    let name = column(&headers, "Name");
    let argument = column(&headers, "Argument");
    let explicit = column(&headers, "Explicit");

    let get = |row: &[String], index: Option<usize>| {
        index.and_then(|i| row.get(i)).cloned().unwrap_or_default()
    };

    rows.iter()
        .filter(|row| !is_noise_row(row))
        .filter_map(|row| {
            let name = get(row, name);
            if name.is_empty() {
                return None;
            }
            Some(SourceRow {
                name,
                argument: get(row, argument),
                explicit: get(row, explicit).eq_ignore_ascii_case("true"),
            })
        })
        .collect()
}

/// `winget show --id <id> --exact`.
///
/// Three shapes appear in one document: flat `Key: value` lines, section
/// headers such as `Tags:` whose values are the indented lines that follow,
/// and text sections (`Description:`, `Release Notes:`) whose indented lines
/// are prose kept with their line breaks.
pub fn parse_show(text: &str) -> PackageDetail {
    let mut detail = PackageDetail::default();
    let mut section: Option<String> = None;

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // `Found Visual Studio Code [Microsoft.VisualStudioCode]`
        if let Some(rest) = trimmed.strip_prefix("Found ")
            && let Some((name, id)) = rest.rsplit_once(" [")
            && let Some(id) = id.strip_suffix(']')
        {
            if detail.name.is_empty() {
                detail.name = name.to_owned();
            }
            detail.id = id.to_owned();
            continue;
        }

        let indented = line.starts_with(char::is_whitespace);

        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            // Inside a text section an indented `X: y` line is prose (a URL,
            // a sentence with a colon) unless X is a label winget prints.
            if indented && section.as_deref().is_some_and(is_text_section) && !is_show_label(key) {
                push_text_line(&mut detail, section.as_deref().unwrap_or_default(), trimmed);
                continue;
            }
            if value.is_empty() {
                // A section header; its values are the indented lines below.
                section = Some(key.to_owned());
                continue;
            }
            section = None;
            assign(&mut detail, key, value);
        } else if indented {
            match section.as_deref() {
                Some(key) if key.eq_ignore_ascii_case("Tags") => {
                    detail.tags.push(trimmed.to_owned());
                }
                Some(key) if is_text_section(key) => {
                    push_text_line(&mut detail, key, trimmed);
                }
                _ => {}
            }
        }
    }

    detail.tags.retain(|t| !t.is_empty());
    detail
}

/// A section whose indented lines are prose, not fields.
fn is_text_section(key: &str) -> bool {
    key.eq_ignore_ascii_case("Description") || key.eq_ignore_ascii_case("Release Notes")
}

/// Append one prose line to a text section, keeping line breaks.
fn push_text_line(detail: &mut PackageDetail, section: &str, line: &str) {
    let slot = if section.eq_ignore_ascii_case("Description") {
        &mut detail.description
    } else if section.eq_ignore_ascii_case("Release Notes") {
        &mut detail.release_notes
    } else {
        return;
    };
    if !slot.is_empty() {
        slot.push('\n');
    }
    slot.push_str(line);
}

/// The labels winget prints in `show` output. Used to tell an indented
/// `Key: value` field apart from a prose line containing a colon.
fn is_show_label(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "name"
            | "id"
            | "version"
            | "channel"
            | "publisher"
            | "publisher url"
            | "publisher support url"
            | "author"
            | "moniker"
            | "description"
            | "homepage"
            | "license"
            | "license url"
            | "privacy url"
            | "copyright"
            | "copyright url"
            | "release notes"
            | "release notes url"
            | "package url"
            | "purchase url"
            | "installation notes"
            | "documentation"
            | "wiki"
            | "tags"
            | "agreements"
            | "installer"
            | "installer type"
            | "installer locale"
            | "installer url"
            | "installer sha256"
            | "release date"
            | "offline distribution supported"
            | "scope"
            | "elevation behavior"
            | "install location required"
    )
}

fn assign(detail: &mut PackageDetail, key: &str, value: &str) {
    let slot = match key.to_ascii_lowercase().as_str() {
        "name" => &mut detail.name,
        "id" => &mut detail.id,
        "version" => &mut detail.version,
        "publisher" => &mut detail.publisher,
        "publisher url" => &mut detail.publisher_url,
        "publisher support url" => &mut detail.publisher_support_url,
        "moniker" => &mut detail.moniker,
        "description" => &mut detail.description,
        "homepage" => &mut detail.homepage,
        "license" => &mut detail.license,
        "license url" => &mut detail.license_url,
        "copyright" => &mut detail.copyright,
        "release notes url" => &mut detail.release_notes_url,
        "installer type" => &mut detail.installer_type,
        "installer url" => &mut detail.installer_url,
        "installer sha256" => &mut detail.installer_sha256,
        "release date" => &mut detail.release_date,
        "offline distribution supported" => &mut detail.offline_distribution,
        _ => return,
    };
    if slot.is_empty() {
        *slot = value.to_owned();
    }
}

/// `winget source export` — JSON Lines, one source object per non-empty line.
pub fn parse_source_export(text: &str) -> Result<Vec<SourceExport>, WingetError> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str::<SourceExport>(line).map_err(WingetError::from))
        .collect()
}

/// Package identifiers from a `winget export` document.
pub fn parse_export_json(text: &str) -> Result<Vec<String>, WingetError> {
    #[derive(Deserialize)]
    struct Document {
        #[serde(rename = "Sources", default)]
        sources: Vec<Source>,
    }
    #[derive(Deserialize)]
    struct Source {
        #[serde(rename = "Packages", default)]
        packages: Vec<Package>,
    }
    #[derive(Deserialize)]
    struct Package {
        #[serde(rename = "PackageIdentifier", default)]
        package_identifier: String,
    }

    let document: Document = serde_json::from_str(text)?;
    Ok(document
        .sources
        .into_iter()
        .flat_map(|source| source.packages)
        .map(|package| package.package_identifier)
        .filter(|id| !id.is_empty())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../../tests/fixtures/search.txt");
    const LIST: &str = include_str!("../../tests/fixtures/list.txt");
    const UPGRADE: &str = include_str!("../../tests/fixtures/upgrade.txt");
    const SOURCES: &str = include_str!("../../tests/fixtures/sources.txt");
    const SHOW: &str = include_str!("../../tests/fixtures/show.txt");
    const SHOW_GIT: &str = include_str!("../../tests/fixtures/show_git.txt");
    const SOURCE_EXPORT: &str = include_str!("../../tests/fixtures/source_export.json");
    const EXPORT: &str = include_str!("../../tests/fixtures/export.json");

    #[test]
    fn search_finds_the_queried_package() {
        let rows = parse_search(SEARCH);
        assert!(!rows.is_empty(), "search fixture must yield rows");
        let vscode = rows
            .iter()
            .find(|row| row.id.contains("VisualStudioCode"))
            .expect("the fixture was captured for vscode");
        assert_eq!(vscode.name, "Microsoft Visual Studio Code");
        assert!(!vscode.version.is_empty());
        assert!(vscode.match_kind.is_some(), "Match column is present");
    }

    #[test]
    fn search_keeps_ids_intact_for_wide_names() {
        // The second fixture row has a plain ASCII name but a wide-character
        // neighbour column; a byte-offset split would corrupt every id.
        for row in parse_search(SEARCH) {
            assert!(
                !row.id.contains(' '),
                "id {:?} swallowed a space — column split drifted",
                row.id
            );
        }
    }

    #[test]
    fn list_rows_are_complete_and_keep_cjk_names_aligned() {
        let rows = parse_list(LIST);
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|row| !row.name.is_empty()));
        assert!(rows.iter().all(|row| !row.id.is_empty()));

        // `360极速浏览器` is the widest-name row in the fixture: its neighbours
        // must still land in the right columns.
        let first = rows.first().expect("fixture has rows");
        assert_eq!(first.name, "360极速浏览器");
        assert_eq!(first.id, "360.360Chrome");
        assert_eq!(first.version, "23.0.1280.0");
        assert_eq!(first.source, "winget");
        assert_eq!(first.available, None);
    }

    #[test]
    fn list_tolerates_rows_without_available_or_source() {
        // Store/MSIX entries carry neither an Available value nor a source.
        let rows = parse_list(LIST);
        let msix = rows
            .iter()
            .find(|row| row.id.starts_with("MSIX\\"))
            .expect("fixture has an MSIX entry");
        assert_eq!(msix.available, None);
        assert_eq!(msix.source, "");
        assert_eq!(msix.name, "Adobe Acrobat Reader");
    }

    #[test]
    fn upgrade_rows_all_offer_a_newer_version() {
        let rows = parse_list(UPGRADE);
        assert!(!rows.is_empty());
        assert!(
            rows.iter().all(|row| row.available.is_some()),
            "every --upgrade-available row offers a version"
        );
    }

    #[test]
    fn source_list_reports_explicit_flags() {
        let rows = parse_source_list(SOURCES);
        let winget = rows
            .iter()
            .find(|r| r.name == "winget")
            .expect("winget source");
        let msstore = rows
            .iter()
            .find(|r| r.name == "msstore")
            .expect("msstore source");
        assert!(!winget.explicit);
        assert!(!msstore.explicit);
        assert!(winget.argument.contains("cdn.winget.microsoft.com"));

        let fonts = rows
            .iter()
            .find(|r| r.name == "winget-font")
            .expect("fixture has an explicit source");
        assert!(fonts.explicit);
    }

    #[test]
    fn show_extracts_the_rendered_fields() {
        let detail = parse_show(SHOW);
        assert_eq!(detail.name, "Microsoft Visual Studio Code");
        assert_eq!(detail.id, "Microsoft.VisualStudioCode");
        assert_eq!(detail.version, "1.142.0");
        assert_eq!(detail.publisher, "Microsoft Corporation");
        assert_eq!(detail.moniker, "vscode");
        assert_eq!(detail.installer_type, "inno");
        assert_eq!(detail.release_date, "2026-10-14");
        assert!(detail.description.contains("code editor"));
        assert!(detail.homepage.starts_with("https://"));
        assert!(!detail.license.is_empty());
        assert_eq!(detail.tags, vec!["developer-tools", "editor"]);
    }

    #[test]
    fn show_keeps_multiline_sections_and_the_installer_block() {
        // Captured from `winget show Git.Git`: the description and the release
        // notes span several indented lines, and the installer block carries
        // indented fields of its own.
        let detail = parse_show(SHOW_GIT);
        assert_eq!(detail.version, "2.55.0.5");
        assert_eq!(detail.publisher, "The Git Development Community");
        assert_eq!(detail.publisher_url, "https://gitforwindows.org/");
        assert_eq!(
            detail.publisher_support_url,
            "https://github.com/git-for-windows/git/issues"
        );
        assert_eq!(detail.moniker, "git");
        assert!(
            detail
                .description
                .contains("distributed version control system")
                && detail.description.contains("Git for Windows focuses"),
            "both description paragraphs are kept, got {:?}",
            detail.description
        );
        assert_eq!(detail.license, "GPL-2.0");
        assert_eq!(
            detail.copyright,
            "Copyright (C) 1989, 1991 Free Software Foundation, Inc."
        );
        assert!(detail.release_notes.contains("New Features"));
        assert!(detail.release_notes.contains("Bug Fixes"));
        assert!(
            detail.release_notes.contains('\n'),
            "release notes keep their line breaks"
        );
        assert_eq!(
            detail.release_notes_url,
            "https://github.com/git-for-windows/git/releases/tag/v2.55.0.windows.5"
        );
        assert_eq!(detail.tags, vec!["git", "vcs"]);
        assert_eq!(detail.installer_type, "inno");
        assert_eq!(
            detail.installer_sha256,
            "d065a4e23c3d9a6b5073d609b5be0830227ec3ca053c083ba385061ddfaf94c6"
        );
        assert_eq!(detail.release_date, "2026-08-20");
        assert_eq!(detail.offline_distribution, "true");
    }

    #[test]
    fn show_treats_colons_inside_text_sections_as_prose() {
        let text = "Description:\n  See https://example.com/docs for usage: it matters.\nHomepage: https://example.com\n";
        let detail = parse_show(text);
        assert_eq!(
            detail.description,
            "See https://example.com/docs for usage: it matters."
        );
        assert_eq!(detail.homepage, "https://example.com");
    }

    #[test]
    fn source_export_is_json_lines_and_marks_winget_trusted() {
        let sources = parse_source_export(SOURCE_EXPORT).expect("fixture is valid JSONL");
        assert_eq!(sources.len(), 3);
        let winget = sources
            .iter()
            .find(|s| s.name == "winget")
            .expect("winget source");
        assert!(winget.trusted());
        assert_eq!(winget.ty, "Microsoft.PreIndexed.Package");
        assert!(winget.identifier.contains("Microsoft.Winget.Source"));
    }

    #[test]
    fn export_document_yields_package_ids() {
        let ids = parse_export_json(EXPORT).expect("fixture is a valid export document");
        assert!(!ids.is_empty());
        assert!(
            ids.iter().any(|id| id.contains("VisualStudio")),
            "fixture has an installed Visual Studio id"
        );
        assert!(ids.iter().all(|id| !id.is_empty()));
    }

    #[test]
    fn separator_detection_rejects_data_rows() {
        assert!(is_separator("-----------------------------------"));
        assert!(!is_separator("7-Zip    7zip.7zip   26.03"));
        assert!(!is_separator("---"), "too short to be a rule");
    }

    #[test]
    fn header_spans_use_display_width_for_cjk() {
        // A CJK header two cells wide must still anchor its own column.
        let spans = header_spans("名称        Id    Version");
        assert_eq!(spans[0], (0, "名称".to_owned()));
        assert_eq!(spans[1].1, "Id");
        assert_eq!(spans[2].1, "Version");
    }

    #[test]
    fn fallback_split_handles_missing_rule() {
        let text = "Name        Id          Version\nFoo Bar     Foo.Bar     1.0\n";
        let (headers, rows) = split_table(text).expect("fallback parse");
        assert_eq!(headers, vec!["Name", "Id", "Version"]);
        assert_eq!(rows.len(), 1);
    }
}
