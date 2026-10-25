//! The window shell: titlebar, sidebar, main pane, status bar, queue drawer.
//!
//! [`AppView`] owns the [`AppState`], the detected [`PackageBackend`] and every
//! in-flight task. Views render from a state snapshot and send changes back
//! through listeners, so no view talks to a package manager directly.

use std::sync::Arc;
use std::time::Duration;

use crate::backends::{self, PackageBackend, types};
use crate::config;
use crate::icons::{IconKey, icon};
use crate::model::Package;
use crate::queue::{self, Enqueued, QueueAction, QueueState};
use crate::state::{AppState, Route, SearchSource};
use crate::theme::{self, Palette};
use crate::views;
use crate::widgets;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::sidebar::{
    Sidebar, SidebarCollapsible, SidebarFooter, SidebarHeader, SidebarMenuItem,
};
use gpui_kit::component::{
    Side, StyledExt as _, TitleBar, WindowExt as _, badge::Badge, h_flex, v_flex,
};
use gpui_kit::prelude::{
    FluentBuilder as _, InteractiveElement as _, StatefulInteractiveElement as _,
};
use gpui_kit::{
    AnyElement, AppContext as _, ClipboardItem, Context, Entity, Focusable as _, Hsla, IntoElement,
    KeyBinding, ParentElement as _, Render, SharedString, Styled as _, Subscription, Task, Window,
    actions, div, px,
};

actions!(pakkit, [FocusSearch]);

/// The application view. One per window.
pub struct AppView {
    pub state: AppState,
    /// The detected package backend; `None` when its CLI could not be found.
    pub backend: Option<Arc<dyn PackageBackend>>,
    /// The settings tab's text fields. Created once, at construction, so the
    /// settings renderer only ever borrows them.
    pub search_input: Entity<InputState>,
    pub install_location_input: Entity<InputState>,
    pub proxy_input: Entity<InputState>,
    pub backend_path_input: Entity<InputState>,
    /// In-flight refreshes. Dropping a [`Task`] cancels it, so they are held
    /// here; finished ones are pruned when the next refresh starts. A single
    /// slot cannot be used — the startup source load would cancel the package
    /// load, and every mutation would cancel the refresh that follows it.
    refresh_tasks: Vec<Task<()>>,
    /// The queue's progress ticker.
    ticker: Option<Task<()>>,
    /// The search debounce timer, replaced on each keystroke.
    debounce: Option<Task<()>>,
    /// The in-flight keystroke search. Replaced on every query, so a stale
    /// search never runs to completion alongside its successor. (The blocking
    /// winget child itself cannot be killed once spawned — dropping the task
    /// only discards its result, and the epoch below guards the apply step.)
    search_task: Option<Task<()>>,
    /// Monotonic catalogue-query id, bumped by every search or full refresh.
    /// A result that arrives with an older id is dropped, so out-of-order
    /// completions cannot overwrite a newer query's list.
    search_epoch: u64,
    /// Cached `winget list` rows and installed ids from the last full refresh.
    /// Keystroke searches merge against these instead of re-running `list`
    /// and `export` on every pause.
    list_rows: Vec<types::ListRow>,
    installed_ids: Vec<String>,
    /// Package ids (lowercased) with a `winget show` fetch in flight, so a
    /// reopened detail dialog never spawns a duplicate.
    detail_inflight: std::collections::HashSet<String>,
    /// Hosts with a favicon fetch in flight.
    icon_inflight: std::collections::HashSet<String>,
    _subscriptions: Vec<Subscription>,
}

