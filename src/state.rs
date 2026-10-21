//! Application state: routing, filters, settings, and the package inventory.
//!
//! Everything here is plain data plus pure logic, so the parts that decide
//! *what* the UI shows (filtering, sorting, status text, command lines) are
//! unit-testable without a window. [`crate::shell::AppView`] owns one of these
//! and drives the live winget calls.

use serde::{Deserialize, Serialize};

use crate::model::{CategoryId, Package, Source};
use crate::theme::ModeChoice;
use crate::winget::{Arch, InstallOptions, Scope, SearchFilters};

/// The route table, mirroring the prototype's router. `Categories` carries the
/// selected category, exactly as its `category` route does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Home,
    Categories,
    Category(CategoryId),
    Search,
    Installed,
    Updates,
    Sources,
    Settings,
}

impl Route {
    /// Whether the sidebar's 分类 entry is the active one for this route.
    pub const fn sidebar_key(self) -> &'static str {
        match self {
            Route::Home => "home",
            Route::Categories | Route::Category(_) => "categories",
            Route::Search => "search",
            Route::Installed => "installed",
            Route::Updates => "updates",
            Route::Sources => "sources",
            Route::Settings => "settings",
        }
    }
}

/// The source a search targets. `All` drops the `--source` flag (see
/// [`crate::winget::search`], which treats `all` as "every source").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchSource {
    #[default]
    Winget,
    MsStore,
    All,
}

impl SearchSource {
    /// The value passed to `--source`.
    pub const fn arg(self) -> &'static str {
        match self {
            SearchSource::Winget => "winget",
            SearchSource::MsStore => "msstore",
            SearchSource::All => crate::winget::SOURCE_ALL,
        }
    }

    /// The label the titlebar pill and the search segments show.
    pub fn label(self) -> String {
        match self {
            SearchSource::Winget => "winget".to_owned(),
            SearchSource::MsStore => "msstore".to_owned(),
            SearchSource::All => tr!("source.all"),
        }
    }

    /// `winget` → `msstore` → `all`, the titlebar pill's cycle.
    pub const fn next(self) -> Self {
        match self {
            SearchSource::Winget => SearchSource::MsStore,
            SearchSource::MsStore => SearchSource::All,
            SearchSource::All => SearchSource::Winget,
        }
    }

    /// The source options the search filter card offers, live sources first.
    pub fn all() -> [SearchSource; 3] {
        [
            SearchSource::Winget,
            SearchSource::MsStore,
            SearchSource::All,
        ]
    }
}

/// The 全部 / 已安装 / 未安装 filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusFilter {
    #[default]
    All,
    Installed,
    NotInstalled,
}

impl StatusFilter {
    pub const ALL: [StatusFilter; 3] = [
        StatusFilter::All,
        StatusFilter::Installed,
        StatusFilter::NotInstalled,
    ];

    pub fn label(self) -> String {
        match self {
            StatusFilter::All => tr!("filter.all"),
            StatusFilter::Installed => tr!("filter.installed"),
            StatusFilter::NotInstalled => tr!("filter.not_installed"),
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            StatusFilter::All => "all",
            StatusFilter::Installed => "installed",
            StatusFilter::NotInstalled => "notinstalled",
        }
    }
}

/// The search view's sort segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMode {
    #[default]
    Relevance,
    Rating,
    Updated,
}

impl SortMode {
    pub const ALL: [SortMode; 3] = [SortMode::Relevance, SortMode::Rating, SortMode::Updated];

    pub fn label(self) -> String {
        match self {
            SortMode::Relevance => tr!("sort.relevance"),
            SortMode::Rating => tr!("sort.rating"),
            SortMode::Updated => tr!("sort.updated"),
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            SortMode::Relevance => "relevance",
            SortMode::Rating => "rating",
            SortMode::Updated => "updated",
        }
    }
}

/// The category-detail sort segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CategorySort {
    #[default]
    Popular,
    Rating,
    Updated,
    Name,
}

impl CategorySort {
    pub const ALL: [CategorySort; 4] = [
        CategorySort::Popular,
        CategorySort::Rating,
        CategorySort::Updated,
        CategorySort::Name,
    ];

