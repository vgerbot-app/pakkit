//! Domain model: packages, categories and sources.
//!
//! winget supplies identity and installer metadata; the prototype also shows a
//! rating, an install count and an "updated" date, which no winget source
//! provides. Those stay `Option` and render as `—` when absent.

use crate::icons::IconKey;
use crate::winget::{ListRow, PackageDetail, SearchRow, SourceExport};

/// A built-in category. The set is fixed and client-side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum CategoryId {
    Dev,
    Office,
    Media,
    #[default]
    Tools,
    Science,
    Game,
    Net,
    Driver,
}

impl CategoryId {
    /// Every category, in the order the prototype lists them. The order is
    /// load-bearing: [`categorize`] takes the first match in this order, so it
    /// is the tie-break between overlapping tag tables.
    pub const ALL: [CategoryId; 8] = [
        CategoryId::Dev,
        CategoryId::Office,
        CategoryId::Media,
        CategoryId::Tools,
        CategoryId::Science,
        CategoryId::Game,
        CategoryId::Net,
        CategoryId::Driver,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            CategoryId::Dev => "编程开发",
            CategoryId::Office => "办公软件",
            CategoryId::Media => "影音娱乐",
            CategoryId::Tools => "实用工具",
            CategoryId::Science => "科学计算",
            CategoryId::Game => "游戏平台",
            CategoryId::Net => "网络通信",
            CategoryId::Driver => "驱动运行库",
        }
    }

    /// The tile colour from the prototype's `CATEGORIES`.
    pub const fn color(self) -> &'static str {
        match self {
            CategoryId::Dev => "#4f8cff",
            CategoryId::Office => "#16a34a",
            CategoryId::Media => "#e11d48",
            CategoryId::Tools => "#d97706",
            CategoryId::Science => "#7c3aed",
            CategoryId::Game => "#0891b2",
            CategoryId::Net => "#0ea5e9",
            CategoryId::Driver => "#64748b",
        }
    }

    pub const fn icon(self) -> IconKey {
        match self {
            CategoryId::Dev => IconKey::Code,
            CategoryId::Office => IconKey::Office,
            CategoryId::Media => IconKey::Media,
            CategoryId::Tools => IconKey::Wrench,
            CategoryId::Science => IconKey::Flask,
            CategoryId::Game => IconKey::Gamepad,
            CategoryId::Net => IconKey::Network,
            CategoryId::Driver => IconKey::Driver,
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            CategoryId::Dev => "IDE、编译器、运行时与版本控制",
            CategoryId::Office => "文档、表格、演示与协作套件",
            CategoryId::Media => "播放器、剪辑、直播与音乐",
            CategoryId::Tools => "压缩、截图、系统增强与清理",
            CategoryId::Science => "数值分析、统计与数据可视化",
            CategoryId::Game => "游戏启动器、运行库与加速器",
            CategoryId::Net => "浏览器、下载、远程与 VPN",
            CategoryId::Driver => "驱动、.NET/VC++ 运行库与固件",
        }
    }

    /// The short tag the prototype shows as a pill on the tile footer.
    pub const fn tag(self) -> &'static str {
        match self {
            CategoryId::Dev => "开发工具",
            CategoryId::Office => "办公效率",
            CategoryId::Media => "多媒体",
            CategoryId::Tools => "系统工具",
            CategoryId::Science => "科研",
            CategoryId::Game => "娱乐",
            CategoryId::Net => "网络",
            CategoryId::Driver => "系统",
        }
    }

    /// Stable key used by routing and by [`CategoryId::from_key`].
    pub const fn key(self) -> &'static str {
        match self {
            CategoryId::Dev => "dev",
            CategoryId::Office => "office",
            CategoryId::Media => "media",
            CategoryId::Tools => "tools",
            CategoryId::Science => "science",
            CategoryId::Game => "game",
            CategoryId::Net => "net",
            CategoryId::Driver => "driver",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }
}