impl AppView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // The persisted settings decide the UI language and theme, so they are
        // applied before the first translated string or widget is built.
        let settings = config::load();
        rust_i18n::set_locale(&settings.language);
        theme::set_mode(cx, settings.theme);
        theme::set_accent(cx, &settings.accent);

        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!("search.placeholder")));
        let install_location_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("D:\\Software");
            input.set_value(settings.install_location.clone(), window, cx);
            input
        });
        let proxy_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("http://127.0.0.1:7890");
            input.set_value(settings.proxy.clone(), window, cx);
            input
        });
        let backend_path_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder(tr!("placeholder.backend_path"));
            input.set_value(settings.backend_path.clone(), window, cx);
            input
        });
        let state = AppState {
            settings,
            ..AppState::default()
        };
        let mut view = Self {
            state,
            backend: None,
            search_input: search_input.clone(),
            install_location_input: install_location_input.clone(),
            proxy_input: proxy_input.clone(),
            backend_path_input: backend_path_input.clone(),
            refresh_tasks: Vec::new(),
            ticker: None,
            debounce: None,
            search_task: None,
            search_epoch: 0,
            list_rows: Vec::new(),
            installed_ids: Vec::new(),
            detail_inflight: std::collections::HashSet::new(),
            icon_inflight: std::collections::HashSet::new(),
            _subscriptions: Vec::new(),
        };

        view._subscriptions.push(cx.subscribe_in(
            &search_input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let text = this.search_input.read(cx).value().to_string();
                    this.state.query = text;
                    this.state.search_page = 0;
                    // Typing from another route navigates here first; the query
                    // still has to be run, or the view filters a stale catalogue.
                    if this.state.route != Route::Search {
                        this.navigate(Route::Search, window, cx);
                    }
                    this.debounce_search(cx);
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    this.debounce = None;
                    this.navigate(Route::Search, window, cx);
                    this.refresh_search(cx);
                }
                InputEvent::Focus | InputEvent::Blur => cx.notify(),
            },
        ));

        for entity in [&install_location_input, &proxy_input, &backend_path_input] {
            view._subscriptions.push(cx.subscribe_in(
                entity,
                window,
                |this: &mut Self, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        let state = &mut this.state.settings;
                        state.install_location =
                            this.install_location_input.read(cx).value().to_string();
                        state.proxy = this.proxy_input.read(cx).value().to_string();
                        state.backend_path = this.backend_path_input.read(cx).value().to_string();
                        this.persist_settings();
                        cx.notify();
                    }
                },
            ));
        }

        cx.bind_keys([KeyBinding::new("ctrl-k", FocusSearch, Some("App"))]);
        view.start(cx);
        view
    }

    /// Resolve the backend, read its version, then load the first pages of data.
    fn start(&mut self, cx: &mut Context<Self>) {
        // The disk caches warm the detail/icon maps even when the backend is
        // missing, so this runs before the resolve below can early-return.
        self.load_cache(cx);
        let backend = match backends::detect(
            self.state.settings.backend_kind(),
            &self.state.settings.backend_path,
        ) {
            Ok(backend) => backend,
            Err(error) => {
                let message = error.localized();
                self.state.backend_error = Some(message.clone());
                self.state.status = message;
                return;
            }
        };
        self.state.backend_name = backend.cli_name().to_owned();
        self.state.backend_kind = backend.kind();
        self.state.capabilities = backend.capabilities();
        self.state.backend_error = None;
        self.backend = Some(backend.clone());

        self.spawn_refresh(cx.spawn(async move |this, cx| {
            let version = cx
                .background_spawn(async move { backend.version().unwrap_or_default() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.state.backend_version = version;
                this.refresh_packages(cx);
                this.refresh_sources(cx);
                cx.notify();
            });
        }));
    }

    /// Re-resolve the backend from the configured path, as 检测 does.
    pub fn redetect_backend(&mut self, cx: &mut Context<Self>) {
        match backends::detect(
            self.state.settings.backend_kind(),
            &self.state.settings.backend_path,
        ) {
            Ok(backend) => {
                self.state.backend_name = backend.cli_name().to_owned();
                self.state.backend_kind = backend.kind();
                self.state.capabilities = backend.capabilities();
                self.state.backend_error = None;
                self.state.status = tr!("status.backend_detected", backend = backend.cli_name());
                self.backend = Some(backend.clone());
                self.spawn_refresh(cx.spawn(async move |this, cx| {
                    let version = cx
                        .background_spawn(async move { backend.version().unwrap_or_default() })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.state.backend_version = version;
                        this.refresh_packages(cx);
                        this.refresh_sources(cx);
                        cx.notify();
                    });
                }));
            }
            Err(error) => {
                let message = error.localized();
                self.state.backend_error = Some(message.clone());
                self.state.status = message;
            }
        }
        cx.notify();
    }

    /// Switch the active backend, as the settings 包管理器 segment does. The
    /// change is persisted before detection runs, so a failed switch rolls the
    /// setting back and keeps the old backend serving.
    pub fn switch_backend(
        &mut self,
        kind: backends::BackendKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if kind == self.state.backend_kind {
            return;
        }
        let previous = self.state.settings.backend.clone();
        self.state.settings.backend = kind.cli_name().to_owned();
        self.persist_settings();

        let backend = match backends::detect(kind, &self.state.settings.backend_path) {
            Ok(backend) => backend,
            Err(error) => {
                let message = error.localized();
                self.state.backend_error = Some(message.clone());
                self.state.status = message;
                self.state.settings.backend = previous;
                self.persist_settings();
                cx.notify();
                return;
            }
        };

        // Drop everything the old backend produced: cached rows, in-flight
        // lookups, and queued-but-not-started jobs. Running queue items hold
        // their own backend Arc (see pump) and finish on the old CLI.
        self.state.packages.clear();
        self.state.details.clear();
        self.state.sources.clear();
        self.list_rows.clear();
        self.installed_ids.clear();
        self.detail_inflight.clear();
        self.icon_inflight.clear();
        self.state.search_source = SearchSource::All;
        self.state.search_page = 0;
        self.state.installed_page = 0;
        self.state.category_page = 0;
        self.state
            .queue
            .retain(|item| item.state != QueueState::Pending);
        self.search_task = None;
        self.search_epoch += 1;
        self.state.backend_version.clear();

        self.state.backend_name = backend.cli_name().to_owned();
        self.state.backend_kind = backend.kind();
        self.state.capabilities = backend.capabilities();
        self.state.backend_error = None;
        self.state.status = tr!("status.backend_detected", backend = backend.cli_name());
        self.backend = Some(backend.clone());

        if self.state.route == Route::Sources && !self.state.capabilities.sources_view {
            self.navigate(Route::Home, window, cx);
        }

        self.spawn_refresh(cx.spawn(async move |this, cx| {
            let version = cx
                .background_spawn(async move { backend.version().unwrap_or_default() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.state.backend_version = version;
                this.refresh_packages(cx);
                this.refresh_sources(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// The equivalent `<cli> search` line the search view shows.
    pub fn search_command(&self) -> String {
        let Some(backend) = &self.backend else {
            return String::new();
        };
        search_command(&self.state, &**backend)
    }

    /// The equivalent `<cli> install` line a package detail shows.
    pub fn install_command(&self, id: &str) -> String {
        let Some(backend) = &self.backend else {
            return String::new();
        };
        install_command(&self.state, id, &**backend)
    }

    // --------------------------------------------------------------- routing

    pub fn navigate(&mut self, route: Route, _window: &mut Window, cx: &mut Context<Self>) {
        if self.state.route == route {
            return;
        }
        self.state.set_route(route);
        // A different category (or any route change) restarts paging.
        self.state.category_page = 0;
        match route {
            Route::Updates => self.refresh_updates(cx),
            Route::Sources if self.state.sources.is_empty() => self.refresh_sources(cx),
            _ => cx.notify(),
        }
        cx.notify();
    }

    // -------------------------------------------------------------- searching

    /// Re-run the catalogue query after a short pause, replacing the timer.
    fn debounce_search(&mut self, cx: &mut Context<Self>) {
        self.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(180))
                .await;
            let _ = this.update(cx, |this, cx| this.refresh_search(cx));
        }));
    }

    /// Run only the backend's search, merging against the cached installed data.
    ///
    /// Keystrokes, Enter, the source pill and the advanced filters land here;
    /// the expensive installed-set sweep stays with [`Self::refresh_packages`]
    /// (startup, redetect), while install/uninstall mutations keep the caches
    /// in sync directly.
    pub fn refresh_search(&mut self, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            cx.notify();
            return;
        };
        let query = self.state.query.clone();
        let source = self.state.search_source.arg().to_owned();
        let filters = self.state.settings.search_filters();
        let cli_name = backend.cli_name();

        // Cancel the previous keystroke search and invalidate its result.
        self.search_task = None;
        self.search_epoch += 1;
        let epoch = self.search_epoch;

        // A search with an empty query matches nothing on winget — skip the
        // process entirely and show just the cached installed packages.
        // Backends with a full-catalog empty search (flatpak, apt) run it.
        if query.trim().is_empty() && !backend.empty_query_searches_catalog() {
            self.state.packages = merge_catalog(&[], &self.list_rows, &self.installed_ids, cli_name);
            self.state.status = self.state.status_for(self.state.route);
            cx.notify();
            return;
        }

        self.state.status = tr!("status.searching_catalog");
        cx.notify();

        self.search_task = Some(cx.spawn(async move |this, cx| {
            let search = cx
                .background_spawn(async move {
                    backend.search_packages(&query, Some(&source), filters, None)
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                if epoch != this.search_epoch {
                    // A newer query (or a full refresh) superseded this one.
                    return;
                }
                this.state.packages =
                    merge_catalog(&search, &this.list_rows, &this.installed_ids, cli_name);
                this.state.status = this.state.status_for(this.state.route);
                cx.notify();
            });
        }));
    }

    pub fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.search_input.focus_handle(cx);
        handle.focus(window, cx);
    }

    /// Switch the search source, as the titlebar pill and the segments do.
    pub fn set_search_source(
        &mut self,
        source: SearchSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.search_source == source {
            return;
        }
        self.state.search_source = source.clone();
        self.state.search_page = 0;
        window.push_notification(
            Notification::info(tr!("notification.source_switched", source = source.label())),
            cx,
        );
        self.refresh_search(cx);
    }

    /// Track a refresh future, pruning the ones that have already finished.
    fn spawn_refresh(&mut self, task: Task<()>) {
        self.refresh_tasks.retain(|task| !task.is_ready());
        self.refresh_tasks.push(task);
    }

    // ------------------------------------------------------------------ data

    pub fn refresh_packages(&mut self, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            cx.notify();
            return;
        };
        // The data every paged view renders is about to be replaced.
        self.state.search_page = 0;
        self.state.installed_page = 0;
        self.state.category_page = 0;
        let query = self.state.query.clone();
        let source = self.state.search_source.arg().to_owned();
        let filters = self.state.settings.search_filters();
        let cli_name = backend.cli_name();

        // Supersede any in-flight keystroke search — its result would be
        // stale once this full sweep lands.
        self.search_task = None;
        self.search_epoch += 1;
        let epoch = self.search_epoch;

        self.state.status = tr!("status.searching_catalog");
        cx.notify();

        self.spawn_refresh(cx.spawn(async move |this, cx| {
            let (search, list, installed) = cx
                .background_spawn(async move {
                    // An empty query matches nothing on winget; backends with
                    // a full-catalog empty search (flatpak, apt) run it.
                    let search = if query.trim().is_empty()
                        && !backend.empty_query_searches_catalog()
                    {
                        Vec::new()
                    } else {
                        backend.search_packages(&query, Some(&source), filters, None)
                    };

                    let list = backend.list_installed();
                    let installed = backend.managed_ids(&list);

                    (search, list, installed)
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                // The installed-data caches always move forward; the visible
                // list only when no newer query has been issued since.
                this.list_rows = list;
                this.installed_ids = installed;
                if epoch == this.search_epoch {
                    this.state.packages =
                        merge_catalog(&search, &this.list_rows, &this.installed_ids, cli_name);
                    this.state.status = this.state.status_for(this.state.route);
                }
                cx.notify();
            });
        }));
    }

    /// The backend's upgrades listing — the 更新 view's data.
    pub fn refresh_updates(&mut self, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let cli_name = backend.cli_name();
        let installed: Vec<String> = self
            .state
            .packages
            .iter()
            .filter(|p| p.installed)
            .map(|p| p.key())
            .collect();

        self.spawn_refresh(cx.spawn(async move |this, cx| {
            let rows = cx
                .background_spawn(async move { backend.list_upgrades() })
                .await;

            let _ = this.update(cx, |this, cx| {
                for pkg in &mut this.state.packages {
                    if pkg.installed {
                        pkg.available = None;
                    }
                }
                for row in &rows {
                    let key = row.id.to_lowercase();
                    if let Some(pkg) = this.state.packages.iter_mut().find(|p| p.key() == key) {
                        pkg.installed = true;
                        pkg.available = row.available.clone();
                    } else if !installed.contains(&key) {
                        let mut pkg = Package::from_search(
                            &types::SearchRow {
                                name: row.name.clone(),
                                id: row.id.clone(),
                                version: row.version.clone(),
                                match_kind: None,
                                source: row.source.clone(),
                                description: None,
                            },
                            cli_name,
                        );
                        pkg.installed = true;
                        pkg.available = row.available.clone();
                        this.state.packages.push(pkg);
                    }
                }
                this.state.status = tr!("status.check_updates");
                cx.notify();
            });
        }));
    }

    pub fn refresh_sources(&mut self, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        self.spawn_refresh(cx.spawn(async move |this, cx| {
            let sources = cx
                .background_spawn(async move { backend.list_sources().unwrap_or_default() })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.state.sources = sources;
                for source in &mut this.state.sources {
                    source.updated = Some(now_stamp());
                }
                // A default source the backend no longer offers (e.g. a
                // settings file carried across platforms) snaps to the first.
                if !this.state.sources.is_empty()
                    && !this
                        .state
                        .sources
                        .iter()
                        .any(|source| source.name == this.state.default_source)
                {
                    let first = this.state.sources[0].name.clone();
                    this.state.default_source = first.clone();
                    this.state.settings.default_source = first;
                    this.persist_settings();
                }
                cx.notify();
            });
        }));
    }

    /// The backend's `show` metadata for one package — the detail dialog's data.
    ///
    /// The parsed detail is cached in [`AppState::details`] and merged into
    /// the package row, so a reopened dialog renders instantly. No-op when
    /// the backend is missing, the detail is already cached, or a fetch for
    /// this package is in flight.
    pub fn fetch_detail(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let key = id.to_lowercase();
        if self.state.details.contains_key(&key) || !self.detail_inflight.insert(key.clone()) {
            return;
        }
        let source = self
            .state
            .packages
            .iter()
            .find(|p| p.key() == key)
            .map(|p| p.source.clone());
        let id = id.to_owned();
        let cache_key = key.clone();

        self.spawn_refresh(cx.spawn(async move |this, cx| {
            let detail = cx
                .background_spawn(async move {
                    backend
                        .show_package(&id, source.as_deref())
                        // Persist next to the in-memory cache so the next run
                        // renders details without re-running the backend.
                        .inspect(|detail| {
                            if let (Some(path), Ok(json)) = (
                                crate::cache::detail_path(&cache_key),
                                serde_json::to_string(detail),
                            ) {
                                let _ = crate::cache::atomic_write(&path, json.as_bytes());
                            }
                        })
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.detail_inflight.remove(&key);
                if let Some(detail) = detail {
                    if let Some(pkg) = this.state.packages.iter_mut().find(|p| p.key() == key) {
                        pkg.apply_detail(&detail);
                    }
                    // The detail names the favicon's host; start its fetch so
                    // the icon can appear while the dialog is still open.
                    if let Some(host) = crate::cache::host_of(&detail) {
                        this.fetch_icon(&host, cx);
                    }
                    this.state.details.insert(key, detail);
                }
                cx.notify();
            });
        }));
    }

    /// Warm the detail/icon maps from the on-disk caches, then fetch favicons
    /// for hosts the cached details point at but no icon is cached for yet.
    fn load_cache(&mut self, cx: &mut Context<Self>) {
        self.spawn_refresh(cx.spawn(async move |this, cx| {
            let (details, icons) = cx
                .background_spawn(async move {
                    (crate::cache::load_details(), crate::cache::load_icons())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                for detail in details {
                    this.state.details.insert(detail.id.to_lowercase(), detail);
                }
                for (host, state) in icons {
                    let state = match state {
                        Ok(path) => crate::state::IconState::Ready(path),
                        Err(()) => crate::state::IconState::Missing,
                    };
                    this.state.icons.insert(host, state);
                }
                let hosts: Vec<String> = this
                    .state
                    .details
                    .values()
                    .filter_map(crate::cache::host_of)
                    .filter(|host| !this.state.icons.contains_key(host))
                    .collect();
                for host in hosts {
                    this.fetch_icon(&host, cx);
                }
                cx.notify();
            });
        }));
    }

    /// Fetch the favicon for `host` into the disk cache. No-op when the state
    /// is already known (fetched or negative-cached) or a fetch is in flight.
    pub fn fetch_icon(&mut self, host: &str, cx: &mut Context<Self>) {
        if self.state.icons.contains_key(host) || !self.icon_inflight.insert(host.to_owned()) {
            return;
        }
        let host = host.to_owned();
        self.spawn_refresh(cx.spawn(async move |this, cx| {
            let bytes = {
                let host = host.clone();
                cx.background_spawn(async move { crate::cache::fetch_favicon(&host) })
                    .await
            };
            let _ = this.update(cx, |this, cx| {
                this.icon_inflight.remove(&host);
                let state = match bytes {
                    Some(bytes) => crate::cache::icon_path(&host).and_then(|path| {
                        crate::cache::atomic_write(&path, &bytes).ok()?;
                        Some(crate::state::IconState::Ready(path))
                    }),
                    None => {
                        if let Some(marker) = crate::cache::none_marker_path(&host) {
                            let _ = crate::cache::atomic_write(&marker, &[]);
                        }
                        Some(crate::state::IconState::Missing)
                    }
                };
                if let Some(state) = state {
                    this.state.icons.insert(host, state);
                }
                cx.notify();
            });
        }));
    }

    /// The backend's source refresh for one source (`None` = all).
    pub fn sync_sources(
        &mut self,
        name: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        self.state.status = tr!("status.sources_syncing");
        cx.notify();
        self.spawn_refresh(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let args = backend.source_update_args(name.as_deref());
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    backend.run_checked(&refs)
                })
                .await;

            let _ = this.update_in(cx, |this, window, cx| {
                match &result {
                    Ok(_) => window.push_notification(
                        Notification::success(tr!("notification.sources_synced")),
                        cx,
                    ),
                    Err(error) => {
                        window.push_notification(Notification::error(error.localized()), cx)
                    }
                }
                this.refresh_sources(cx);
                this.state.status = tr!("status.ready");
                cx.notify();
            });
        }));
    }

    // ----------------------------------------------------------------- queue

    /// Add a package to the queue and start it; every install button lands here.
    pub fn enqueue(
        &mut self,
        id: &str,
        action: QueueAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match queue::enqueue(&mut self.state, id, action) {
            Enqueued::Added(_) => {
                let name = self
                    .state
                    .packages
                    .iter()
                    .find(|p| p.key() == id.to_lowercase())
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| id.to_owned());
                self.state.queue_open = true;
                window.push_notification(
                    Notification::info(tr!("notification.enqueued", name = name)),
                    cx,
                );
                self.pump(cx);
            }
            Enqueued::Duplicate(name) => window.push_notification(
                Notification::warning(tr!("notification.duplicate", name = name)),
                cx,
            ),
            Enqueued::Unknown => {
                window.push_notification(Notification::error(tr!("notification.pkg_not_found")), cx)
            }
        }
        cx.notify();
    }

    /// Start every entry the concurrency limit allows.
    pub fn pump(&mut self, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        for ix in queue::startable(&self.state) {
            if queue::begin(&mut self.state, ix, &*backend).is_none() {
                continue;
            }
            let uid = self.state.queue[ix].uid;
            let item = self.state.queue[ix].clone();
            let args = queue::args_for(&item, &self.state.settings, &*backend);

            let runner = backend.clone();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_spawn({
                        let runner = runner.clone();
                        async move {
                            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                            runner.run(&refs)
                        }
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    queue::complete(&mut this.state, uid, result, &*runner);
                    // Keep the keystroke-search cache in step with the queue:
                    // a successful install joins the installed set.
                    if let Some(item) =
                        this.state
                            .queue
                            .iter()
                            .find(|item| item.uid == uid)
                            .filter(|item| {
                                item.state == QueueState::Done
                                    && item.action == QueueAction::Install
                            })
                    {
                        let key = item.id.to_lowercase();
                        if !this.installed_ids.contains(&key) {
                            this.installed_ids.push(key);
                        }
                    }
                    this.pump(cx);
                    cx.notify();
                });
            })
            .detach();
        }
        self.start_ticker(cx);
    }

    fn start_ticker(&mut self, cx: &mut Context<Self>) {
        if !self
            .state
            .queue
            .iter()
            .any(|item| item.state == QueueState::Running)
        {
            self.ticker = None;
            return;
        }
        self.ticker = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let running = this
                    .update(cx, |this, cx| {
                        let mut running = false;
                        for item in &mut this.state.queue {
                            if item.state == QueueState::Running {
                                queue::advance_progress(item);
                                running = true;
                            }
                        }
                        cx.notify();
                        running
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        }));
    }

    pub fn remove_queue_item(&mut self, uid: u64, cx: &mut Context<Self>) {
        queue::remove(&mut self.state, uid);
        cx.notify();
    }

    pub fn clear_finished(&mut self, cx: &mut Context<Self>) {
        queue::clear_finished(&mut self.state);
        cx.notify();
    }

    // -------------------------------------------------------------- settings

    /// 深色 → 浅色 → 跟随系统, as the sidebar footer row does.
    pub fn cycle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = self.state.settings.theme.next();
        self.state.settings.theme = next;
        theme::set_mode(cx, next);
        self.persist_settings();
        window.push_notification(
            Notification::info(tr!("notification.theme_switched", theme = next.label())),
            cx,
        );
        cx.notify();
    }

    pub fn set_accent(&mut self, hex: &str, cx: &mut Context<Self>) {
        self.state.settings.accent = hex.to_owned();
        theme::set_accent(cx, hex);
        self.persist_settings();
        cx.notify();
    }

    /// Write the current settings to disk; every mutation point calls this.
    pub fn persist_settings(&self) {
        config::save(&self.state.settings);
    }

    /// Run a source mutation, then refresh the list.
    pub fn run_source_op(
        &mut self,
        args: Vec<String>,
        success: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let success = success.to_owned();
        self.spawn_refresh(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    backend.run_checked(&refs)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                match &result {
                    Ok(_) => window.push_notification(Notification::success(success.clone()), cx),
                    Err(error) => {
                        window.push_notification(Notification::error(error.localized()), cx)
                    }
                }
                this.refresh_sources(cx);
            });
        }));
    }

    /// Run an uninstall and drop the package from the installed set on success.
    pub fn run_uninstall(
        &mut self,
        args: Vec<String>,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let id = id.to_owned();
        self.spawn_refresh(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn({
                    let backend = backend.clone();
                    async move {
                        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                        backend.run(&refs)
                    }
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let ok = result
                    .as_ref()
                    .map(|out| out.status == 0 || backend.exit_is_benign(out.status))
                    .unwrap_or(false);
                if ok {
                    this.state.mark_uninstalled(&id);
                    this.installed_ids.retain(|key| key != &id.to_lowercase());
                    window.push_notification(
                        Notification::success(tr!("notification.uninstalled")),
                        cx,
                    );
                } else {
                    let message = result
                        .map(|out| {
                            let note = if backend.exit_needs_elevation(out.status) {
                                tr!("queue.elevation_note")
                            } else {
                                String::new()
                            };
                            tr!(
                                "notification.uninstall_failed_code",
                                code = backend.format_exit_code(out.status),
                                desc = backend.describe_exit(out.status),
                                note = note
                            )
                        })
                        .unwrap_or_else(|error| {
                            tr!("notification.uninstall_failed", error = error.localized())
                        });
                    window.push_notification(Notification::error(message), cx);
                }
                cx.notify();
            });
        }));
    }
    pub fn copy(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
        window.push_notification(Notification::success(tr!("notification.copied")), cx);
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let collapsed = window.viewport_size().width < px(1280.);

        v_flex()
            .key_context("App")
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.focus_search(window, cx);
            }))
            .size_full()
            .bg(p.bg)
            .text_color(p.ink)
            .child(self.titlebar(&p, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(&p, collapsed, window, cx))
                    .child(self.main_pane(&p, cx))
                    .when(self.state.queue_open, |this| {
                        this.child(self.queue_drawer(&p, cx))
                    }),
            )
    }
}