    pub fn label(self) -> String {
        match self {
            CategorySort::Popular => tr!("sort.popular"),
            CategorySort::Rating => tr!("sort.rating"),
            CategorySort::Updated => tr!("sort.updated"),
            CategorySort::Name => tr!("sort.name"),
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            CategorySort::Popular => "hot",
            CategorySort::Rating => "rating",
            CategorySort::Updated => "updated",
            CategorySort::Name => "name",
        }
    }
}

/// A UI language the app ships, in the order the settings segments show them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    ZhCN,
    ZhTW,
    En,
    Ja,
    Ko,
}

impl Language {
    pub const ALL: [Language; 5] = [
        Language::ZhCN,
        Language::ZhTW,
        Language::En,
        Language::Ja,
        Language::Ko,
    ];

    /// The BCP-47 tag rust-i18n and the locale filenames use.
    pub const fn tag(self) -> &'static str {
        match self {
            Language::ZhCN => "zh-CN",
            Language::ZhTW => "zh-TW",
            Language::En => "en",
            Language::Ja => "ja",
            Language::Ko => "ko",
        }
    }

    /// The native endonym the selector shows — language names are never
    /// translated, so a user can always find their own.
    pub const fn native_name(self) -> &'static str {
        match self {
            Language::ZhCN => "简体中文",
            Language::ZhTW => "繁體中文",
            Language::En => "English",
            Language::Ja => "日本語",
            Language::Ko => "한국어",
        }
    }

    pub fn from_tag(tag: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|l| l.tag() == tag)
            .unwrap_or_default()
    }
}

/// Every option the settings screen writes, and the single source of truth for
/// the command line the queue runs.
///
/// Persisted as JSON (see [`crate::config`]); `#[serde(default)]` keeps a
/// partial or older file loadable by filling new fields from [`Default`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub default_source: String,
    pub arch: Arch,
    pub scope: Scope,
    pub install_location: String,
    pub accept_agreements: bool,
    pub silent: bool,
    pub auto_check_updates: bool,
    pub concurrency: u8,
    pub theme: ModeChoice,
    pub accent: String,
    pub reduce_motion: bool,
    pub winget_path: String,
    pub proxy: String,
    pub language: String,
    pub telemetry: bool,
    /// The 高级筛选 flags; none on by default, as the prototype's checkboxes.
    pub filters: SearchFilters,
}

impl Default for Settings {
    /// The prototype's `S.settings`, with `winget_path` left empty so
    /// [`Winget::resolve`] auto-detects until the app fills it in.
    fn default() -> Self {
        Self {
            default_source: "winget".to_owned(),
            arch: Arch::X64,
            scope: Scope::User,
            install_location: String::new(),
            accept_agreements: true,
            silent: true,
            auto_check_updates: false,
            concurrency: 1,
            theme: ModeChoice::Dark,
            accent: crate::theme::ACCENT_SWATCHES[0].to_owned(),
            reduce_motion: false,
            winget_path: String::new(),
            proxy: String::new(),
            language: "zh-CN".to_owned(),
            telemetry: false,
            filters: SearchFilters::default(),
        }
    }
}

impl Settings {
    /// The install options the queue derives from these settings.
    pub fn install_options(&self) -> InstallOptions {
        InstallOptions {
            source: Some(self.default_source.clone()),
            arch: self.arch,
            scope: self.scope,
            location: Some(self.install_location.trim().to_owned()).filter(|s| !s.is_empty()),
            silent: self.silent,
            accept: self.accept_agreements,
        }
    }

    /// The search filters the advanced-search dialog writes.
    pub fn search_filters(&self) -> SearchFilters {
        self.filters
    }

    /// Concurrency clamped to the segment control's range.
    pub fn effective_concurrency(&self) -> usize {
        self.concurrency.clamp(1, 3) as usize
    }

    /// Replace the advanced-search flags, as the 高级筛选 dialog does.
    pub fn set_search_filters(&mut self, filters: SearchFilters) {
        self.filters = filters;
    }
}

/// A host's favicon, as known to the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconState {
    /// Fetched favicon bytes on disk; gpui renders this path directly.
    Ready(std::path::PathBuf),
    /// The fetch failed (negative cache); keep showing the category chip.
    Missing,
}