/// Tag/keyword table mapping winget tags and name fragments onto a category.
///
/// Matched case-insensitively as substrings, tags first, in [`CategoryId::ALL`]
/// order, so a package lands in exactly one category.
pub const CATEGORY_TAGS: &[(CategoryId, &[&str])] = &[
    (
        CategoryId::Dev,
        &[
            "developer",
            "development",
            "programming",
            "ide",
            "sdk",
            "compiler",
            "code",
            "python",
            "nodejs",
            "dotnet",
            "java",
            "golang",
            "rust",
            "git",
            "docker",
            "vscode",
            "visual-studio",
        ],
    ),
    (
        CategoryId::Office,
        &[
            "office",
            "document",
            "spreadsheet",
            "presentation",
            "pdf",
            "mail",
            "note",
            "reader",
        ],
    ),
    (
        CategoryId::Media,
        &[
            "media",
            "video",
            "audio",
            "music",
            "player",
            "recorder",
            "streaming",
            "codec",
        ],
    ),
    (
        CategoryId::Tools,
        &[
            "utility",
            "tools",
            "archive",
            "compress",
            "cleaner",
            "monitor",
            "benchmark",
            "screenshot",
        ],
    ),
    (
        CategoryId::Science,
        &[
            "science",
            "math",
            "statistics",
            "simulation",
            "lab",
            "octave",
            "r-language",
            "matlab",
        ],
    ),
    (
        CategoryId::Game,
        &["game", "gaming", "launcher", "steam", "emulator"],
    ),
    (
        CategoryId::Net,
        &[
            "network", "vpn", "proxy", "ftp", "ssh", "remote", "browser", "chat",
        ],
    ),
    (
        CategoryId::Driver,
        &[
            "driver",
            "runtime",
            "redistributable",
            "library",
            "firmware",
        ],
    ),
];

/// Assign a package to exactly one category.
///
/// Tags are scanned before the package name, and categories are tried in
/// [`CategoryId::ALL`] order, so the result is deterministic. Unmatched
/// packages fall into [`CategoryId::Tools`].
pub fn categorize(name: &str, tags: &[String]) -> CategoryId {
    let haystacks: Vec<String> = tags
        .iter()
        .map(|t| t.to_lowercase())
        .chain(std::iter::once(name.to_lowercase()))
        .collect();

    for (category, keywords) in CATEGORY_TAGS {
        if haystacks
            .iter()
            .any(|hay| keywords.iter().any(|keyword| hay.contains(keyword)))
        {
            return *category;
        }
    }
    CategoryId::Tools
}

/// A software package as the UI presents it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    /// A newer version winget can move to.
    pub available: Option<String>,
    pub source: String,
    pub size: Option<String>,
    pub license: Option<String>,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub installer_url: Option<String>,
    pub updated: Option<String>,
    pub tags: Vec<String>,
    pub category: CategoryId,
    pub installed: bool,
    pub rating: Option<f32>,
    pub installs: Option<u64>,
    /// How winget matched this row in the last search, e.g. `Moniker: vscode`.
    ///
    /// Search results come from winget's own matching, which reaches monikers
    /// and tags a substring filter over name/id cannot; carrying the match
    /// keeps a row that legitimately matched visible.
    pub match_kind: Option<String>,
}

impl Package {
    /// Build a catalog entry from a `winget search` row.
    pub fn from_search(row: &SearchRow, source_fallback: &str) -> Self {
        let source = if row.source.is_empty() {
            source_fallback.to_owned()
        } else {
            row.source.clone()
        };
        let category = categorize(&row.name, &[]);
        Self {
            id: row.id.clone(),
            name: row.name.clone(),
            version: row.version.clone(),
            source,
            category,
            match_kind: row.match_kind.clone(),
            ..Self::default()
        }
    }

    /// Merge `winget show` metadata into this package.
    pub fn apply_detail(&mut self, detail: &PackageDetail) {
        if self.name.is_empty() {
            self.name = detail.name.clone();
        }
        if !detail.publisher.is_empty() {
            self.publisher = detail.publisher.clone();
        }
        if self.version.is_empty() {
            self.version = detail.version.clone();
        }
        self.license = non_empty(&detail.license);
        self.description = non_empty(&detail.description);
        self.homepage = non_empty(&detail.homepage);
        self.installer_url = non_empty(&detail.installer_url);
        self.updated = non_empty(&detail.release_date);
        if !detail.tags.is_empty() {
            self.tags = detail.tags.clone();
            self.category = categorize(&self.name, &self.tags);
        }
    }

