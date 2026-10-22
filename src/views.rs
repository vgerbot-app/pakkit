//! The seven routes. Each function renders from an [`AppState`] snapshot and
//! routes every action back through [`AppView`]'s listeners, so the views stay
//! free of task bookkeeping.

use crate::backends::BackendKind;
use crate::dialogs;
use crate::icons::{IconKey, icon};
use crate::model::{CategoryId, Package};
use crate::queue::QueueAction;
use crate::shell::AppView;
use crate::state::{AppState, Language, Route, SearchSource};
use crate::theme::Palette;
use crate::widgets::{self, BtnKind, BtnSize, PillKind};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Disableable as _, StyledExt as _};
use gpui_kit::prelude::{
    FluentBuilder as _, InteractiveElement as _, StatefulInteractiveElement as _,
};
use gpui_kit::{
    AnyElement, App, Context, Entity, Hsla, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, px, relative,
};

/// The shell-owned input entities the settings view embeds. They cannot be
/// read through `cx.entity()` during render — the view is already borrowed —
/// so the shell passes them down.
#[derive(Clone)]
pub struct ShellInputs {
    pub install_location: Entity<InputState>,
    pub proxy: Entity<InputState>,
    pub backend_path: Entity<InputState>,
}

/// Dispatch to the active route's renderer.
///
/// `search_command` is the shell-computed equivalent command line; it cannot
/// be derived from `state` alone because it needs the backend's arg builders.
pub fn render(
    state: &AppState,
    inputs: &ShellInputs,
    search_command: &str,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let p = crate::theme::palette(cx);
    match state.route {
        Route::Home => home(state, &p, cx),
        Route::Categories => categories(state, &p, cx),
        Route::Category(id) => category_detail(state, &p, id, cx),
        Route::Search => search(state, search_command, &p, cx),
        Route::Installed => installed(state, &p, cx),
        Route::Updates => updates(state, &p, cx),
        Route::Sources => sources(state, &p, cx),
        Route::Settings => settings(state, inputs, &p, cx),
    }
}

// ------------------------------------------------------------------- 发现

fn home(state: &AppState, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let mut children: Vec<AnyElement> = vec![
        widgets::page_head(
            p,
            tr!("nav.home"),
            Some(tr!("home.subtitle").into()),
            vec![
                widgets::btn(
                    cx,
                    p,
                    "home-check",
                    BtnKind::Soft,
                    BtnSize::Regular,
                    tr!("home.check_updates"),
                )
                .on_click(cx.listener(|this, _, _, cx| this.refresh_updates(cx)))
                .into_any_element(),
                widgets::btn(
                    cx,
                    p,
                    "home-sync",
                    BtnKind::Soft,
                    BtnSize::Regular,
                    tr!("home.sync_sources"),
                )
                .on_click(cx.listener(|this, _, window, cx| this.sync_sources(None, window, cx)))
                .into_any_element(),
            ],
        )
        .into_any_element(),
    ];

    children.push(
        div()
            .px(px(24.))
            .child(hero(state, p, cx))
            .into_any_element(),
    );
    children.push(
        div()
        .h(px(10.))
        .into_any_element(),
    );

    // 常用分类
    children.push(
        widgets::page_body()
            .child(section_head(
                p,
                &tr!("home.categories_section"),
                Some((tr!("home.view_all"), Route::Categories)),
                cx,
            ))
            .child(
                div().flex().flex_wrap().gap(px(12.)).children(
                    CategoryId::ALL
                        .iter()
                        .map(|id| category_tile(state, p, *id, cx)),
                ),
            )
            .into_any_element(),
    );

    // 高分精选
    let featured = state.featured();
    if !featured.is_empty() {
        children.push(
            widgets::page_body()
                .child(widgets::section_label(p, tr!("home.featured")))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(12.))
                        .children(featured.iter().map(|pkg| package_card(state, p, pkg, cx))),
                )
                .into_any_element(),
        );
    }

    // 最近更新
    let recent = state.recently_updated();
    if !recent.is_empty() {
        children.push(
            widgets::page_body()
                .child(widgets::section_label(p, tr!("home.recent")))
                .child(
                    widgets::card(p).children(
                        recent
                            .iter()
                            .enumerate()
                            .map(|(ix, pkg)| recent_row(state, p, pkg, ix > 0, cx)),
                    ),
                )
                .into_any_element(),
        );
    }

    if state.backend_error.is_some() {
        children.insert(1, backend_missing(state, p, cx));
    }

    div()
        .flex()
        .flex_col()
        .children(children)
        .into_any_element()
}

/// The hero panel: glow, chip, headline, copy, buttons, three stat tiles.
fn hero(state: &AppState, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    widgets::card(p)
        .relative()
        .overflow_hidden()
        .rounded(px(18.))
        .p(px(24.))
        .child(
            div()
                .absolute()
                .top(px(-80.))
                .left(px(-40.))
                .size(px(260.))
                .rounded(px(9999.))
                .bg(Hsla {
                    a: 0.16,
                    ..p.accent
                }),
        )
        .child(
            div()
                .absolute()
                .bottom(px(-90.))
                .right(px(-40.))
                .size(px(240.))
                .rounded(px(9999.))
                .bg(Hsla {
                    a: 0.16,
                    ..p.accent2
                }),
        )
        .child(
            div()
                .relative()
                .flex()
                .flex_wrap()
                .gap(px(24.))
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(320.))
                        .child(widgets::pill(p, PillKind::Accent, tr!("home.hero_pill")))
                        .child(
                            div()
                                .text_size(px(24.))
                                .font_semibold()
                                .mt(px(12.))
                                .child(tr!("home.hero_title")),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .line_height(relative(1.6))
                                .text_color(p.ink2)
                                .mt(px(8.))
                                .child(tr!("home.hero_body")),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(8.))
                                .mt(px(16.))
                                .child(
                                    widgets::btn(
                                        cx,
                                        p,
                                        "hero-installed",
                                        BtnKind::Primary,
                                        BtnSize::Regular,
                                        tr!("home.manage_installed"),
                                    )
                                    .on_click(cx.listener(
                                        |this, _, window, cx| {
                                            this.navigate(Route::Installed, window, cx);
                                        },
                                    )),
                                )
                                .child(
                                    widgets::btn(
                                        cx,
                                        p,
                                        "hero-sources",
                                        BtnKind::Outline,
                                        BtnSize::Regular,
                                        tr!("home.manage_sources"),
                                    )
                                    .on_click(cx.listener(
                                        |this, _, window, cx| {
                                            this.navigate(Route::Sources, window, cx);
                                        },
                                    )),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(10.))
                        .child(widgets::stat_tile(
                            p,
                            IconKey::Source,
                            state.sources.len().to_string(),
                            tr!("home.stat_sources"),
                        ))
                        .child(widgets::stat_tile(
                            p,
                            IconKey::Package,
                            state.catalog_label(),
                            tr!("home.stat_packages"),
                        ))
                        .child(widgets::stat_tile(
                            p,
                            IconKey::Check,
                            state.installed_count().to_string(),
                            tr!("home.stat_installed"),
                        )),
                ),
        )
        .into_any_element()
}