/// Everything the views read.
#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub route: Route,
    pub query: String,
    pub search_source: SearchSource,
    pub search_filter: StatusFilter,
    pub sort: SortMode,
    pub category_sort: CategorySort,
    pub packages: Vec<Package>,
    /// Cached `winget show` output per package id (lowercased), filled when a
    /// detail dialog opens. The search/list rows carry no description, license
    /// or installer metadata — only `show` provides them.
    pub details: std::collections::HashMap<String, crate::winget::PackageDetail>,
    /// Favicon state per host, filled from the disk cache at startup and as
    /// fetches land. See [`crate::cache`].
    pub icons: std::collections::HashMap<String, IconState>,
    pub sources: Vec<Source>,
    pub default_source: String,
    pub settings: Settings,
    pub queue: Vec<crate::queue::QueueItem>,
    /// Next queue handle. Monotonic so a removed entry's handle is never
    /// reused by a later one.
    pub queue_uid: u64,
    pub queue_open: bool,
    pub status: String,
    pub winget_version: String,
    /// Why winget is unavailable, when it is. The app still runs and every
    /// data view shows an empty state.
    pub winget_error: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            route: Route::Home,
            query: String::new(),
            search_source: SearchSource::Winget,
            search_filter: StatusFilter::All,
            packages: Vec::new(),
            details: std::collections::HashMap::new(),
            icons: std::collections::HashMap::new(),
            sources: Vec::new(),
            sort: SortMode::Relevance,
            category_sort: CategorySort::Popular,
            queue: Vec::new(),
            queue_uid: 0,
            default_source: "winget".to_owned(),
            settings: Settings::default(),
            queue_open: false,
            status: tr!("status.ready"),
            winget_version: String::new(),
            winget_error: None,
        }
    }
}

impl AppState {
    /// The cached favicon path for a package, when its detail is cached, the
    /// detail names a host, and that host's icon has been fetched. `None`
    /// keeps the category chip.
    pub fn icon_path_for(&self, pkg_id: &str) -> Option<std::path::PathBuf> {
        let detail = self.details.get(&pkg_id.to_lowercase())?;
        let host = crate::cache::host_of(detail)?;
        match self.icons.get(&host) {
            Some(IconState::Ready(path)) => Some(path.clone()),
            _ => None,
        }
    }

    /// All packages assigned to `category`.
    pub fn category_packages(&self, category: CategoryId) -> Vec<&Package> {
        let mut list: Vec<&Package> = self
            .packages
            .iter()
            .filter(|p| p.category == category)
            .collect();
        sort_by(&mut list, self.category_sort);
        list
    }

    /// How many packages the category tile's footer shows.
    pub fn category_count(&self, category: CategoryId) -> usize {
        self.packages
            .iter()
            .filter(|p| p.category == category)
            .count()
    }

    /// The search view's result list: status filter, then the text query, then
    /// the sort.
    pub fn filtered(&self) -> Vec<&Package> {
        let query = self.query.trim().to_lowercase();
        let mut list: Vec<&Package> = self
            .packages
            .iter()
            .filter(|p| match self.search_filter {
                StatusFilter::All => true,
                StatusFilter::Installed => p.installed,
                StatusFilter::NotInstalled => !p.installed,
            })
            .filter(|p| query.is_empty() || p.matches(&query))
            .collect();
        sort_by(
            &mut list,
            match self.sort {
                SortMode::Relevance => CategorySort::Popular,
                SortMode::Rating => CategorySort::Rating,
                SortMode::Updated => CategorySort::Updated,
            },
        );
        list
    }