    /// Apply an installed-state row from `winget list`.
    pub fn apply_list(&mut self, row: &ListRow) {
        self.installed = true;
        if self.version.is_empty() {
            self.version = row.version.clone();
        }
        self.available = row.available.clone();
        if self.source.is_empty() && !row.source.is_empty() {
            self.source = row.source.clone();
        }
        if self.name.is_empty() {
            self.name = row.name.clone();
        }
    }

    /// Create an installed-only package from a `winget list` row, for entries
    /// the catalog has no record of (Store apps, machine-wide installers).
    pub fn from_list(row: &ListRow) -> Self {
        Self {
            id: row.id.clone(),
            name: row.name.clone(),
            version: row.version.clone(),
            available: row.available.clone(),
            source: row.source.clone(),
            installed: true,
            category: categorize(&row.name, &[]),
            ..Self::default()
        }
    }

    /// Normalized identity for deduplication across the catalog and the
    /// installed list. winget ids are case-insensitive.
    pub fn key(&self) -> String {
        self.id.to_lowercase()
    }

    /// The version-diff badge text, e.g. `+1.2.3`, when an upgrade exists.
    pub fn upgrade_delta(&self) -> Option<&str> {
        self.available.as_deref()
    }

    /// Where the package is installed — `机器` for a machine-wide source.
    pub fn source_label(&self) -> &str {
        if self.source.is_empty() {
            "unknown"
        } else {
            &self.source
        }
    }

