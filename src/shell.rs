//! The window shell: titlebar, sidebar, main pane, status bar, queue drawer.
//!
//! [`AppView`] owns the [`AppState`], the resolved [`Winget`] handle and every
//! in-flight task. Views render from a state snapshot and send changes back
//! through listeners, so no view talks to winget directly.

use std::time::Duration;

use crate::icons::{IconKey, icon};
use crate::model::Package;
use crate::queue::{self, Enqueued, QueueAction, QueueState};
use crate::state::{AppState, Route, SearchSource};
use crate::theme::{self, Palette};
use crate::views;
use crate::widgets;
use crate::winget::{self, Winget};
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

actions!(wingetgui, [FocusSearch]);

/// The application view. One per window.
pub struct AppView {
    pub state: AppState,
    /// The resolved winget executable; `None` when it could not be found.
    pub winget: Option<Winget>,
    /// The settings tab's text fields. Created once, at construction, so the
    /// settings renderer only ever borrows them.
    pub search_input: Entity<InputState>,
    pub install_location_input: Entity<InputState>,
    pub proxy_input: Entity<InputState>,
    pub winget_path_input: Entity<InputState>,
    /// In-flight refreshes. Dropping a [`Task`] cancels it, so they are held
    /// here; finished ones are pruned when the next refresh starts. A single
    /// slot cannot be used — the startup source load would cancel the package
    /// load, and every mutation would cancel the refresh that follows it.
    refresh_tasks: Vec<Task<()>>,
    /// The queue's progress ticker.
    ticker: Option<Task<()>>,
    /// The search debounce timer, replaced on each keystroke.
    debounce: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl AppView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("搜索软件包…   (Ctrl+K)"));
        let install_location_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("D:\\Software"));
        let proxy_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("http://127.0.0.1:7890"));
        let winget_path_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("留空则自动检测"));
        let mut view = Self {
            state: AppState::default(),
            winget: None,
            search_input: search_input.clone(),
            install_location_input: install_location_input.clone(),
            proxy_input: proxy_input.clone(),
            winget_path_input: winget_path_input.clone(),
            refresh_tasks: Vec::new(),
            ticker: None,
            debounce: None,
            _subscriptions: Vec::new(),
        };

        view._subscriptions.push(cx.subscribe_in(
            &search_input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let text = this.search_input.read(cx).value().to_string();
                    this.state.query = text;
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
                    this.refresh_packages(cx);
                }
                InputEvent::Focus | InputEvent::Blur => cx.notify(),
            },
        ));

        for entity in [&install_location_input, &proxy_input, &winget_path_input] {
            view._subscriptions.push(cx.subscribe_in(
                entity,
                window,
                |this: &mut Self, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        let state = &mut this.state.settings;
                        state.install_location =
                            this.install_location_input.read(cx).value().to_string();
                        state.proxy = this.proxy_input.read(cx).value().to_string();
                        state.winget_path = this.winget_path_input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ));
        }

        cx.bind_keys([KeyBinding::new("ctrl-k", FocusSearch, Some("App"))]);
        view.start(cx);
        view
    }

    /// Resolve winget, read its version, then load the first pages of data.
    fn start(&mut self, cx: &mut Context<Self>) {
        let Ok(winget) = Winget::resolve(&self.state.settings.winget_path) else {
            let message = Winget::resolve(&self.state.settings.winget_path)
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default();
            self.state.winget_error = Some(message.clone());
            self.state.status = message;
            return;
        };
        self.winget = Some(winget.clone());
        self.state.winget_error = None;

        self.spawn_refresh(cx.spawn(async move |this, cx| {
            let probe = winget.clone();
            let version = cx
                .background_spawn(async move { probe.version().unwrap_or_default() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.state.winget_version = version;
                this.refresh_packages(cx);
                this.refresh_sources(cx);
                cx.notify();
            });
        }));
    }

    /// Re-resolve winget from the configured path, as 检测 does.
    pub fn redetect_winget(&mut self, cx: &mut Context<Self>) {
        match Winget::resolve(&self.state.settings.winget_path) {
            Ok(winget) => {
                self.winget = Some(winget.clone());
                self.state.winget_error = None;
                self.state.status = "已检测到 winget".to_owned();
                self.spawn_refresh(cx.spawn(async move |this, cx| {
                    let probe = winget.clone();
                    let version = cx
                        .background_spawn(async move { probe.version().unwrap_or_default() })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.state.winget_version = version;
                        this.refresh_packages(cx);
                        cx.notify();
                    });
                }));
            }
            Err(error) => {
                let message = error.to_string();
                self.state.winget_error = Some(message.clone());
                self.state.status = message;
            }
        }
        cx.notify();
    }

    // --------------------------------------------------------------- routing

    pub fn navigate(&mut self, route: Route, _window: &mut Window, cx: &mut Context<Self>) {
        if self.state.route == route {
            return;
        }
        self.state.set_route(route);
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
            let _ = this.update(cx, |this, cx| this.refresh_packages(cx));
        }));
    }

    pub fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.search_input.focus_handle(cx);
        handle.focus(window, cx);
    }

    /// `winget → msstore → 全部源`, as the titlebar pill and the segments do.
    pub fn set_search_source(
        &mut self,
        source: SearchSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.search_source == source {
            return;
        }
        self.state.search_source = source;
        window.push_notification(
            Notification::info(format!("搜索源：{}", source.label())),
            cx,
        );
        self.refresh_packages(cx);
    }

    /// Track a refresh future, pruning the ones that have already finished.
    fn spawn_refresh(&mut self, task: Task<()>) {
        self.refresh_tasks.retain(|task| !task.is_ready());
        self.refresh_tasks.push(task);
    }

    // ------------------------------------------------------------------ data

    pub fn refresh_packages(&mut self, cx: &mut Context<Self>) {
        let Some(winget) = self.winget.clone() else {
            cx.notify();
            return;
        };
        let query = self.state.query.clone();
        let source = self.state.search_source.arg().to_owned();
        let settings = self.state.settings.clone();

        self.state.status = "正在检索…".to_owned();
        cx.notify();

        self.spawn_refresh(cx.spawn(async move |this, cx| {
            let (search, list, installed) = cx
                .background_spawn(async move {
                    let args =
                        winget::search(&query, Some(&source), settings.search_filters(), None);
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    let search = winget
                        .run(&refs)
                        .map(|out| winget::parse_search(&out.stdout))
                        .unwrap_or_default();

                    let args = winget::list(None);
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    let list = winget
                        .run(&refs)
                        .map(|out| winget::parse_list(&out.stdout))
                        .unwrap_or_default();

                    (search, list, export_installed(&winget))
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.state.packages = merge_catalog(&search, &list, &installed);
                this.state.status = this.state.status_for(this.state.route);
                cx.notify();
            });
        }));
    }

    /// `winget list --upgrade-available` — the 更新 view's data.
    pub fn refresh_updates(&mut self, cx: &mut Context<Self>) {
        let Some(winget) = self.winget.clone() else {
            return;
        };
        let installed: Vec<String> = self
            .state
            .packages
            .iter()
            .filter(|p| p.installed)
            .map(|p| p.key())
            .collect();

        self.spawn_refresh(cx.spawn(async move |this, cx| {
            let rows = cx
                .background_spawn(async move {
                    let args = winget::list_upgrade_available(false, false);
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    winget
                        .run(&refs)
                        .map(|out| winget::parse_list(&out.stdout))
                        .unwrap_or_default()
                })
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
                            &winget::SearchRow {
                                name: row.name.clone(),
                                id: row.id.clone(),
                                version: row.version.clone(),
                                match_kind: None,
                                source: row.source.clone(),
                            },
                            "winget",
                        );
                        pkg.installed = true;
                        pkg.available = row.available.clone();
                        this.state.packages.push(pkg);
                    }
                }
                this.state.status = "检查更新".to_owned();
                cx.notify();
            });
        }));
    }

    pub fn refresh_sources(&mut self, cx: &mut Context<Self>) {
        let Some(winget) = self.winget.clone() else {
            return;
        };
        self.spawn_refresh(cx.spawn(async move |this, cx| {
            let sources = cx
                .background_spawn(async move {
                    let args = winget::source_list();
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    let rows = winget
                        .run(&refs)
                        .map(|out| winget::parse_source_list(&out.stdout))
                        .unwrap_or_default();

                    let args = winget::source_export(None);
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    let exports = winget
                        .run(&refs)
                        .ok()
                        .and_then(|out| winget::parse_source_export(&out.stdout).ok())
                        .unwrap_or_default();

                    rows.iter()
                        .map(|row| {
                            let export = exports.iter().find(|e| e.name == row.name);
                            let mut source = crate::model::Source::from_rows(row, export);
                            source.updated = Some(now_stamp());
                            source
                        })
                        .collect::<Vec<_>>()
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.state.sources = sources;
                cx.notify();
            });
        }));
    }

    /// `winget source update` for one source (`None` = all).
    pub fn sync_sources(
        &mut self,
        name: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(winget) = self.winget.clone() else {
            return;
        };
        self.state.status = "正在同步源…".to_owned();
        cx.notify();
        self.spawn_refresh(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let args = winget::source_update(name.as_deref());
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    winget.run_checked(&refs)
                })
                .await;

            let _ = this.update_in(cx, |this, window, cx| {
                match &result {
                    Ok(_) => window.push_notification(Notification::success("源已同步"), cx),
                    Err(error) => {
                        window.push_notification(Notification::error(error.to_string()), cx)
                    }
                }
                this.refresh_sources(cx);
                this.state.status = "就绪".to_owned();
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
                window.push_notification(Notification::info(format!("已加入队列：{name}")), cx);
                self.pump(cx);
            }
            Enqueued::Duplicate(name) => {
                window.push_notification(Notification::warning(format!("{name} 已在队列中")), cx)
            }
            Enqueued::Unknown => {
                window.push_notification(Notification::error("未找到该软件包"), cx)
            }
        }
        cx.notify();
    }

    /// Start every entry the concurrency limit allows.
    pub fn pump(&mut self, cx: &mut Context<Self>) {
        let Some(winget) = self.winget.clone() else {
            return;
        };
        for ix in queue::startable(&self.state) {
            if queue::begin(&mut self.state, ix).is_none() {
                continue;
            }
            let uid = self.state.queue[ix].uid;
            let item = self.state.queue[ix].clone();
            let args = queue::args_for(&item, &self.state.settings);

            let runner = winget.clone();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_spawn(async move {
                        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                        runner.run(&refs)
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    queue::complete(&mut this.state, uid, result);
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
        window.push_notification(Notification::info(format!("主题：{}", next.label())), cx);
        cx.notify();
    }

    pub fn set_accent(&mut self, hex: &str, cx: &mut Context<Self>) {
        self.state.settings.accent = hex.to_owned();
        theme::set_accent(cx, hex);
        cx.notify();
    }

    /// Run a `source …` mutation, then refresh the list.
    pub fn run_source_op(
        &mut self,
        args: Vec<String>,
        success: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(winget) = self.winget.clone() else {
            return;
        };
        let success = success.to_owned();
        self.spawn_refresh(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    winget.run_checked(&refs)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                match &result {
                    Ok(_) => window.push_notification(Notification::success(success.clone()), cx),
                    Err(error) => {
                        window.push_notification(Notification::error(error.to_string()), cx)
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
        let Some(winget) = self.winget.clone() else {
            return;
        };
        let id = id.to_owned();
        self.spawn_refresh(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    winget.run(&refs)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let ok = result.as_ref().map(|out| out.success()).unwrap_or(false);
                if ok {
                    this.state.mark_uninstalled(&id);
                    window.push_notification(Notification::success("已卸载"), cx);
                } else {
                    let message = result
                        .map(|out| {
                            let note = if winget::exit::needs_elevation(out.status) {
                                "（需要管理员权限）"
                            } else {
                                ""
                            };
                            format!(
                                "卸载失败：{} {}{note}",
                                winget::exit::format_code(out.status),
                                winget::exit::describe(out.status)
                            )
                        })
                        .unwrap_or_else(|error| format!("卸载失败：{error}"));
                    window.push_notification(Notification::error(message), cx);
                }
                cx.notify();
            });
        }));
    }
    pub fn copy(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
        window.push_notification(Notification::success("已复制到剪贴板"), cx);
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
        let source = self.state.search_source;
        TitleBar::new()
            .h(px(52.))
            .px(px(10.))
            .bg(p.surface)
            .child(brand_row(p))
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

        let routes = [
            ("home", "发现", IconKey::Home, Route::Home),
            ("categories", "分类", IconKey::Grid, Route::Categories),
            ("search", "全部包", IconKey::Search, Route::Search),
            ("installed", "已安装", IconKey::Check, Route::Installed),
            ("updates", "更新", IconKey::Refresh, Route::Updates),
            ("sources", "源管理", IconKey::Source, Route::Sources),
            ("settings", "设置", IconKey::Settings, Route::Settings),
        ];

        Sidebar::new("app-sidebar")
            .side(Side::Left)
            .w(if collapsed { px(64.) } else { px(224.) })
            .collapsible(SidebarCollapsible::Icon)
            .header(SidebarHeader::new().child(brand_row(p)))
            .footer(
                SidebarFooter::new()
                    .child(nav_row(
                        p,
                        collapsed,
                        IconKey::Theme,
                        &format!("主题 · {theme_label}"),
                        None,
                        cx.listener(|this, _, window, cx| this.cycle_theme(window, cx)),
                    ))
                    .child(nav_row(
                        p,
                        collapsed,
                        IconKey::Queue,
                        "安装队列",
                        Some(queue_count),
                        cx.listener(|this, _, _, cx| {
                            this.state.queue_open = !this.state.queue_open;
                            cx.notify();
                        }),
                    )),
            )
            .children(routes.into_iter().map(|(key, label, glyph, route)| {
                SidebarMenuItem::new(label)
                    .icon(icon(glyph, px(16.), p.muted))
                    .active(active == key)
                    .collapsed(collapsed)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.navigate(route, window, cx);
                    }))
            }))
            .border_r_1()
            .h_full()
    }

    /// The input entities the settings view embeds, cloned out of `self` so
    /// rendering never re-borrows the `AppView`.
    fn shell_inputs(&self) -> views::ShellInputs {
        views::ShellInputs {
            install_location: self.install_location_input.clone(),
            proxy: self.proxy_input.clone(),
            winget_path: self.winget_path_input.clone(),
        }
    }

    fn main_pane(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .id("content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(views::render(&self.state, &self.shell_inputs(), cx)),
            )
            .child(self.status_bar(p))
    }

    fn status_bar(&self, p: &Palette) -> impl IntoElement {
        let queue_len = queue::active_count(&self.state);
        let version = if self.state.winget_version.is_empty() {
            "winget".to_owned()
        } else {
            format!("winget {}", self.state.winget_version)
        };
        let dot = if self.state.winget_error.is_some() {
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
            .child(format!("源 {}", self.state.sources.len()))
            .child(format!("队列 {queue_len}"))
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
                    .child(div().text_size(px(13.)).font_semibold().child("安装队列"))
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
                                widgets::btn_ghost(cx, p, "queue-clear", "清除").on_click(
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
                                .child("队列为空"),
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
                    .child("安装任务串行执行，避免 winget 锁冲突"),
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
        QueueState::Pending => (p.muted, "等待中"),
        QueueState::Running => (p.accent, "进行中"),
        QueueState::Done => (p.ok, "已完成"),
        QueueState::Error => (p.danger, "失败"),
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

/// The 28 px gradient tile with the download glyph.
fn brand_mark(p: &Palette) -> impl IntoElement {
    div()
        .size(px(28.))
        .flex_none()
        .rounded(px(9.))
        .flex()
        .items_center()
        .justify_center()
        .bg(p.accent)
        .child(icon(IconKey::Download, px(15.), p.on_accent))
}

/// The wordmark row: gradient tile, name, prototype pill.
fn brand_row(p: &Palette) -> impl IntoElement {
    h_flex()
        .gap(px(8.))
        .flex_none()
        .child(brand_mark(p))
        .child(div().text_size(px(13.)).font_semibold().child("WingetGUI"))
        .child(widgets::pill(p, widgets::PillKind::Line, "原型 v0.1"))
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
            let next = this.state.search_source.next();
            this.set_search_source(next, window, cx);
        }))
        .child(source.label())
}