/// `.cat-tile` — icon chip, name, description, then a bordered tag footer.
fn category_tile(
    state: &AppState,
    p: &Palette,
    id: CategoryId,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let count = state.category_count(id);
    let tag = crate::model::CATEGORY_TAGS
        .iter()
        .find(|(c, _)| *c == id)
        .and_then(|(_, tags)| tags.first().copied())
        .unwrap_or("software");

    widgets::card_hover(
        p,
        widgets::card(p)
            .w(px(230.))
            .flex_none()
            .flex()
            .flex_col()
            .p(px(14.))
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(widgets::category_chip(p, id))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_size(px(13.5)).font_semibold().child(id.label()))
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(p.muted)
                                    .truncate()
                                    .child(id.description()),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .pt(px(10.))
                    .border_t_1()
                    .border_color(p.line)
                    .child(widgets::pill(p, PillKind::Line, tag))
                    .child(
                        div()
                            .ml_auto()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child(tr!("category.count", count = count)),
                    ),
            ),
    )
    .id(SharedString::from(format!("cat-{}", id.key())))
    .cursor_pointer()
    .on_click(cx.listener(move |this, _, window, cx| {
        this.navigate(Route::Category(id), window, cx);
    }))
    .into_any_element()
}

/// `pkgRow` — the approved two-row package card.
fn package_card(
    state: &AppState,
    p: &Palette,
    pkg: &Package,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let busy = crate::queue::is_busy(state, &pkg.id);
    let action = widgets::package_action(pkg);
    let (label, _) = widgets::install_label(pkg, busy, action);
    let id = pkg.id.clone();

    widgets::card_hover(
        p,
        widgets::card(p)
            .w(px(280.))
            .flex_none()
            .flex()
            .flex_col()
            .p(px(14.))
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(10.))
                    .child(widgets::pkg_icon(
                        p,
                        pkg,
                        state.icon_path_for(&pkg.id),
                        44.,
                        22.,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_semibold()
                                    .truncate()
                                    .child(pkg.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(p.muted)
                                    .truncate()
                                    .child(pkg.publisher.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_end()
                            .gap(px(4.))
                            .when(pkg.installed, |this| {
                                this.child(widgets::pill(p, PillKind::Ok, tr!("pkg.installed")))
                            })
                            .when(pkg.available.is_some(), |this| {
                                this.child(widgets::pill(p, PillKind::Accent, tr!("pkg.updatable")))
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .child(widgets::meta_item(
                        p,
                        IconKey::Terminal,
                        pkg.version.clone(),
                    ))
                    .when_some(pkg.size.clone(), |this, size| {
                        this.child(widgets::meta_item(p, IconKey::File, size))
                    })
                    .when_some(pkg.rating, |this, rating| {
                        this.child(widgets::stars(p, rating))
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pt(px(10.))
                    .border_t_1()
                    .border_color(p.line)
                    .child(
                        widgets::btn(
                            cx,
                            p,
                            SharedString::from(format!("inst-{id}")),
                            BtnKind::Primary,
                            BtnSize::Small,
                            label,
                        )
                        .disabled(busy)
                        .on_click(cx.listener({
                            let id = id.clone();
                            move |this, _, window, cx| {
                                this.enqueue(&id, action, window, cx);
                            }
                        })),
                    )
                    .child(widgets::pill(p, PillKind::Line, pkg.category.label()))
                    .child(
                        div()
                            .id(SharedString::from(format!("open-{id}")))
                            .ml_auto()
                            .text_size(px(11.5))
                            .text_color(p.accent)
                            .cursor_pointer()
                            .hover(move |this| this.underline())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                dialogs::pkg_detail(this, &id, window, cx);
                            }))
                            .child(tr!("pkg.details")),
                    ),
            ),
    )
    .into_any_element()
}

/// One row of the 最近更新 card, divided by a hairline.
fn recent_row(
    state: &AppState,
    p: &Palette,
    pkg: &Package,
    divided: bool,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let id = pkg.id.clone();
    let installed = pkg.installed;
    div()
        .id(SharedString::from(format!("recent-{id}")))
        .flex()
        .items_center()
        .gap(px(10.))
        .px(px(14.))
        .py(px(10.))
        .when(divided, |this| this.border_t_1().border_color(p.line))
        .cursor_pointer()
        .child(widgets::pkg_icon(
            p,
            pkg,
            state.icon_path_for(&pkg.id),
            32.,
            16.,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_medium()
                        .truncate()
                        .child(pkg.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(p.muted)
                        .truncate()
                        .child(pkg.publisher.clone()),
                ),
        )
        .when(installed, |this| {
            this.child(widgets::pill(p, PillKind::Ok, tr!("pkg.installed")))
        })
        .child(widgets::pill(p, PillKind::Line, pkg.version.clone()))
        .child(
            div()
                .w(px(80.))
                .text_right()
                .text_size(px(11.))
                .text_color(p.muted)
                .child(pkg.updated.clone().unwrap_or_else(|| "—".to_owned())),
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            dialogs::pkg_detail(this, &id, window, cx);
        }))
        .into_any_element()
}

// ------------------------------------------------------------------- 分类

fn categories(_state: &AppState, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let head = widgets::page_head(
        p,
        tr!("nav.categories"),
        Some(tr!("categories.subtitle", count = CategoryId::ALL.len()).into()),
        Vec::new(),
    );

    div()
        .flex()
        .flex_col()
        .child(head)
        .child(
            widgets::page_body().child(
                div().flex().flex_wrap().gap(px(12.)).children(
                    CategoryId::ALL
                        .iter()
                        .map(|id| category_tile(_state, p, *id, cx)),
                ),
            ),
        )
        .into_any_element()
}

fn category_detail(
    state: &AppState,
    p: &Palette,
    id: CategoryId,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let color = Palette::from_hex(id.color());
    let packages = state.category_packages(id);
    let items = widgets::category_sort_items();
    let others: Vec<CategoryId> = CategoryId::ALL
        .iter()
        .copied()
        .filter(|c| *c != id)
        .take(5)
        .collect();
    let install_all: Vec<String> = packages
        .iter()
        .filter(|pkg| !pkg.installed)
        .map(|pkg| pkg.id.clone())
        .collect();

    let mut children: Vec<AnyElement> = vec![
        div()
            .px(px(24.))
            .pt(px(20.))
            .child(
                div()
                    .id("cat-back")
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_size(px(12.5))
                    .text_color(p.muted)
                    .cursor_pointer()
                    .hover(move |this| this.text_color(p.ink))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.navigate(Route::Categories, window, cx);
                    }))
                    .child(icon(IconKey::Back, px(13.), p.muted))
                    .child(tr!("category.back")),
            )
            .into_any_element(),
    ];

    // Hero banner tinted by the category colour.
    children.push(
        div()
            .px(px(24.))
            .pt(px(12.))
            .child(
                widgets::card(p)
                    .relative()
                    .overflow_hidden()
                    .rounded(px(18.))
                    .p(px(20.))
                    .child(
                        div()
                            .absolute()
                            .top(px(-70.))
                            .right(px(-30.))
                            .size(px(240.))
                            .rounded(px(9999.))
                            .bg(Hsla { a: 0.18, ..color }),
                    )
                    .child(
                        div()
                            .relative()
                            .flex()
                            .items_center()
                            .gap(px(14.))
                            .child(widgets::tinted_chip(p, id.icon(), id.color(), 52., 26.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div().text_size(px(20.)).font_semibold().child(id.label()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .text_color(p.ink2)
                                            .child(id.description()),
                                    )
                                    .child(div().mt(px(6.)).child(widgets::pill(
                                        p,
                                        PillKind::Line,
                                        tr!("category.packages_count", count = packages.len()),
                                    ))),
                            )
                            .child(
                                widgets::btn(
                                    cx,
                                    p,
                                    "cat-install-all",
                                    BtnKind::Primary,
                                    BtnSize::Regular,
                                    tr!("category.install_all"),
                                )
                                // The disabled style fades the label to 50% of
                                // the muted colour, so it is swapped for a
                                // readable one plus a tooltip that says why.
                                .when(install_all.is_empty(), |button| {
                                    button
                                        .disabled(true)
                                        .text_color(p.muted)
                                        .bg(p.tint(p.accent, 0.15))
                                        .border_color(p.tint(p.accent, 0.25))
                                        .tooltip(tr!("category.all_installed"))
                                })
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        for id in &install_all {
                                            this.enqueue(id, QueueAction::Install, window, cx);
                                        }
                                    },
                                )),
                            ),
                    ),
            )
            .into_any_element(),
    );
    children.push(
        div()
        .h(px(10.))
        .into_any_element(),
    );

    children.push(
        widgets::page_body()
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(widgets::section_label(p, tr!("category.sort_label")))
                            .child(widgets::seg_control(
                                p,
                                "cat-sort",
                                &items,
                                state.category_sort,
                                seg_handler(cx, |this, value, _, cx| {
                                    this.state.category_sort = value;
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(div().ml_auto().flex().items_center().gap(px(6.)).children(
                        others.iter().map(|other| {
                            let other = *other;
                            widgets::pill_button(
                                p,
                                SharedString::from(format!("sib-{}", other.key())),
                                other.label(),
                                cx.listener(move |this, _, window, cx| {
                                    this.navigate(Route::Category(other), window, cx);
                                }),
                            )
                        }),
                    )),
            )
            .child(
                div()
                    .pt(px(14.))
                    .flex()
                    .flex_wrap()
                    .gap(px(12.))
                    .children(packages.iter().map(|pkg| package_card(state, p, pkg, cx))),
            )
            .into_any_element(),
    );

    div()
        .flex()
        .flex_col()
        .children(children)
        .into_any_element()
}

// ------------------------------------------------------------------- 搜索

fn search(
    state: &AppState,
    command: &str,
    p: &Palette,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let results = state.filtered();
    let title = if state.query.trim().is_empty() {
        tr!("status.all_packages")
    } else {
        tr!("search.title_query", query = state.query.trim())
    };
    let subtitle = tr!(
        "search.results_count",
        count = results.len(),
        source = state.search_source.label()
    );

    let head_actions = if state.capabilities.search_filters {
        vec![
            widgets::btn(
                cx,
                p,
                "adv-filter",
                BtnKind::Soft,
                BtnSize::Regular,
                tr!("dialog.advanced_filter"),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                dialogs::advanced_search(this, window, cx);
            }))
            .into_any_element(),
        ]
    } else {
        Vec::new()
    };
    let head = widgets::page_head(p, title, Some(subtitle.into()), head_actions);

    let sources = widgets::source_items(&state.sources);
    let statuses = widgets::status_items();
    let sorts = widgets::sort_items();
    let command_for_copy = command.to_owned();

    let filter_card = widgets::card(p)
        .mx(px(24.))
        .p(px(10.))
        .flex()
        .items_center()
        .flex_wrap()
        .gap(px(10.))
        .child(widgets::seg_control(
            p,
            "src-seg",
            &sources,
            state.search_source.clone(),
            seg_handler(cx, |this, value, window, cx| {
                this.set_search_source(value, window, cx);
            }),
        ))
        .child(div().w(px(1.)).h(px(20.)).bg(p.line))
        .child(widgets::seg_control(
            p,
            "status-seg",
            &statuses,
            state.search_filter,
            seg_handler(cx, |this, value, _, cx| {
                this.state.search_filter = value;
                cx.notify();
            }),
        ))
        .child(div().ml_auto().child(widgets::seg_control(
            p,
            "sort-seg",
            &sorts,
            state.sort,
            seg_handler(cx, |this, value, _, cx| {
                this.state.sort = value;
                cx.notify();
            }),
        )));

    let command_line = div()
        .mx(px(24.))
        .mt(px(10.))
        .flex()
        .items_center()
        .gap(px(6.))
        .child(icon(IconKey::Terminal, px(12.), p.muted))
        .child(
            widgets::mono(p)
                .child(command.to_owned())
                .into_any_element(),
        )
        .child(
            widgets::icon_btn("copy-command", IconKey::Copy, 24., p.muted).on_click(cx.listener(
                move |this, _, window, cx| {
                    this.copy(&command_for_copy, window, cx);
                },
            )),
        );

    let body = if results.is_empty() {
        widgets::page_body().child(widgets::empty_state(
            p,
            IconKey::Search,
            if state.query.trim().is_empty() {
                tr!("search.empty_none")
            } else {
                tr!("search.empty_no_match")
            },
            tr!("search.empty_hint"),
            vec![
                widgets::btn(
                    cx,
                    p,
                    "empty-all-src",
                    BtnKind::Primary,
                    BtnSize::Regular,
                    tr!("search.search_all_sources"),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.set_search_source(SearchSource::All, window, cx);
                }))
                .into_any_element(),
                widgets::btn(
                    cx,
                    p,
                    "empty-sources",
                    BtnKind::Outline,
                    BtnSize::Regular,
                    tr!("home.manage_sources"),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.navigate(Route::Sources, window, cx);
                }))
                .into_any_element(),
            ],
        ))
    } else {
        widgets::page_body().child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(12.))
                .children(results.iter().map(|pkg| package_card(state, p, pkg, cx))),
        )
    };

    div()
        .flex()
        .flex_col()
        .child(head)
        .child(filter_card)
        .child(command_line)
        .child(body)
        .into_any_element()
}