impl AppView {
    /// `TitleBar` supplies the drag area and the OS window-control hitboxes;
    /// it is re-declared 52 px tall to match `index.html`.
    fn titlebar(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let source = self.state.search_source.clone();
        TitleBar::new()
            .h(px(52.))
            .px(px(10.))
            .bg(p.surface)
            // .child(brand_row(p))
            .child(
                div().flex_1().flex().items_center().justify_center().child(
                    div().w_full().max_w(px(560.)).h(px(34.)).child(
                        Input::new(&self.search_input)
                            .prefix(icon(IconKey::Search, px(15.), p.muted).into_any_element())
                            .suffix(source_pill(p, source, cx).into_any_element()),
                    ),
                ),
            )
    }

    fn sidebar(
        &self,
        p: &Palette,
        collapsed: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.state.route.sidebar_key();
        let queue_count = queue::active_count(&self.state);
        let theme_label = self.state.settings.theme.label();

        let routes: [(&str, String, IconKey, Route); 7] = [
            ("home", tr!("nav.home"), IconKey::Home, Route::Home),
            (
                "categories",
                tr!("nav.categories"),
                IconKey::Grid,
                Route::Categories,
            ),
            ("search", tr!("nav.search"), IconKey::Search, Route::Search),
            (
                "installed",
                tr!("nav.installed"),
                IconKey::Check,
                Route::Installed,
            ),
            (
                "updates",
                tr!("nav.updates"),
                IconKey::Refresh,
                Route::Updates,
            ),
            (
                "sources",
                tr!("nav.sources"),
                IconKey::Source,
                Route::Sources,
            ),
            (
                "settings",
                tr!("nav.settings"),
                IconKey::Settings,
                Route::Settings,
            ),
        ];

        Sidebar::new("app-sidebar")
            .side(Side::Left)
            .w(if collapsed { px(64.) } else { px(224.) })
            .collapsible(SidebarCollapsible::Icon)
            .header(SidebarHeader::new().child(brand_row(p)))
            .footer(
                // `SidebarFooter` lays children out in an `h_flex`; stack the
                // theme and queue rows vertically instead.
                SidebarFooter::new()
                    .flex_col()
                    .child(nav_row(
                        p,
                        collapsed,
                        IconKey::Theme,
                        &format!("{} · {theme_label}", tr!("nav.theme")),
                        None,
                        cx.listener(|this, _, window, cx| this.cycle_theme(window, cx)),
                    ))
                    .child(nav_row(
                        p,
                        collapsed,
                        IconKey::Queue,
                        &tr!("nav.queue"),
                        Some(queue_count),
                        cx.listener(|this, _, _, cx| {
                            this.state.queue_open = !this.state.queue_open;
                            cx.notify();
                        }),
                    )),
            )
            .children(
                routes
                    .into_iter()
                    .filter(|(key, ..)| {
                        *key != "sources" || self.state.capabilities.sources_view
                    })
                    .map(|(key, label, glyph, route)| {
                        SidebarMenuItem::new(label)
                            .min_h(px(38.))
                            .icon(icon(glyph, px(16.), p.muted))
                            .active(active == key)
                            .collapsed(collapsed)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.navigate(route, window, cx);
                            }))
                    }),
            )
            .border_r_1()
            .h_full()
    }

    /// The input entities the settings view embeds, cloned out of `self` so
    /// rendering never re-borrows the `AppView`.
    fn shell_inputs(&self) -> views::ShellInputs {
        views::ShellInputs {
            install_location: self.install_location_input.clone(),
            proxy: self.proxy_input.clone(),
            backend_path: self.backend_path_input.clone(),
        }
    }

    fn main_pane(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .child(
                div()
                    .id("content")
                    .flex_1()
                    .min_h_0()
                    // Search and category detail own their scroll region
                    // (the card grid) and pin their pager, so the pane must
                    // clip instead of scroll.
                    .when(
                        matches!(
                            self.state.route,
                            Route::Search | Route::Category(_)
                        ),
                        |this| this.overflow_hidden(),
                    )
                    .when(
                        !matches!(
                            self.state.route,
                            Route::Search | Route::Category(_)
                        ),
                        |this| this.overflow_y_scroll(),
                    )
                    .child(views::render(
                        &self.state,
                        &self.shell_inputs(),
                        &self.search_command(),
                        cx,
                    )),
            )
            .child(self.status_bar(p))
    }

    fn status_bar(&self, p: &Palette) -> impl IntoElement {
        let queue_len = queue::active_count(&self.state);
        let version = if self.state.backend_version.is_empty() {
            self.state.backend_name.clone()
        } else {
            format!("{} {}", self.state.backend_name, self.state.backend_version)
        };
        let dot = if self.state.backend_error.is_some() {
            p.danger
        } else {
            p.ok
        };

        h_flex()
            .h(px(28.))
            .flex_none()
            .px(px(12.))
            .gap(px(10.))
            .text_size(px(11.5))
            .text_color(p.muted)
            .border_t_1()
            .border_color(p.line)
            .bg(Hsla {
                a: 0.6,
                ..p.surface
            })
            .child(div().size(px(7.)).rounded(px(99.)).bg(dot))
            .child(self.state.status.clone())
            .child(div().text_color(p.line_strong).child("|"))
            .child(tr!(
                "status.sources_count",
                count = self.state.sources.len()
            ))
            .child(tr!("status.queue_count", count = queue_len))
            .child(
                h_flex()
                    .ml_auto()
                    .gap(px(6.))
                    .child(icon(IconKey::Terminal, px(12.), p.muted))
                    .child(version)
                    .child(widgets::pill(p, widgets::PillKind::Line, "x64")),
            )
    }

    fn queue_drawer(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.weak_entity();
        v_flex()
            .w(px(360.))
            .flex_none()
            .h_full()
            .border_l_1()
            .border_color(p.line)
            .bg(Hsla {
                a: 0.85,
                ..p.surface
            })
            .child(
                h_flex()
                    .h(px(48.))
                    .flex_none()
                    .px(px(14.))
                    .gap(px(8.))
                    .border_b_1()
                    .border_color(p.line)
                    .child(icon(IconKey::Queue, px(16.), p.accent))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_semibold()
                            .child(tr!("queue.title")),
                    )
                    .child(widgets::pill(
                        p,
                        widgets::PillKind::Accent,
                        queue::active_count(&self.state).to_string(),
                    ))
                    .child(
                        div()
                            .ml_auto()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .child(
                                widgets::btn_ghost(cx, p, "queue-clear", tr!("queue.clear"))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.clear_finished(cx)),
                                    ),
                            )
                            .child(
                                widgets::icon_btn("queue-close", IconKey::Close, 26., p.muted)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.queue_open = false;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .id("queue-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .when(self.state.queue.is_empty(), |this| {
                        this.child(
                            div()
                                .text_size(px(12.5))
                                .text_color(p.muted)
                                .child(tr!("queue.empty")),
                        )
                    })
                    .children(self.state.queue.iter().map(|item| {
                        let pkg = self
                            .state
                            .packages
                            .iter()
                            .find(|p| p.key() == item.id.to_lowercase());
                        queue_row(p, item, pkg, view.clone())
                    })),
            )
            .child(
                h_flex()
                    .flex_none()
                    .p(px(10.))
                    .gap(px(6.))
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .border_t_1()
                    .border_color(p.line)
                    .child(icon(IconKey::Info, px(12.), p.muted))
                    .child(tr!("queue.serial_hint")),
            )
    }
}