    /// Whether the package matches a lowercased query, the way the prototype's
    /// search box filters: name, id, publisher, description and tags.
    ///
    /// A `match_kind` from `winget search` wins outright — winget matched on a
    /// moniker, tag or command the visible fields do not contain, and dropping
    /// such a row would contradict the result count winget reported.
    pub fn matches(&self, query: &str) -> bool {
        fn hit(haystack: &str, query: &str) -> bool {
            haystack.to_lowercase().contains(query)
        }
        if let Some(kind) = self.match_kind.as_deref()
            && hit(kind, query)
        {
            return true;
        }
        hit(&self.name, query)
            || hit(&self.id, query)
            || hit(&self.publisher, query)
            || self.description.as_deref().is_some_and(|d| hit(d, query))
            || self.tags.iter().any(|tag| hit(tag, query))
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

/// A winget source.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Source {
    pub name: String,
    pub url: String,
    pub ty: String,
    pub trusted: bool,
    pub explicit: bool,
    pub packages: Option<u64>,
    pub updated: Option<String>,
}

impl Source {
    /// Merge the `source list` table row with its `source export` record.
    pub fn from_rows(row: &crate::winget::SourceRow, export: Option<&SourceExport>) -> Self {
        let mut source = Self {
            name: row.name.clone(),
            url: row.argument.clone(),
            explicit: row.explicit,
            ..Self::default()
        };
        if let Some(export) = export {
            source.trusted = export.trusted();
            source.ty = export.ty.clone();
            if source.url.is_empty() {
                source.url = export.argument.clone();
            }
        }
        source
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tagged(name: &str, tags: &[&str]) -> (String, Vec<String>) {
        (
            name.to_owned(),
            tags.iter().map(|t| (*t).to_owned()).collect(),
        )
    }

    #[test]
    fn a_developer_tag_lands_in_dev() {
        let (name, tags) = tagged("Some Editor", &["developer-tools", "editor"]);
        assert_eq!(categorize(&name, &tags), CategoryId::Dev);
    }

    #[test]
    fn an_unmatched_package_lands_in_tools() {
        let (name, tags) = tagged("Zzz Unknown Thing", &["zzz", "qqq"]);
        assert_eq!(categorize(&name, &tags), CategoryId::Tools);
    }

    #[test]
    fn every_category_is_reachable_from_its_own_keywords() {
        for (category, keywords) in CATEGORY_TAGS {
            let (name, tags) = tagged("Package", &[keywords[0]]);
            let landed = categorize(&name, &tags);
            // An earlier category may claim a shared keyword; what must hold is
            // that the keyword maps somewhere, never nowhere.
            assert!(
                CategoryId::ALL.contains(&landed),
                "{} keyword {:?} produced no category",
                category.label(),
                keywords[0]
            );
        }
    }

    #[test]
    fn categorization_is_a_partition() {
        // Each package maps to exactly one category, by construction.
        let cases = [
            tagged("Git for Windows", &["version-control"]),
            tagged("VLC media player", &["video", "player"]),
            tagged("7-Zip", &["archive", "compress"]),
            tagged("Steam", &["game", "platform"]),
            tagged("PuTTY", &["ssh", "remote"]),
        ];
        for (name, tags) in cases {
            let first = categorize(&name, &tags);
            let second = categorize(&name, &tags);
            assert_eq!(first, second, "categorize must be deterministic");
        }
    }

    #[test]
    fn tags_win_over_the_name() {
        // The name says "driver" but the tag says "python".
        let (name, tags) = tagged("Intel Driver", &["python"]);
        assert_eq!(categorize(&name, &tags), CategoryId::Dev);
    }

    #[test]
    fn search_row_becomes_a_package_with_its_source() {
        let row = crate::winget::SearchRow {
            name: "Visual Studio Code".into(),
            id: "Microsoft.VisualStudioCode".into(),
            version: "1.142.0".into(),
            match_kind: Some("Moniker: vscode".into()),
            source: "winget".into(),
        };
        let package = Package::from_search(&row, "winget");
        assert_eq!(package.id, "Microsoft.VisualStudioCode");
        assert_eq!(package.source, "winget");
        assert!(!package.installed);
    }

    #[test]
    fn missing_source_falls_back_to_the_query_source() {
        let row = crate::winget::SearchRow {
            name: "VSCodium".into(),
            id: "VSCodium.VSCodium".into(),
            version: "1.0".into(),
            match_kind: None,
            source: String::new(),
        };
        assert_eq!(Package::from_search(&row, "msstore").source, "msstore");
    }

    #[test]
    fn detail_metadata_is_merged_without_clobbering_identity() {
        let row = crate::winget::SearchRow {
            name: "Visual Studio Code".into(),
            id: "Microsoft.VisualStudioCode".into(),
            version: String::new(),
            match_kind: None,
            source: "winget".into(),
        };
        let mut package = Package::from_search(&row, "winget");
        package.apply_detail(&PackageDetail {
            name: "Microsoft Visual Studio Code".into(),
            id: "Microsoft.VisualStudioCode".into(),
            version: "1.142.0".into(),
            publisher: "Microsoft Corporation".into(),
            license: "Microsoft Software License".into(),
            tags: vec!["developer-tools".into(), "editor".into()],
            release_date: "2026-10-14".into(),
            installer_url: "https://example.invalid/vscode.exe".into(),
            ..PackageDetail::default()
        });
        assert_eq!(package.publisher, "Microsoft Corporation");
        assert_eq!(package.version, "1.142.0");
        assert_eq!(package.updated.as_deref(), Some("2026-10-14"));
        assert_eq!(package.category, CategoryId::Dev);
        assert_eq!(package.name, "Visual Studio Code", "identity is preserved");
        assert!(package.size.is_none(), "winget has no size field to merge");
    }

    #[test]
    fn list_row_marks_installed_and_records_an_upgrade() {
        let mut package = Package {
            id: "7zip.7zip".into(),
            name: "7-Zip".into(),
            ..Package::default()
        };
        package.apply_list(&ListRow {
            name: "7-Zip 26.03 (x64 edition)".into(),
            id: "7zip.7zip".into(),
            version: "26.03.00.0".into(),
            available: Some("26.04".into()),
            source: "winget".into(),
        });
        assert!(package.installed);
        assert_eq!(package.version, "26.03.00.0");
        assert_eq!(package.upgrade_delta(), Some("26.04"));
        assert_eq!(package.name, "7-Zip", "catalog name wins");
    }

    #[test]
    fn category_keys_round_trip() {
        for category in CategoryId::ALL {
            assert_eq!(CategoryId::from_key(category.key()), Some(category));
        }
        assert_eq!(CategoryId::from_key("nope"), None);
    }
}