// ------------------------------------------------------------------- 已安装

fn installed(state: &AppState, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let rows = state.installed();
    let all_ids: Vec<String> = rows.iter().map(|pkg| pkg.id.clone()).collect();

    let mut head_actions = Vec::new();
    if state.capabilities.export {
        head_actions.push(
            widgets::btn(
                cx,
                p,
                "export-manifest",
                BtnKind::Soft,
                BtnSize::Regular,
                tr!("installed.export"),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                dialogs::export_manifest(this, window, cx);
            }))
            .into_any_element(),
        );
    }
    head_actions.push(
        widgets::btn(
            cx,
            p,
            "upgrade-all",
            BtnKind::Primary,
            BtnSize::Regular,
            tr!("installed.upgrade_all"),
        )
        .disabled(all_ids.is_empty())
        .on_click(cx.listener(move |this, _, window, cx| {
            for id in &all_ids {
                this.enqueue(id, QueueAction::Upgrade, window, cx);
            }
        }))
        .into_any_element(),
    );
    let head = widgets::page_head(
        p,
        tr!("nav.installed"),
        Some(tr!("installed.subtitle", count = rows.len()).into()),
        head_actions,
    );

    let header = div()
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(14.))
        .py(px(9.))
        .text_size(px(11.))
        .text_color(p.muted)
        .bg(Hsla {
            a: 0.6,
            ..p.surface2
        })
        .border_b_1()
        .border_color(p.line)
        .child(div().flex_1().child(tr!("installed.col_package")))
        .child(div().w(px(110.)).child(tr!("installed.col_version")))
        .child(
            div()
                .w(px(70.))
                .text_right()
                .child(tr!("installed.col_size")),
        )
        .child(
            div()
                .w(px(120.))
                .text_right()
                .child(tr!("installed.col_actions")),
        );

    let body = if rows.is_empty() {
        widgets::page_body().child(widgets::empty_state(
            p,
            IconKey::Check,
            tr!("installed.empty"),
            tr!("installed.empty_hint"),
            Vec::new(),
        ))
    } else {
        div().px(px(24.)).pb(px(32.)).child(
            widgets::card(p)
                .child(header)
                .children(
                    rows.iter()
                        .enumerate()
                        .map(|(ix, pkg)| installed_row(state, p, pkg, ix > 0, cx)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(px(14.))
                        .py(px(10.))
                        .border_t_1()
                        .border_color(p.line)
                        .text_size(px(11.))
                        .text_color(p.muted)
                        .child(icon(IconKey::Shield, px(12.), p.muted))
                        .child(tr!("installed.elevation_note")),
                ),
        )
    };

    div()
        .flex()
        .flex_col()
        .child(head)
        .child(body)
        .into_any_element()
}