/// A sidebar-footer row: icon, label, and an optional count badge. `Sidebar`'s
/// own items land in the scroll area, so the footer keeps its own row shape.
fn nav_row(
    p: &Palette,
    collapsed: bool,
    glyph: IconKey,
    label: &str,
    count: Option<usize>,
    on_click: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut gpui_kit::App) + 'static,
) -> AnyElement {
    h_flex()
        .id(SharedString::from(format!("nav-{label}")))
        .w_full()
        .h(px(34.))
        .px(px(8.))
        .gap(px(8.))
        .rounded(px(8.))
        .cursor_pointer()
        .hover(move |this| this.bg(p.surface2))
        .child(icon(glyph, px(16.), p.muted))
        .when(!collapsed, |this| {
            this.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.5))
                    .text_color(p.ink2)
                    .truncate()
                    .child(label.to_owned()),
            )
        })
        .when_some(count.filter(|n| *n > 0), |this, count| {
            this.child(Badge::new().count(count).color(p.accent))
        })
        .on_click(on_click)
        .into_any_element()
}

fn queue_row(
    p: &Palette,
    item: &crate::queue::QueueItem,
    pkg: Option<&Package>,
    view: gpui_kit::WeakEntity<AppView>,
) -> AnyElement {
    let uid = item.uid;
    let (color, label) = match item.state {
        QueueState::Pending => (p.muted, tr!("queue.row.pending")),
        QueueState::Running => (p.accent, tr!("queue.row.running")),
        QueueState::Done => (p.ok, tr!("queue.row.done")),
        QueueState::Error => (p.danger, tr!("queue.row.error")),
    };
    let glyph = pkg.map_or(IconKey::Package, |p| p.category.icon());
    let hex = pkg.map_or("#4f8cff", |p| p.category.color());

    widgets::card(p)
        .p(px(10.))
        .child(
            h_flex()
                .gap(px(8.))
                .child(widgets::tinted_chip(p, glyph, hex, 30., 15.))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_medium()
                                .truncate()
                                .child(item.name.clone()),
                        )
                        .child(div().text_size(px(11.)).text_color(color).child(label)),
                )
                .child(
                    widgets::icon_btn("queue-remove", IconKey::Close, 24., p.muted).on_click(
                        move |_, _, cx| {
                            let _ = view.update(cx, |this, cx| this.remove_queue_item(uid, cx));
                        },
                    ),
                ),
        )
        .child(
            div()
                .mt(px(8.))
                .child(widgets::progress_bar(p, item.progress)),
        )
        .child(
            div()
                .mt(px(6.))
                .text_size(px(11.))
                .font_family("Cascadia Code")
                .text_color(p.muted)
                .truncate()
                .child(item.log.clone()),
        )
        .into_any_element()
}