    /// The installed view's rows, name-ordered for a stable list.
    pub fn installed(&self) -> Vec<&Package> {
        let mut list: Vec<&Package> = self.packages.iter().filter(|p| p.installed).collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// Packages with a newer version available.
    pub fn updates(&self) -> Vec<&Package> {
        let mut list: Vec<&Package> = self
            .packages
            .iter()
            .filter(|p| p.installed && p.available.is_some())
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// The packages the home view's 高分精选 grid shows.
    pub fn featured(&self) -> Vec<&Package> {
        let mut list: Vec<&Package> = self
            .packages
            .iter()
            .filter(|p| p.rating.unwrap_or(0.0) >= 4.7)
            .collect();
        sort_by(&mut list, CategorySort::Rating);
        list.truncate(6);
        list
    }

    /// The home view's 最近更新 rows.
    pub fn recently_updated(&self) -> Vec<&Package> {
        let mut list: Vec<&Package> = self.packages.iter().collect();
        sort_by(&mut list, CategorySort::Updated);
        list.truncate(5);
        list
    }

    /// The packages the home view's stat tiles count.
    pub fn installed_count(&self) -> usize {
        self.packages.iter().filter(|p| p.installed).count()
    }

    /// The equivalent `winget search` line the search view shows.
    pub fn search_command(&self) -> String {
        let args = crate::winget::search(
            self.query.trim(),
            Some(self.search_source.arg()),
            self.settings.search_filters(),
            None,
        );
        format!("winget {}", args.join(" "))
    }

    /// The equivalent `winget install` line a package detail shows.
    pub fn install_command(&self, id: &str) -> String {
        let args = crate::winget::install(id, &self.settings.install_options());
        format!("winget {}", args.join(" "))
    }

    /// The `winget export` document for the installed packages, pretty-printed
    /// so the dialog can show (and copy) real JSON.
    pub fn export_skeleton(&self) -> String {
        let packages: Vec<serde_json::Value> = self
            .installed()
            .iter()
            .map(|pkg| {
                serde_json::json!({
                    "PackageIdentifier": pkg.id,
                    "Version": pkg.version,
                })
            })
            .collect();
        let document = serde_json::json!({
            "$schema": "https://aka.ms/winget-packages.schema.2.0.json",
            "CreationDate": "",
            "Sources": [
                {
                    "Packages": packages,
                    "SourceDetails": {
                        "Argument": "https://cdn.winget.microsoft.com/cache",
                        "Identifier": "Microsoft.Winget.Source_8wekyb3d8bbwe",
                        "Name": "winget",
                        "Type": "Microsoft.PreIndexed.Package"
                    }
                }
            ],
            "WinGetVersion": self.winget_version,
        });
        serde_json::to_string_pretty(&document).unwrap_or_default()
    }

    /// Take the next queue handle. Monotonic by construction.
    pub fn next_queue_uid(&mut self) -> u64 {
        self.queue_uid += 1;
        self.queue_uid
    }

    /// Point the router at `route` and refresh the status bar text.
    pub fn set_route(&mut self, route: Route) {
        self.route = route;
        self.status = self.status_for(route);
    }

    /// The status-bar text for `route`, matching the prototype's `render()`.
    pub fn status_for(&self, route: Route) -> String {
        match route {
            Route::Home => tr!("status.ready"),
            Route::Categories => tr!("status.categories_count", count = CategoryId::ALL.len()),
            Route::Category(id) => tr!("status.category_item", name = id.label()),
            Route::Search => {
                if self.query.trim().is_empty() {
                    tr!("status.all_packages")
                } else {
                    tr!("status.searching", query = self.query.trim())
                }
            }
            Route::Installed => tr!("status.installed_mgmt"),
            Route::Updates => tr!("status.check_updates"),
            Route::Sources => tr!("status.sources_count", count = self.sources.len()),
            Route::Settings => tr!("status.settings"),
        }
    }

    /// The catalog row count the hero's 可用包 tile shows.
    pub fn catalog_label(&self) -> String {
        match self.packages.len() {
            0 => "—".to_owned(),
            n if n >= 1000 => format!("{:.1}k", n as f64 / 1000.),
            n => n.to_string(),
        }
    }

    /// Apply a `winget list`-derived upgrade to a package. Used after an
    /// upgrade succeeds so the row drops out of the updates view immediately.
    pub fn mark_update_applied(&mut self, id: &str) {
        if let Some(pkg) = self
            .packages
            .iter_mut()
            .find(|p| p.key() == id.to_lowercase())
            && let Some(available) = pkg.available.take()
        {
            pkg.version = available;
        }
    }

    /// Mark a package installed, the way `pump` does on success.
    pub fn mark_installed(&mut self, id: &str) {
        if let Some(pkg) = self
            .packages
            .iter_mut()
            .find(|p| p.key() == id.to_lowercase())
        {
            pkg.installed = true;
        }
    }

    /// Mark a package uninstalled.
    pub fn mark_uninstalled(&mut self, id: &str) {
        if let Some(pkg) = self
            .packages
            .iter_mut()
            .find(|p| p.key() == id.to_lowercase())
        {
            pkg.installed = false;
            pkg.available = None;
        }
    }
}

/// Sort `list` in place, always tie-breaking on name so the order is stable
/// across refreshes.
fn sort_by(list: &mut [&Package], mode: CategorySort) {
    list.sort_by(|a, b| {
        let primary = match mode {
            CategorySort::Popular => b
                .installed
                .cmp(&a.installed)
                .then_with(|| rating(b).total_cmp(&rating(a))),
            CategorySort::Rating => rating(b).total_cmp(&rating(a)),
            CategorySort::Updated => b.updated.cmp(&a.updated),
            CategorySort::Name => std::cmp::Ordering::Equal,
        };
        primary.then_with(|| a.name.cmp(&b.name))
    });
}

fn rating(pkg: &Package) -> f32 {
    pkg.rating.unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winget::SOURCE_ALL;

    fn pkg(id: &str, name: &str, category: CategoryId) -> Package {
        Package {
            id: id.to_owned(),
            name: name.to_owned(),
            category,
            ..Package::default()
        }
    }

    #[test]
    fn search_source_cycles_winget_msstore_all() {
        assert_eq!(SearchSource::Winget.next(), SearchSource::MsStore);
        assert_eq!(SearchSource::MsStore.next(), SearchSource::All);
        assert_eq!(SearchSource::All.next(), SearchSource::Winget);
        assert_eq!(SearchSource::All.arg(), SOURCE_ALL);
    }

    #[test]
    fn status_text_matches_the_prototype_router() {
        crate::ensure_zh_locale();
        let mut state = AppState::default();
        assert_eq!(state.status_for(Route::Home), "就绪");
        assert_eq!(state.status_for(Route::Categories), "分类 8");
        assert_eq!(
            state.status_for(Route::Category(CategoryId::Dev)),
            "分类 · 编程开发"
        );
        assert_eq!(state.status_for(Route::Search), "全部软件包");
        state.query = "vscode".into();
        assert_eq!(state.status_for(Route::Search), "搜索：vscode");
        assert_eq!(state.status_for(Route::Installed), "已安装管理");
        assert_eq!(state.status_for(Route::Updates), "检查更新");
        assert_eq!(state.status_for(Route::Settings), "设置");
        assert_eq!(state.status_for(Route::Sources), "源 0");
    }

    #[test]
    fn set_route_also_refreshes_the_status_text() {
        crate::ensure_zh_locale();
        let mut state = AppState::default();
        state.set_route(Route::Updates);
        assert_eq!(state.route, Route::Updates);
        assert_eq!(state.status, "检查更新");
    }

    #[test]
    fn sidebar_highlight_treats_a_category_route_as_categories() {
        assert_eq!(Route::Category(CategoryId::Dev).sidebar_key(), "categories");
        assert_eq!(Route::Categories.sidebar_key(), "categories");
        assert_eq!(Route::Home.sidebar_key(), "home");
    }

    #[test]
    fn status_filter_partitions_on_installed_state() {
        let mut state = AppState {
            packages: vec![
                pkg("A", "Alpha", CategoryId::Dev),
                pkg("B", "Beta", CategoryId::Dev),
            ],
            ..AppState::default()
        };
        state.packages[0].installed = true;

        state.search_filter = StatusFilter::Installed;
        assert_eq!(state.filtered().len(), 1);
        assert_eq!(state.filtered()[0].id, "A");

        state.search_filter = StatusFilter::NotInstalled;
        assert_eq!(state.filtered().len(), 1);
        assert_eq!(state.filtered()[0].id, "B");

        state.search_filter = StatusFilter::All;
        assert_eq!(state.filtered().len(), 2);
    }

    /// One query per field, since `matches` is a case-insensitive substring
    /// test over name, id, publisher, description and tags.
    #[test]
    fn query_matches_name_id_publisher_description_and_tags() {
        let mut state = AppState::default();
        let mut vscode = pkg(
            "Microsoft.VisualStudioCode",
            "Visual Studio Code",
            CategoryId::Dev,
        );
        vscode.publisher = "Microsoft Corporation".into();
        vscode.description = Some("Code editor".into());
        vscode.tags = vec!["editor".into()];
        state.packages = vec![vscode, pkg("Mozilla.Firefox", "Firefox", CategoryId::Net)];

        for query in [
            "visualstudiocode", // id
            "visual studio",    // name
            "corporation",      // publisher
            "code editor",      // description
            "editor",           // tags
        ] {
            state.query = query.to_owned();
            let hits = state.filtered();
            assert_eq!(hits.len(), 1, "{query} must match exactly one package");
            assert_eq!(hits[0].id, "Microsoft.VisualStudioCode");
        }

        state.query = "fire".into();
        assert_eq!(state.filtered()[0].id, "Mozilla.Firefox");
    }

    #[test]
    fn an_empty_query_lists_every_package() {
        let mut state = AppState {
            packages: vec![
                pkg("A", "Alpha", CategoryId::Dev),
                pkg("B", "Beta", CategoryId::Dev),
            ],
            ..AppState::default()
        };
        state.query = "   ".into();
        assert_eq!(state.filtered().len(), 2);
    }

    #[test]
    fn relevance_sort_promotes_installed_packages_then_rating() {
        let mut state = AppState::default();
        let mut a = pkg("A", "Alpha", CategoryId::Dev);
        a.rating = Some(4.0);
        let mut b = pkg("B", "Beta", CategoryId::Dev);
        b.rating = Some(4.9);
        let mut c = pkg("C", "Gamma", CategoryId::Dev);
        c.rating = Some(3.0);
        c.installed = true;
        state.packages = vec![a, b, c];

        state.sort = SortMode::Relevance;
        let ids: Vec<&str> = state.filtered().iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["C", "B", "A"],
            "installed first, then rating desc"
        );
    }

    #[test]
    fn sorting_is_stable_on_name_for_equal_keys() {
        let mut state = AppState::default();
        let mut a = pkg("A", "Zeta", CategoryId::Dev);
        a.rating = Some(5.0);
        let mut b = pkg("B", "Alpha", CategoryId::Dev);
        b.rating = Some(5.0);
        state.packages = vec![a, b];

        state.sort = SortMode::Rating;
        let names: Vec<&str> = state.filtered().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "Zeta"]);
    }

    #[test]
    fn category_packages_are_a_partition() {
        let state = AppState {
            packages: vec![
                pkg("A", "Alpha", CategoryId::Dev),
                pkg("B", "Beta", CategoryId::Office),
                pkg("C", "Gamma", CategoryId::Dev),
            ],
            ..AppState::default()
        };
        assert_eq!(state.category_packages(CategoryId::Dev).len(), 2);
        assert_eq!(state.category_packages(CategoryId::Office).len(), 1);
        assert_eq!(state.category_packages(CategoryId::Game).len(), 0);
        assert_eq!(state.category_count(CategoryId::Dev), 2);

        let total: usize = CategoryId::ALL
            .into_iter()
            .map(|c| state.category_count(c))
            .sum();
        assert_eq!(
            total,
            state.packages.len(),
            "every package has one category"
        );
    }

    #[test]
    fn category_sort_name_is_alphabetical() {
        let mut state = AppState {
            packages: vec![
                pkg("A", "Zeta", CategoryId::Dev),
                pkg("B", "Alpha", CategoryId::Dev),
            ],
            ..AppState::default()
        };
        state.category_sort = CategorySort::Name;
        let names: Vec<&str> = state
            .category_packages(CategoryId::Dev)
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, vec!["Alpha", "Zeta"]);
    }

    #[test]
    fn installed_and_updates_views_agree_with_the_flags() {
        let state = AppState {
            packages: {
                let mut a = pkg("A", "Alpha", CategoryId::Dev);
                a.installed = true;
                let mut b = pkg("B", "Beta", CategoryId::Dev);
                b.installed = true;
                b.available = Some("2.0".into());
                let c = pkg("C", "Gamma", CategoryId::Dev);
                vec![a, b, c]
            },
            ..AppState::default()
        };

        assert_eq!(state.installed().len(), 2);
        assert_eq!(state.installed_count(), 2);
        let updates = state.updates();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].id, "B");
    }

    #[test]
    fn featured_keeps_only_high_ratings_and_caps_at_six() {
        let mut state = AppState {
            packages: (0..8)
                .map(|i| {
                    let mut p = pkg(&format!("P{i}"), &format!("Pkg {i}"), CategoryId::Dev);
                    p.rating = Some(5.0);
                    p
                })
                .collect(),
            ..AppState::default()
        };
        state.packages.push({
            let mut low = pkg("Low", "Low", CategoryId::Dev);
            low.rating = Some(3.0);
            low
        });
        let featured = state.featured();
        assert_eq!(featured.len(), 6);
        assert!(featured.iter().all(|p| p.rating.unwrap_or(0.0) >= 4.7));
    }

    #[test]
    fn install_options_follow_the_settings() {
        let settings = Settings {
            default_source: "msstore".into(),
            arch: Arch::Arm64,
            scope: Scope::Machine,
            silent: false,
            install_location: "  D:\\Software  ".into(),
            ..Settings::default()
        };

        let options = settings.install_options();
        assert_eq!(options.source.as_deref(), Some("msstore"));
        assert_eq!(options.arch, Arch::Arm64);
        assert_eq!(options.scope, Scope::Machine);
        assert!(!options.silent);
        assert_eq!(options.location.as_deref(), Some("D:\\Software"));
    }

    #[test]
    fn a_blank_location_is_not_passed_to_winget() {
        let settings = Settings {
            install_location: "   ".into(),
            ..Settings::default()
        };
        assert_eq!(settings.install_options().location, None);
    }

    #[test]
    fn install_command_line_reflects_the_settings() {
        let state = AppState {
            settings: Settings {
                default_source: "winget".into(),
                arch: Arch::X86,
                scope: Scope::User,
                ..Settings::default()
            },
            ..AppState::default()
        };

        let command = state.install_command("Microsoft.PowerToys");
        assert!(command.starts_with("winget install --id Microsoft.PowerToys"));
        assert!(command.contains("--source winget"));
        assert!(command.contains("--architecture x86"));
        assert!(command.contains("--scope user"));
        assert!(command.contains("--silent"));
    }

    #[test]
    fn search_command_line_names_the_selected_source() {
        let mut state = AppState {
            query: "vscode".into(),
            search_source: SearchSource::All,
            ..AppState::default()
        };
        let command = state.search_command();
        assert!(command.starts_with("winget search -q vscode"));
        assert!(
            !command.contains("--source"),
            "全部源 must not pass a --source flag"
        );

        state.search_source = SearchSource::MsStore;
        assert!(state.search_command().contains("--source msstore"));
    }

    #[test]
    fn concurrency_is_clamped_to_the_segment_range() {
        let mut settings = Settings {
            concurrency: 0,
            ..Settings::default()
        };
        assert_eq!(settings.effective_concurrency(), 1);
        settings.concurrency = 9;
        assert_eq!(settings.effective_concurrency(), 3);
        settings.concurrency = 2;
        assert_eq!(settings.effective_concurrency(), 2);
    }

    #[test]
    fn marking_updates_and_installs_moves_rows_between_views() {
        let mut state = AppState {
            packages: {
                let mut b = pkg("B", "Beta", CategoryId::Dev);
                b.installed = true;
                b.available = Some("2.0".into());
                b.version = "1.0".into();
                vec![b]
            },
            ..AppState::default()
        };

        assert_eq!(state.updates().len(), 1);
        state.mark_update_applied("b");
        assert_eq!(state.updates().len(), 0, "id matching is case-insensitive");
        assert_eq!(state.packages[0].version, "2.0");

        state.mark_uninstalled("B");
        assert!(!state.packages[0].installed);
        assert_eq!(state.packages[0].available, None);

        state.mark_installed("b");
        assert!(state.packages[0].installed);
    }

    #[test]
    fn catalog_label_abbreviates_large_catalogs() {
        let mut state = AppState::default();
        assert_eq!(state.catalog_label(), "—");
        state.packages = vec![pkg("A", "A", CategoryId::Dev)];
        assert_eq!(state.catalog_label(), "1");
        state.packages = (0..11_800)
            .map(|i| pkg(&format!("P{i}"), "p", CategoryId::Dev))
            .collect();
        assert_eq!(state.catalog_label(), "11.8k");
    }
}