fn installed_row(
    state: &AppState,
    p: &Palette,
    pkg: &Package,
    divided: bool,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let id = pkg.id.clone();
    let busy = crate::queue::is_busy(state, &pkg.id);
    let ter_id = pkg.id.clone();
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(14.))
        .py(px(10.))
        .when(divided, |this| this.border_t_1().border_color(p.line))
        .child(widgets::pkg_icon(
            p,
            pkg,
            state.icon_path_for(&pkg.id),
            36.,
            18.,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_medium()
                        .truncate()
                        .child(pkg.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .font_family("Cascadia Code")
                        .text_color(p.muted)
                        .truncate()
                        .child(pkg.id.clone()),
                ),
        )
        .child(
            div()
                .w(px(110.))
                .text_size(px(11.5))
                .font_family("Cascadia Code")
                .text_color(p.ink2)
                .truncate()
                .child(pkg.version.clone()),
        )
        .child(
            div()
                .w(px(70.))
                .text_right()
                .text_size(px(11.5))
                .text_color(p.muted)
                .child(pkg.size.clone().unwrap_or_else(|| "—".to_owned())),
        )
        .child(
            div()
                .w(px(120.))
                .flex()
                .justify_end()
                .gap(px(6.))
                .child(
                    widgets::btn(
                        cx,
                        p,
                        SharedString::from(format!("up-{id}")),
                        BtnKind::Soft,
                        BtnSize::Small,
                        tr!("installed.upgrade"),
                    )
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.enqueue(&id, QueueAction::Upgrade, window, cx);
                    })),
                )
                .child(
                    widgets::icon_btn(
                        SharedString::from(format!("rm-{ter_id}")),
                        IconKey::Trash,
                        28.,
                        p.muted,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        dialogs::confirm_uninstall(this, &ter_id, window, cx);
                    })),
                ),
        )
        .into_any_element()
}