/// The 28 px gradient tile with the Pakkit logo glyph.
fn brand_mark(p: &Palette) -> impl IntoElement {
    div()
        .size(px(28.))
        .flex_none()
        .rounded(px(9.))
        .flex()
        .items_center()
        .justify_center()
        .bg(p.accent)
        .child(icon(IconKey::Logo, px(15.), p.on_accent))
}

/// The wordmark row: gradient tile, name, prototype pill.
fn brand_row(p: &Palette) -> impl IntoElement {
    h_flex()
        .gap(px(8.))
        .flex_none()
        .child(brand_mark(p))
        .child(div().text_size(px(13.)).font_semibold().child("Pakkit"))
        .child(widgets::pill(
            p,
            widgets::PillKind::Line,
            tr!("app.prototype_pill"),
        ))
}

/// The pill showing (and cycling) the active search source.
fn source_pill(p: &Palette, source: SearchSource, cx: &mut Context<AppView>) -> impl IntoElement {
    div()
        .id("source-pill")
        .mr(px(6.))
        .text_size(px(11.))
        .px(px(7.))
        .py(px(3.))
        .rounded(px(6.))
        .bg(p.surface3)
        .border_1()
        .border_color(p.line)
        .text_color(p.ink2)
        .whitespace_nowrap()
        .cursor_pointer()
        .on_click(cx.listener(|this, _, window, cx| {
            let next = this.state.next_search_source();
            this.set_search_source(next, window, cx);
        }))
        .child(source.label())
}