/// Installed package ids from `winget export`, lowercased.
fn export_installed(winget: &Winget) -> Vec<String> {
    let path = std::env::temp_dir().join("wingetgui-export.json");
    let args = winget::export_json(&path, None);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let ids = winget
        .run(&refs)
        .ok()
        .and_then(|_| std::fs::read_to_string(&path).ok())
        .and_then(|text| winget::parse_export_json(&text).ok())
        .unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    ids.into_iter().map(|id| id.to_lowercase()).collect()
}

/// Merge the catalogue search rows with the installed set and upgrade list.
///
/// `winget search` with an empty query matches nothing, so the installed
/// packages `winget list` reports must be merged in on their own — otherwise
/// 已安装 and 更新 are empty on a cold start.
fn merge_catalog(
    search: &[winget::SearchRow],
    list: &[winget::ListRow],
    installed: &[String],
) -> Vec<Package> {
    let mut packages: Vec<Package> = Vec::new();
    for row in search {
        let key = row.id.to_lowercase();
        if packages.iter().any(|p| p.key() == key) {
            continue;
        }
        let mut pkg = Package::from_search(row, "winget");
        pkg.installed = installed.contains(&key);
        if let Some(found) = list.iter().find(|l| l.id.to_lowercase() == key) {
            pkg.available = found.available.clone();
            if !found.version.is_empty() {
                pkg.version = found.version.clone();
            }
        }
        packages.push(pkg);
    }

    // Only winget-managed rows: the export set excludes ARP/MSIX-only entries,
    // which have no source and no version to offer an upgrade against.
    for row in list {
        let key = row.id.to_lowercase();
        if !installed.contains(&key) || packages.iter().any(|p| p.key() == key) {
            continue;
        }
        let mut pkg = Package::from_search(
            &winget::SearchRow {
                name: row.name.clone(),
                id: row.id.clone(),
                version: row.version.clone(),
                match_kind: None,
                source: row.source.clone(),
            },
            "winget",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winget::{ListRow, SearchRow};

    fn search_row(id: &str, name: &str) -> SearchRow {
        SearchRow {
            name: name.into(),
            id: id.into(),
            version: "1.0".into(),
            match_kind: None,
            source: "winget".into(),
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

        let packages = merge_catalog(&[], &list, &installed);

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

        let packages = merge_catalog(&[], &list, &[]);

        assert!(packages.is_empty(), "got {packages:?}");
    }

    #[test]
    fn a_search_row_is_not_duplicated_by_the_installed_pass() {
        let installed = vec!["git.git".to_owned()];
        let search = vec![search_row("Git.Git", "Git")];
        let list = vec![list_row("Git.Git", "Git", None)];

        let packages = merge_catalog(&search, &list, &installed);

        assert_eq!(packages.len(), 1);
        assert!(packages[0].installed);
    }
}

/// SharedString is used by the view signatures; keep the import honest.
const _: Option<SharedString> = None;