// ------------------------------------------------------------------- 更新

fn updates(state: &AppState, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let rows = state.updates();
    let all_ids: Vec<String> = rows.iter().map(|pkg| pkg.id.clone()).collect();

    let head = widgets::page_head(
        p,
        tr!("nav.updates"),
        Some(tr!("updates.subtitle", count = rows.len()).into()),
        vec![
            widgets::btn(
                cx,
                p,
                "update-all",
                BtnKind::Primary,
                BtnSize::Regular,
                tr!("updates.update_all"),
            )
            .disabled(all_ids.is_empty())
            .on_click(cx.listener(move |this, _, window, cx| {
                for id in &all_ids {
                    this.enqueue(id, QueueAction::Upgrade, window, cx);
                }
            }))
            .into_any_element(),
        ],
    );

    let body = if rows.is_empty() {
        widgets::page_body().child(widgets::empty_state(
            p,
            IconKey::Refresh,
            tr!("updates.empty"),
            tr!("updates.empty_hint"),
            Vec::new(),
        ))
    } else {
        widgets::page_body().child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .children(rows.iter().map(|pkg| update_card(state, p, pkg, cx))),
        )
    };

    div()
        .flex()
        .flex_col()
        .child(head)
        .child(body)
        .into_any_element()
}

fn update_card(
    state: &AppState,
    p: &Palette,
    pkg: &Package,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let id = pkg.id.clone();
    let busy = crate::queue::is_busy(state, &pkg.id);
    let available = pkg.available.clone().unwrap_or_default();
    let delta = pkg.upgrade_delta().unwrap_or_default().to_owned();

    widgets::card_hover(
        p,
        widgets::card(p)
            .p(px(14.))
            .flex()
            .items_center()
            .gap(px(14.))
            .child(widgets::pkg_icon(
                p,
                pkg,
                state.icon_path_for(&pkg.id),
                44.,
                22.,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(13.5))
                            .font_semibold()
                            .truncate()
                            .child(pkg.name.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .mt(px(4.))
                            .text_size(px(11.5))
                            .font_family("Cascadia Code")
                            .text_color(p.muted)
                            .child(div().text_color(p.muted).child(strike(&pkg.version)))
                            .child(icon(IconKey::Chevron, px(12.), p.muted))
                            .child(div().text_color(p.ok).child(available))
                            .when(!delta.is_empty(), |this| {
                                this.child(widgets::pill(p, PillKind::Accent, delta.clone()))
                            }),
                    ),
            )
            .child(
                widgets::btn(
                    cx,
                    p,
                    SharedString::from(format!("upd-{id}")),
                    BtnKind::Primary,
                    BtnSize::Small,
                    tr!("updates.update"),
                )
                .disabled(busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.enqueue(&id, QueueAction::Upgrade, window, cx);
                })),
            ),
    )
    .into_any_element()
}

/// The prototype strikes the old version through; gpui has no strikethrough
/// styling on `Div`, so the version is bracketed instead.
fn strike(text: &str) -> String {
    format!("‹{text}›")
}

// ------------------------------------------------------------------- 源管理

fn sources(state: &AppState, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let mut head_actions = vec![
        widgets::btn(
            cx,
            p,
            "sync-all-src",
            BtnKind::Soft,
            BtnSize::Regular,
            tr!("updates.update_all"),
        )
        .on_click(cx.listener(|this, _, window, cx| this.sync_sources(None, window, cx)))
        .into_any_element(),
    ];
    if state.capabilities.source_add {
        head_actions.push(
            widgets::btn(
                cx,
                p,
                "add-src",
                BtnKind::Primary,
                BtnSize::Regular,
                tr!("sources.add"),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                dialogs::add_source(this, window, cx);
            }))
            .into_any_element(),
        );
    }
    let head = widgets::page_head(
        p,
        tr!("nav.sources"),
        Some(tr!("sources.subtitle").into()),
        head_actions,
    );

    let body = if state.sources.is_empty() {
        widgets::page_body().child(widgets::empty_state(
            p,
            IconKey::Source,
            tr!("sources.empty"),
            tr!("sources.empty_hint"),
            Vec::new(),
        ))
    } else {
        widgets::page_body().child(
            div().flex().flex_col().gap(px(10.)).children(
                state
                    .sources
                    .iter()
                    .map(|source| source_card(state, p, source, cx)),
            ),
        )
    };

    // The equivalent command differs per backend: winget adds a source,
    // flatpak adds a remote. Shown only when the backend can add one at all.
    let add_hint = match state.backend_kind {
        BackendKind::Winget => {
            format!("{} source add --name <name> --arg <url>", state.backend_name)
        }
        BackendKind::Flatpak => {
            format!("{} remote-add --if-not-exists <name> <url>", state.backend_name)
        }
    };

    div()
        .flex()
        .flex_col()
        .child(head)
        .child(body)
        .when(state.capabilities.source_add, |this| {
            this.child(
                div().px(px(24.)).pb(px(32.)).child(
                    div()
                        .border_1()
                        .border_dashed()
                        .border_color(p.line_strong)
                        .rounded(px(12.))
                        .p(px(14.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(11.5))
                        .text_color(p.muted)
                        .child(icon(IconKey::Terminal, px(13.), p.muted))
                        .child(tr!("sources.equivalent_command"))
                        .child(widgets::code_chip(p, add_hint)),
                ),
            )
        })
        .into_any_element()
}