/// The equivalent `<cli> search` line for the current query and filters.
fn search_command(state: &AppState, backend: &dyn PackageBackend) -> String {
    let args = backend.search_args(
        state.query.trim(),
        Some(state.search_source.arg()),
        state.settings.search_filters(),
        None,
    );
    format!("{} {}", backend.cli_name(), args.join(" "))
}

/// The equivalent `<cli> install` line for the current install settings.
fn install_command(state: &AppState, id: &str, backend: &dyn PackageBackend) -> String {
    let args = backend.install_args(id, &state.settings.install_options());
    format!("{} {}", backend.cli_name(), args.join(" "))
}

/// Merge the catalogue search rows with the installed set and upgrade list.
///
/// A search with an empty query matches nothing, so the installed packages
/// the backend's listing reports must be merged in on their own — otherwise
/// 已安装 and 更新 are empty on a cold start. `fallback_source` (the CLI's
/// name) labels rows whose own source column is empty.
fn merge_catalog(
    search: &[types::SearchRow],
    list: &[types::ListRow],
    installed: &[String],
    fallback_source: &str,
) -> Vec<Package> {
    // Hash indexes keep this linear: a full-catalog backend (apt's empty
    // search) returns ~80k rows, and a quadratic scan over them blocks the
    // UI thread for minutes.
    let installed: std::collections::HashSet<&str> =
        installed.iter().map(String::as_str).collect();
    let by_id: std::collections::HashMap<String, &types::ListRow> =
        list.iter().map(|row| (row.id.to_lowercase(), row)).collect();

    let mut packages: Vec<Package> = Vec::with_capacity(search.len());
    let mut seen: std::collections::HashSet<String> =
        std::collections::HashSet::with_capacity(search.len());
    for row in search {
        let key = row.id.to_lowercase();
        if !seen.insert(key.clone()) {
            continue;
        }
        let mut pkg = Package::from_search(row, fallback_source);
        pkg.installed = installed.contains(key.as_str());
        if let Some(found) = by_id.get(&key) {
            pkg.available = found.available.clone();
            if !found.version.is_empty() {
                pkg.version = found.version.clone();
            }
        }
        packages.push(pkg);
    }

    // Only backend-managed rows: for winget the export set excludes
    // ARP/MSIX-only entries, which have no source and no version to offer an
    // upgrade against.
    for row in list {
        let key = row.id.to_lowercase();
        if !installed.contains(key.as_str()) || !seen.insert(key) {
            continue;
        }
        let mut pkg = Package::from_search(
            &types::SearchRow {
                name: row.name.clone(),
                id: row.id.clone(),
                version: row.version.clone(),
                match_kind: None,
                source: row.source.clone(),
                description: None,
            },
            fallback_source,
        );
        pkg.installed = true;
        pkg.available = row.available.clone();
        packages.push(pkg);
    }

    packages
}