fn source_card(
    state: &AppState,
    p: &Palette,
    source: &crate::model::Source,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let name = source.name.clone();
    let is_default = state.default_source == source.name;
    let edit_name = source.name.clone();
    let remove_name = source.name.clone();
    let sync_name = source.name.clone();
    let set_name = source.name.clone();

    widgets::card(p)
        .p(px(14.))
        .flex()
        .items_center()
        .gap(px(14.))
        .child(widgets::tinted_chip(
            p,
            IconKey::Source,
            "#4f8cff",
            44.,
            22.,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(div().text_size(px(14.)).font_semibold().child(name.clone()))
                        .child(if source.trusted {
                            widgets::pill(p, PillKind::Ok, tr!("sources.trusted"))
                        } else {
                            widgets::pill(p, PillKind::Warn, tr!("sources.unverified"))
                        })
                        .when(is_default, |this| {
                            this.child(widgets::pill(p, PillKind::Accent, tr!("sources.default")))
                        }),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_family("Cascadia Code")
                        .text_color(p.muted)
                        .truncate()
                        .child(source.url.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .mt(px(6.))
                        .text_size(px(11.))
                        .child(widgets::meta_item(
                            p,
                            IconKey::Grid,
                            tr!(
                                "sources.packages_count",
                                count = source
                                    .packages
                                    .map_or_else(|| "—".to_owned(), |n| n.to_string())
                            ),
                        ))
                        .child(widgets::meta_item(
                            p,
                            IconKey::Clock,
                            source.updated.clone().unwrap_or_else(|| "—".to_owned()),
                        ))
                        .child(widgets::meta_item(
                            p,
                            IconKey::Shield,
                            if source.ty.is_empty() {
                                "—".to_owned()
                            } else {
                                source.ty.clone()
                            },
                        )),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    widgets::btn(
                        cx,
                        p,
                        SharedString::from(format!("sync-{sync_name}")),
                        BtnKind::Soft,
                        BtnSize::Small,
                        tr!("updates.update"),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.sync_sources(Some(sync_name.clone()), window, cx);
                    })),
                )
                .when(!is_default, |this| {
                    this.child(
                        widgets::btn(
                            cx,
                            p,
                            SharedString::from(format!("def-{set_name}")),
                            BtnKind::Outline,
                            BtnSize::Small,
                            tr!("sources.set_default"),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.default_source = set_name.clone();
                            this.state.settings.default_source = set_name.clone();
                            this.persist_settings();
                            cx.notify();
                        })),
                    )
                })
                .when(state.capabilities.source_edit, |this| {
                    this.child(
                        widgets::icon_btn(
                            SharedString::from(format!("edit-{edit_name}")),
                            IconKey::Edit,
                            28.,
                            p.muted,
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            dialogs::edit_source(this, &edit_name, window, cx);
                        })),
                    )
                })
                .child(
                    widgets::icon_btn(
                        SharedString::from(format!("del-{remove_name}")),
                        IconKey::Trash,
                        28.,
                        p.danger,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        dialogs::confirm_remove_source(this, &remove_name, window, cx);
                    })),
                ),
        )
        .into_any_element()
}

// ------------------------------------------------------------------- 设置

fn settings(
    state: &AppState,
    inputs: &ShellInputs,
    p: &Palette,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let head = widgets::page_head(
        p,
        tr!("nav.settings"),
        Some(tr!("settings.subtitle").into()),
        Vec::new(),
    );

    // The default-source row's hint differs per backend; every other row is
    // a winget-only install flag, gated on the matching capability.
    let source_hint = match state.backend_kind {
        BackendKind::Winget => format!("{} install --source <name>", state.backend_name),
        BackendKind::Flatpak => format!("{} install <remote> <id>", state.backend_name),
    };
    let mut install_rows = vec![row_shell_plain(
        p,
        &tr!("settings.default_source"),
        &source_hint,
        default_source_control(state, p, cx),
    )];
    if state.capabilities.install_arch {
        install_rows.push(seg_row(
            p,
            &tr!("settings.arch"),
            &format!("{} install --architecture <arch>", state.backend_name),
            arch_segments(state, p, cx),
        ));
    }
    if state.capabilities.install_scope {
        install_rows.push(seg_row(
            p,
            &tr!("settings.scope"),
            &format!("{} install --scope <scope>", state.backend_name),
            scope_segments(state, p, cx),
        ));
    }
    if state.capabilities.install_location {
        install_rows.push(field_row(
            p,
            &tr!("settings.location"),
            &format!("{} install --location <path>", state.backend_name),
            input_field(p, &inputs.install_location),
        ));
    }
    if state.capabilities.agreements {
        install_rows.push(switch_field_row(
            cx,
            p,
            &tr!("settings.accept_agreements"),
            &format!(
                "{} install --accept-package-agreements",
                state.backend_name
            ),
            state.settings.accept_agreements,
            |this, value, cx| {
                this.state.settings.accept_agreements = value;
                this.persist_settings();
                cx.notify();
            },
        ));
    }
    if state.capabilities.silent {
        install_rows.push(switch_field_row(
            cx,
            p,
            &tr!("settings.silent"),
            &format!("{} install --silent", state.backend_name),
            state.settings.silent,
            |this, value, cx| {
                this.state.settings.silent = value;
                this.persist_settings();
                cx.notify();
            },
        ));
    }
    let install_card = settings_card(
        p,
        &tr!("settings.install_defaults"),
        &tr!("settings.install_defaults_hint"),
        install_rows,
    );

    let queue_card = settings_card(
        p,
        &tr!("settings.updates_queue"),
        &tr!("settings.updates_queue_hint"),
        vec![
            switch_field_row(
                cx,
                p,
                &tr!("settings.auto_check"),
                &tr!("settings.auto_check_hint"),
                state.settings.auto_check_updates,
                |this, value, cx| {
                    this.state.settings.auto_check_updates = value;
                    this.persist_settings();
                    cx.notify();
                },
            ),
            seg_row(
                p,
                &tr!("settings.concurrency"),
                &tr!("settings.concurrency_hint"),
                concurrency_segments(state, p, cx),
            ),
        ],
    );

    let appearance_card = settings_card(
        p,
        &tr!("settings.appearance"),
        &tr!("settings.appearance_hint"),
        vec![
            seg_row(
                p,
                &tr!("settings.theme"),
                &tr!("settings.theme_hint"),
                theme_segments(state, p, cx),
            ),
            seg_row(
                p,
                &tr!("settings.language"),
                &tr!("settings.language_hint"),
                language_segments(state, p, cx),
            ),
            accent_row(p, state, cx),
            switch_field_row(
                cx,
                p,
                &tr!("settings.reduce_motion"),
                &tr!("settings.reduce_motion_hint"),
                state.settings.reduce_motion,
                |this, value, cx| {
                    this.state.settings.reduce_motion = value;
                    this.persist_settings();
                    cx.notify();
                },
            ),
        ],
    );

    let advanced_card = settings_card(
        p,
        &tr!("settings.advanced"),
        &tr!("settings.advanced_hint"),
        vec![
            field_row(
                p,
                &tr!("settings.backend_path", backend = state.backend_name),
                &tr!("settings.backend_path_hint"),
                input_field(p, &inputs.backend_path),
            ),
            field_row(
                p,
                &tr!("settings.proxy"),
                &tr!("settings.proxy_hint"),
                input_field(p, &inputs.proxy),
            ),
            switch_field_row(
                cx,
                p,
                &tr!("settings.telemetry"),
                &tr!("settings.telemetry_hint"),
                state.settings.telemetry,
                |this, value, cx| {
                    this.state.settings.telemetry = value;
                    this.persist_settings();
                    cx.notify();
                },
            ),
        ],
    );

    let about_card = about(p, state, cx);

    div()
        .flex()
        .flex_col()
        .child(head)
        .child(
            widgets::page_body().child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(380.))
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .child(install_card)
                            .child(queue_card),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(380.))
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .child(appearance_card)
                            .child(advanced_card)
                            .child(about_card),
                    ),
            ),
        )
        .into_any_element()
}

fn settings_card(p: &Palette, title: &str, description: &str, rows: Vec<AnyElement>) -> AnyElement {
    widgets::card(p)
        .flex_1()
        .child(
            div()
                .px(px(16.))
                .py(px(14.))
                .child(
                    div()
                        .text_size(px(14.))
                        .font_semibold()
                        .child(title.to_owned()),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.muted)
                        .mt(px(2.))
                        .child(description.to_owned()),
                ),
        )
        .children(rows)
        .into_any_element()
}

/// One settings row: label + hint on the left, control on the right.
fn row_shell(p: &Palette, label: &str, hint: &str, control: AnyElement) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(14.))
        .px(px(16.))
        .py(px(11.))
        .border_t_1()
        .border_color(p.line)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(13.))
                        .font_medium()
                        .child(label.to_owned()),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(p.muted)
                        .truncate()
                        .child(hint.to_owned()),
                ),
        )
        .child(control)
        .into_any_element()
}

/// A settings row whose control is a segment control.
fn seg_row(p: &Palette, label: &str, hint: &str, control: AnyElement) -> AnyElement {
    row_shell(p, label, hint, control)
}

/// A settings row whose control is a text field.
fn field_row(p: &Palette, label: &str, hint: &str, control: AnyElement) -> AnyElement {
    row_shell(p, label, hint, control)
}

/// A settings row whose control is a list of choices rather than a segment.
fn row_shell_plain(p: &Palette, label: &str, hint: &str, control: AnyElement) -> AnyElement {
    row_shell(p, label, hint, control)
}

fn switch_field_row(
    cx: &mut Context<AppView>,
    p: &Palette,
    label: &str,
    hint: &str,
    checked: bool,
    on_change: impl Fn(&mut AppView, bool, &mut Context<AppView>) + 'static,
) -> AnyElement {
    let view = cx.entity().downgrade();
    let listener = move |value: &bool, _: &mut Window, cx: &mut App| {
        let value = *value;
        let _ = view.update(cx, |this, cx| on_change(this, value, cx));
    };
    let control = widgets::switch_row(
        p,
        SharedString::from(format!("sw-{label}")),
        checked,
        listener,
    );
    row_shell(p, label, hint, control.into_any_element())
}

fn default_source_control(state: &AppState, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    // One choice per live source; nothing is invented for a backend whose
    // sources have not loaded yet.
    let names: Vec<String> = state.sources.iter().map(|s| s.name.clone()).collect();
    div()
        .flex()
        .gap(px(6.))
        .children(names.iter().map(|name| {
            let key = name.clone();
            let label = name.clone();
            let on = state.default_source == *name;
            div()
                .id(SharedString::from(format!("dsrc-{name}")))
                .text_size(px(11.5))
                .px(px(10.))
                .py(px(5.))
                .rounded(px(7.))
                .border_1()
                .border_color(if on { p.accent } else { p.line })
                .bg(if on {
                    p.tint(p.accent, 0.14)
                } else {
                    p.surface2
                })
                .text_color(if on { p.accent } else { p.ink2 })
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.state.default_source = key.clone();
                    this.state.settings.default_source = key.clone();
                    this.persist_settings();
                    cx.notify();
                }))
                .child(label)
        }))
        .into_any_element()
}

fn arch_segments(state: &AppState, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let items = widgets::arch_items();
    widgets::seg_control(
        p,
        "arch-seg",
        &items,
        state.settings.arch,
        seg_handler(cx, |this, value, _, cx| {
            this.state.settings.arch = value;
            this.persist_settings();
            cx.notify();
        }),
    )
    .into_any_element()
}

fn scope_segments(state: &AppState, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let items = widgets::scope_items();
    widgets::seg_control(
        p,
        "scope-seg",
        &items,
        state.settings.scope,
        seg_handler(cx, |this, value, _, cx| {
            this.state.settings.scope = value;
            this.persist_settings();
            cx.notify();
        }),
    )
    .into_any_element()
}

fn concurrency_segments(state: &AppState, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let items = widgets::concurrency_items();
    widgets::seg_control(
        p,
        "conc-seg",
        &items,
        state.settings.concurrency,
        seg_handler(cx, |this, value, _, cx| {
            this.state.settings.concurrency = value;
            this.persist_settings();
            cx.notify();
        }),
    )
    .into_any_element()
}