/// `HH:MM:SS` for the source cards' sync time.
fn now_stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let secs = now % 86_400;
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

/// SharedString is used by the view signatures; keep the import honest.
const _: Option<SharedString> = None;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::types::{ListRow, SearchRow};

    fn search_row(id: &str, name: &str) -> SearchRow {
        SearchRow {
            name: name.into(),
            id: id.into(),
            version: "1.0".into(),
            match_kind: None,
            source: "winget".into(),
            description: None,
        }
    }

    fn list_row(id: &str, name: &str, available: Option<&str>) -> ListRow {
        ListRow {
            name: name.into(),
            id: id.into(),
            version: "1.0".into(),
            available: available.map(Into::into),
            source: "winget".into(),
        }
    }

    #[test]
    fn installed_packages_survive_an_empty_search() {
        // `winget search` without a query matches nothing, so a cold start
        // hands `merge_catalog` no search rows at all. The installed packages
        // must still reach the 已安装 and 更新 views.
        let installed = vec!["git.git".to_owned()];
        let list = vec![list_row("Git.Git", "Git", Some("2.0"))];

        let packages = merge_catalog(&[], &list, &installed, "winget");

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "Git.Git");
        assert!(packages[0].installed);
        assert_eq!(packages[0].available.as_deref(), Some("2.0"));
    }

    #[test]
    fn unmanaged_list_rows_are_not_offered_as_installed_packages() {
        // `winget list` also reports ARP/MSIX entries winget cannot upgrade;
        // they are absent from the export set and must not appear.
        let list = vec![list_row("ARP\\Machine\\X64\\{GUID}", "Adobe Acrobat", None)];

        let packages = merge_catalog(&[], &list, &[], "winget");

        assert!(packages.is_empty(), "got {packages:?}");
    }

    #[test]
    fn a_search_row_is_not_duplicated_by_the_installed_pass() {
        let installed = vec!["git.git".to_owned()];
        let search = vec![search_row("Git.Git", "Git")];
        let list = vec![list_row("Git.Git", "Git", None)];

        let packages = merge_catalog(&search, &list, &installed, "winget");

        assert_eq!(packages.len(), 1);
        assert!(packages[0].installed);
    }

    #[test]
    fn a_full_catalog_merge_stays_linear() {
        // apt's empty-query search returns the whole repository (~80k rows);
        // a quadratic merge blocked the UI thread for minutes. This scale
        // completes in milliseconds with hash indexes.
        let search: Vec<_> = (0..80_000)
            .map(|ix| search_row(&format!("pkg{ix}"), &format!("Package {ix}")))
            .collect();
        let installed: Vec<String> = (0..3_000).map(|ix| format!("pkg{ix}")).collect();
        let list: Vec<_> = (0..3_000)
            .map(|ix| list_row(&format!("pkg{ix}"), &format!("Package {ix}"), None))
            .collect();

        let start = std::time::Instant::now();
        let packages = merge_catalog(&search, &list, &installed, "apt");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "the merge took {:?}",
            start.elapsed()
        );

        assert_eq!(packages.len(), 80_000);
        assert!(packages[0].installed);
        assert!(!packages[80_000 - 1].installed);
    }

    /// A winget handle for building command lines; nothing is ever spawned.
    fn backend() -> crate::backends::winget::Winget {
        crate::backends::winget::Winget::new("winget.exe")
    }

    #[test]
    fn install_command_line_reflects_the_settings() {
        let state = AppState {
            settings: crate::state::Settings {
                default_source: "winget".into(),
                ..crate::state::Settings::default()
            },
            ..AppState::default()
        };

        let command = install_command(&state, "Microsoft.PowerToys", &backend());
        assert!(command.starts_with("winget install --id Microsoft.PowerToys"));
        assert!(command.contains("--source winget"));
        assert!(command.contains("--silent"));
        assert!(
            !command.contains("--architecture") && !command.contains("--scope"),
            "installs must not force an architecture or scope"
        );
    }

    #[test]
    fn search_command_line_names_the_selected_source() {
        let mut state = AppState {
            query: "vscode".into(),
            ..AppState::default()
        };
        let command = search_command(&state, &backend());
        assert!(command.starts_with("winget search -q vscode"));
        assert!(
            !command.contains("--source"),
            "全部源 must not pass a --source flag"
        );

        state.search_source = SearchSource::Named("msstore".into());
        assert!(search_command(&state, &backend()).contains("--source msstore"));
    }
}