fn theme_segments(state: &AppState, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let items = widgets::theme_items();
    widgets::seg_control(
        p,
        "theme-seg",
        &items,
        state.settings.theme,
        seg_handler(cx, |this, value, _, cx| {
            this.state.settings.theme = value;
            crate::theme::set_mode(cx, value);
            this.persist_settings();
            cx.notify();
        }),
    )
    .into_any_element()
}

/// The language picker. Switching is immediate: `set_locale` retargets every
/// `t!` call, and the `cx.notify()` re-render rebuilds the whole tree — the
/// `AppView` is the single root view, so nothing keeps a stale translation.
fn language_segments(state: &AppState, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    let items = widgets::language_items();
    widgets::seg_control(
        p,
        "lang-seg",
        &items,
        Language::from_tag(&state.settings.language),
        seg_handler(cx, |this, value: Language, window, cx| {
            this.state.settings.language = value.tag().to_owned();
            rust_i18n::set_locale(value.tag());
            this.persist_settings();
            // The search box's placeholder was baked into its InputState at
            // construction; re-seed it so the chrome follows the language too.
            this.search_input.update(cx, |input, cx| {
                input.set_placeholder(tr!("search.placeholder"), window, cx);
            });
            cx.notify();
        }),
    )
    .into_any_element()
}

fn accent_row(p: &Palette, state: &AppState, cx: &mut Context<AppView>) -> AnyElement {
    let selected = state.settings.accent.clone();
    let swatches = crate::theme::ACCENT_SWATCHES;
    row_shell(
        p,
        &tr!("settings.accent"),
        &tr!("settings.accent_hint"),
        div()
            .flex()
            .gap(px(8.))
            .children(swatches.iter().map(|hex| {
                let hex = (*hex).to_owned();
                let color = Palette::from_hex(&hex);
                let on = selected.eq_ignore_ascii_case(&hex);
                let key = hex.clone();
                div()
                    .id(SharedString::from(format!("swatch-{hex}")))
                    .size(px(20.))
                    .rounded(px(9999.))
                    .bg(color)
                    .border_2()
                    .border_color(if on { p.ink } else { p.line })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_accent(&key, cx);
                    }))
            }))
            .into_any_element(),
    )
}

/// `seg_control` reports the picked value by value, while `cx.listener`
/// produces a reference-taking closure. This adapts one to the other.
fn seg_handler<T: Clone + 'static>(
    cx: &Context<AppView>,
    on_select: impl Fn(&mut AppView, T, &mut Window, &mut Context<AppView>) + 'static,
) -> impl Fn(T, &mut Window, &mut App) + 'static {
    let view = cx.entity().downgrade();
    move |value: T, window, cx| {
        let _ = view.update(cx, |this, cx| on_select(this, value, window, cx));
    }
}

/// The three settings text fields live on `AppView`; the shell passes the
/// entities down so the renderer never re-borrows the view.
fn input_field(p: &Palette, input: &Entity<InputState>) -> AnyElement {
    widgets::field(p, input).w(px(200.)).into_any_element()
}

fn about(p: &Palette, state: &AppState, cx: &mut Context<AppView>) -> AnyElement {
    let version = if state.backend_version.is_empty() {
        tr!("settings.not_detected")
    } else {
        state.backend_version.clone()
    };
    settings_card(
        p,
        &tr!("settings.about"),
        "WingetGUI v0.1.0-prototype · Rust + GPUI · MIT License",
        vec![
            div()
                .flex()
                .items_center()
                .gap(px(14.))
                .px(px(16.))
                .py(px(14.))
                .border_t_1()
                .border_color(p.line)
                .child(
                    div()
                        .size(px(48.))
                        .rounded(px(14.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(p.accent)
                        .child(icon(IconKey::Download, px(24.), p.on_accent)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_size(px(13.5)).font_semibold().child("WingetGUI"))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(p.muted)
                                .child("Rust + GPUI · MIT License"),
                        ),
                )
                .child(
                    div().flex().gap(px(6.)).child(
                        widgets::btn(
                            cx,
                            p,
                            "about-redetect",
                            BtnKind::Outline,
                            BtnSize::Small,
                            tr!("settings.detect"),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.redetect_backend(cx))),
                    ),
                )
                .into_any_element(),
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(16.))
                .py(px(11.))
                .border_t_1()
                .border_color(p.line)
                .text_size(px(12.))
                .text_color(p.muted)
                .child(tr!("settings.backend_version", backend = state.backend_name))
                .child(widgets::code_chip(p, version))
                .into_any_element(),
        ],
    )
}

// --------------------------------------------------------------- helpers

fn section_head(
    p: &Palette,
    title: &str,
    link: Option<(String, Route)>,
    cx: &mut Context<AppView>,
) -> AnyElement {
    let mut head = div()
        .flex()
        .items_center()
        .gap(px(8.))
        .pb(px(10.))
        .child(widgets::section_label(p, title));
    if let Some((label, route)) = link {
        head = head.child(
            div()
                .id(SharedString::from(format!("link-{label}")))
                .text_size(px(12.))
                .text_color(p.accent)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.navigate(route, window, cx);
                }))
                .child(label.clone()),
        );
    }
    head.into_any_element()
}

fn backend_missing(state: &AppState, p: &Palette, cx: &mut Context<AppView>) -> AnyElement {
    div()
        .px(px(24.))
        .pb(px(16.))
        .child(
            widgets::card(p)
                .p(px(14.))
                .flex()
                .items_center()
                .gap(px(10.))
                .border_color(p.tint(p.warn, 0.4))
                .child(icon(IconKey::Warning, px(16.), p.warn))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(12.5))
                        .text_color(p.ink2)
                        .child(tr!("home.backend_missing", backend = state.backend_name)),
                )
                .child(
                    widgets::btn(
                        cx,
                        p,
                        "go-settings",
                        BtnKind::Soft,
                        BtnSize::Small,
                        tr!("home.go_settings"),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.navigate(Route::Settings, window, cx);
                    })),
                ),
        )
        .into_any_element()
}
