//! Modal dialogs, opened through `WindowExt`.
//!
//! A dialog's builder is a *render* closure that runs again on layer rebuilds,
//! so it must be an `Fn`: every persistent entity (the text inputs) and every
//! listener is created before the dialog opens. Inside the builder the view is
//! reached through a captured [`WeakEntity`], because the builder only gets
//! `&mut App`, never the view's own context.
//!
//! Click handlers must use `WeakEntity::update` with the handler's own
//! `&mut Window`, never `update_in`: event dispatch runs while the window is
//! taken out of the app's window map, so `update_in`'s window re-lookup always
//! fails mid-click and the handler silently does nothing.

use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{Disableable as _, StyledExt as _, WindowExt as _};
use gpui_kit::prelude::{
    FluentBuilder as _, InteractiveElement as _, StatefulInteractiveElement as _,
};
use gpui_kit::{
    AnyElement, AppContext as _, ClickEvent, Context, IntoElement, ParentElement as _,
    SharedString, Styled as _, Window, div, px, relative,
};
use std::cell::Cell;
use std::rc::Rc;

use crate::backends::types::PackageDetail;
use crate::icons::{IconKey, icon};
use crate::model::Package;
use crate::shell::AppView;
use crate::theme::Palette;
use crate::widgets::{self, BtnKind, BtnSize, PillKind};

/// 软件包详情 — the large modal a card's 查看详情 opens.
///
/// The body is sourced from `winget show`: opening the dialog kicks off a
/// background fetch ([`AppView::fetch_detail`]), and the builder re-reads the
/// view's state on every rebuild, so the parsed detail appears as soon as the
/// fetch lands and every later open renders from the cache.
pub fn pkg_detail(view: &mut AppView, id: &str, window: &mut Window, cx: &mut Context<AppView>) {
    if !view
        .state
        .packages
        .iter()
        .any(|p| p.id.eq_ignore_ascii_case(id))
    {
        return;
    }
    view.fetch_detail(id, cx);

    let lookup_id = id.to_owned();
    let uninstall_id = id.to_owned();
    let install_id = id.to_owned();
    let this = cx.weak_entity();

    window.open_dialog(cx, move |dialog, _, cx| {
        let p = Palette::of(cx);
        let copy_view = this.clone();
        let uninstall_view = this.clone();
        let install_view = this.clone();

        // Re-read on every rebuild: the show fetch lands after the dialog
        // opens, and its notify re-runs this closure.
        let snapshot = this.read_with(cx, |view, _| {
            let pkg = view
                .state
                .packages
                .iter()
                .find(|p| p.id.eq_ignore_ascii_case(&lookup_id))
                .cloned()?;
            let detail = view.state.details.get(&pkg.key()).cloned();
            let command = view.install_command(&pkg.id);
            let busy = crate::queue::is_busy(&view.state, &pkg.id);
            let action = widgets::package_action(&pkg);
            let (label, _) = widgets::install_label(&pkg, busy, action);
            let related: Vec<Package> = view
                .state
                .packages
                .iter()
                .filter(|rel| rel.category == pkg.category && rel.key() != pkg.key())
                .take(3)
                .cloned()
                .collect();
            let icon = view.state.icon_path_for(&pkg.id);
            let related_icons: Vec<_> = related
                .iter()
                .map(|rel| view.state.icon_path_for(&rel.id))
                .collect();
            let loading = detail.is_none() && view.backend.is_some();
            Some((
                pkg,
                detail,
                command,
                busy,
                action,
                label,
                related,
                related_icons,
                icon,
                loading,
            ))
        });
        let Ok(Some((
            pkg,
            detail,
            command,
            busy,
            action,
            label,
            related,
            related_icons,
            icon,
            loading,
        ))) = snapshot
        else {
            return dialog;
        };

        let description = detail
            .as_ref()
            .map(|d| d.description.clone())
            .filter(|d| !d.trim().is_empty())
            .or_else(|| pkg.description.clone())
            .unwrap_or_else(|| tr!("dialog.no_description"));
        let moniker = detail
            .as_ref()
            .map(|d| d.moniker.clone())
            .filter(|m| !m.is_empty());
        let tags = detail
            .as_ref()
            .map(|d| d.tags.clone())
            .filter(|tags| !tags.is_empty());
        let release_notes = detail
            .as_ref()
            .map(|d| d.release_notes.clone())
            .filter(|notes| !notes.trim().is_empty());

        dialog
            .w(px(920.))
            .title(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_semibold()
                            .child(pkg.name.clone()),
                    )
                    .child(div().text_size(px(11.5)).text_color(p.muted).child(format!(
                        "{} · {}",
                        pkg.id,
                        pkg.category.label()
                    ))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(14.))
                            .child(widgets::pkg_icon(&p, &pkg, icon.clone(), 64., 32.))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(6.))
                                    .when(pkg.installed, |this| {
                                        this.child(widgets::pill(
                                            &p,
                                            PillKind::Ok,
                                            tr!("pkg.installed"),
                                        ))
                                    })
                                    .when(pkg.available.is_some(), |this| {
                                        this.child(widgets::pill(
                                            &p,
                                            PillKind::Accent,
                                            tr!("pkg.updatable"),
                                        ))
                                    })
                                    .child(widgets::pill(&p, PillKind::Line, pkg.source_label()))
                                    .when_some(moniker, |this, moniker| {
                                        this.child(widgets::pill(&p, PillKind::Line, moniker))
                                    }),
                            ),
                    )
                    .when(!loading, |this| this.child(prose_lines(&p, &description)))
                    .when(loading, |this| {
                        this.child(
                            div()
                                .text_size(px(11.5))
                                .text_color(p.muted)
                                .child(tr!("dialog.loading")),
                        )
                    })
                    .when_some(pkg.rating, |this, rating| {
                        this.child(widgets::stars(&p, rating))
                    })
                    .child(stat_grid(&p, &pkg, detail.as_ref()))
                    .when_some(release_notes, |this, notes| {
                        this.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .child(widgets::section_label(&p, tr!("dialog.release_notes")))
                                .child(
                                    div()
                                        .id("release-notes")
                                        .max_h(px(160.))
                                        .overflow_y_scroll()
                                        .p(px(10.))
                                        .rounded(px(10.))
                                        .bg(p.surface2)
                                        .border_1()
                                        .border_color(p.line)
                                        .child(prose_lines(&p, &notes)),
                                ),
                        )
                    })
                    .when_some(tags, |this, tags| {
                        this.child(
                            div().flex().flex_wrap().gap(px(6.)).children(
                                tags.iter()
                                    .map(|tag| widgets::pill(&p, PillKind::Line, tag.clone())),
                            ),
                        )
                    })
                    .child(command_block(&p, &command, copy_view.clone()))
                    .when(!related.is_empty(), |this| {
                        this.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .child(widgets::section_label(&p, tr!("dialog.related")))
                                .children(related.iter().zip(related_icons.iter()).map(
                                    |(rel, rel_icon)| {
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(10.))
                                            .child(widgets::pkg_icon(
                                                &p,
                                                rel,
                                                rel_icon.clone(),
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
                                                            .truncate()
                                                            .child(rel.name.clone()),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(11.5))
                                                            .text_color(p.muted)
                                                            .truncate()
                                                            .child(rel.publisher.clone()),
                                                    ),
                                            )
                                    },
                                )),
                        )
                    }),
            )
            .footer(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        widgets::btn(
                            cx,
                            &p,
                            "detail-copy",
                            BtnKind::Outline,
                            BtnSize::Regular,
                            tr!("dialog.copy_command"),
                        )
                        .on_click(move |_, window, cx| {
                            let _ = copy_view.update(cx, |this, cx| {
                                this.copy(&command, window, cx);
                            });
                        }),
                    )
                    .when(pkg.installed, |this| {
                        this.child(
                            widgets::btn(
                                cx,
                                &p,
                                "detail-uninstall",
                                BtnKind::Danger,
                                BtnSize::Regular,
                                tr!("common.uninstall"),
                            )
                            .on_click({
                                let id = uninstall_id.clone();
                                move |_, window, cx| {
                                    let _ = uninstall_view.update(cx, |this, cx| {
                                        confirm_uninstall(this, &id, window, cx);
                                    });
                                }
                            }),
                        )
                    })
                    .child(
                        div().ml_auto().child(
                            widgets::btn(
                                cx,
                                &p,
                                "detail-install",
                                BtnKind::Primary,
                                BtnSize::Regular,
                                label.clone(),
                            )
                            .disabled(busy)
                            .on_click({
                                let id = install_id.clone();
                                move |_, window, cx| {
                                    let _ = install_view.update(cx, |this, cx| {
                                        this.enqueue(&id, action, window, cx);
                                    });
                                }
                            }),
                        ),
                    ),
            )
    });
}

/// The 8-field stat grid the detail dialog shows. Values come from the
/// `winget show` fetch when it has landed, falling back to the package row.
fn stat_grid(p: &Palette, pkg: &Package, detail: Option<&PackageDetail>) -> impl IntoElement {
    // The show value when present, else the row value, else `—`.
    let pick = |shown: &str, fallback: Option<&str>| -> String {
        if !shown.trim().is_empty() {
            shown.to_owned()
        } else {
            fallback
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("—")
                .to_owned()
        }
    };
    let empty = PackageDetail::default();
    let detail = detail.unwrap_or(&empty);
    let fields: [(String, String); 8] = [
        (
            tr!("dialog.version"),
            pick(&detail.version, Some(&pkg.version)),
        ),
        (
            tr!("dialog.publisher"),
            pick(&detail.publisher, Some(&pkg.publisher)),
        ),
        (
            tr!("dialog.license"),
            pick(&detail.license, pkg.license.as_deref()),
        ),
        (
            tr!("dialog.homepage"),
            pick(&detail.homepage, pkg.homepage.as_deref()),
        ),
        (
            tr!("dialog.installer_type"),
            pick(&detail.installer_type, None),
        ),
        (
            tr!("dialog.release_date"),
            pick(&detail.release_date, pkg.updated.as_deref()),
        ),
        (tr!("dialog.source"), pkg.source_label().to_owned()),
        (tr!("dialog.category"), pkg.category.label()),
    ];

    div()
        .flex()
        .flex_wrap()
        .gap(px(8.))
        .children(fields.map(|(label, value)| {
            div()
                .w(px(150.))
                .p(px(10.))
                .rounded(px(10.))
                .bg(p.surface2)
                .border_1()
                .border_color(p.line)
                .child(div().text_size(px(10.5)).text_color(p.muted).child(label))
                .child(div().text_size(px(12.5)).truncate().child(value))
        }))
}

/// Multi-line prose: one div per line so the line breaks `winget show` prints
/// in its Description and Release Notes sections survive the render.
fn prose_lines(p: &Palette, text: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .text_size(px(12.5))
        .line_height(relative(1.6))
        .text_color(p.ink2)
        .children(text.lines().map(|line| div().child(line.to_owned())))
}

/// The 等价命令 block: a `$` prompt, the mono command, and a copy button.
fn command_block(
    p: &Palette,
    command: &str,
    view: gpui_kit::WeakEntity<AppView>,
) -> impl IntoElement {
    let command = command.to_owned();
    let copy_command = command.clone();
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(widgets::section_label(p, tr!("dialog.equivalent_command")))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .p(px(10.))
                .rounded(px(10.))
                .bg(p.surface2)
                .border_1()
                .border_color(p.line)
                .child(div().text_color(p.accent).child("$"))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_family("Cascadia Code")
                        .text_size(px(11.5))
                        .text_color(p.ink2)
                        .child(command),
                )
                .child(
                    widgets::icon_btn("cmd-copy", IconKey::Copy, 24., p.muted).on_click(
                        move |_, window, cx| {
                            let _ = view.update(cx, |this, cx| {
                                this.copy(&copy_command, window, cx);
                            });
                        },
                    ),
                ),
        )
}

/// 错误详情 — the full failure report behind a queue row's 详情 button.
///
/// The report is snapshotted when the dialog opens: it never changes while
/// open, so unlike [`pkg_detail`] the builder does not re-read the view.
pub fn error_detail(view: &mut AppView, uid: u64, window: &mut Window, cx: &mut Context<AppView>) {
    let Some((name, detail)) = view
        .state
        .queue
        .iter()
        .find(|item| item.uid == uid)
        .and_then(|item| {
            item.detail
                .clone()
                .map(|detail| (item.name.clone(), detail))
        })
    else {
        return;
    };
    let this = cx.weak_entity();

    window.open_dialog(cx, move |dialog, _, cx| {
        let p = Palette::of(cx);
        let copy_view = this.clone();
        let copy_text = detail.clone();
        dialog
            .w(px(560.))
            .title(tr!("dialog.error_title"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(widgets::section_label(&p, name.clone()))
                    .child(
                        div()
                            .id("error-detail-body")
                            .max_h(px(320.))
                            .overflow_y_scroll()
                            .p(px(10.))
                            .rounded(px(10.))
                            .bg(p.surface2)
                            .border_1()
                            .border_color(p.line)
                            .font_family("Cascadia Code")
                            .text_size(px(11.5))
                            .line_height(relative(1.5))
                            .text_color(p.ink2)
                            .children(detail.lines().map(|line| div().child(line.to_owned()))),
                    ),
            )
            .footer(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        widgets::btn(
                            cx,
                            &p,
                            "error-copy",
                            BtnKind::Outline,
                            BtnSize::Regular,
                            tr!("dialog.copy_error"),
                        )
                        .on_click(move |_, window, cx| {
                            let _ = copy_view.update(cx, |this, cx| {
                                this.copy(&copy_text, window, cx);
                            });
                        }),
                    )
                    .child(
                        div().ml_auto().child(
                            widgets::btn(
                                cx,
                                &p,
                                "error-close",
                                BtnKind::Primary,
                                BtnSize::Regular,
                                tr!("common.close"),
                            )
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                            }),
                        ),
                    ),
            )
    });
}

/// 高级筛选 — the four flag checkboxes that re-run the search.
pub fn advanced_search(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    let filters = Rc::new(Cell::new(view.state.settings.search_filters()));
    let this = cx.weak_entity();

    window.open_dialog(cx, move |dialog, _, cx| {
        let p = Palette::of(cx);
        let apply_view = this.clone();
        let apply_filters = filters.clone();
        dialog
            .w(px(520.))
            .title(tr!("dialog.advanced_filter"))
            .child(
                div().flex().flex_col().gap(px(6.)).children(
                    [
                        ("--exact", tr!("dialog.exact_hint")),
                        ("--moniker", tr!("dialog.moniker_hint")),
                        ("--tag", tr!("dialog.tag_hint")),
                        ("--command", tr!("dialog.command_hint")),
                    ]
                    .map(|(flag, hint)| {
                        let cell = filters.clone();
                        let current = cell.get();
                        let checked = match flag {
                            "--exact" => current.exact,
                            "--moniker" => current.moniker,
                            "--tag" => current.tag,
                            _ => current.command,
                        };
                        filter_row(&p, flag, hint, checked, move |value| {
                            let mut next = cell.get();
                            match flag {
                                "--exact" => next.exact = value,
                                "--moniker" => next.moniker = value,
                                "--tag" => next.tag = value,
                                _ => next.command = value,
                            }
                            cell.set(next);
                        })
                    }),
                ),
            )
            .footer(
                div().flex().gap(px(8.)).child(
                    widgets::btn(
                        cx,
                        &p,
                        "filter-apply",
                        BtnKind::Primary,
                        BtnSize::Regular,
                        tr!("dialog.apply"),
                    )
                    .on_click(move |_, window, cx| {
                        let next = apply_filters.get();
                        let _ = apply_view.update(cx, |this, cx| {
                            this.state.settings.set_search_filters(next);
                            this.persist_settings();
                            this.refresh_search(cx);
                            cx.notify();
                        });
                        window.close_dialog(cx);
                    }),
                ),
            )
    });
}

fn filter_row(
    p: &Palette,
    flag: &'static str,
    hint: String,
    checked: bool,
    on_change: impl Fn(bool) + 'static,
) -> impl IntoElement {
    let on_change = Rc::new(on_change);
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .p(px(10.))
        .rounded(px(10.))
        .bg(p.surface2)
        .border_1()
        .border_color(p.line)
        .child(widgets::code_chip(p, flag))
        .child(
            div()
                .flex_1()
                .text_size(px(12.))
                .text_color(p.ink2)
                .child(hint),
        )
        .child(widgets::switch_row(
            p,
            SharedString::from(format!("flag-{flag}")),
            checked,
            move |value, _, _| on_change(*value),
        ))
}

/// 添加源 — validates the name is present and unused, then runs `source add`.
pub fn add_source(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    let _ = view;
    let name_input =
        cx.new(|cx| InputState::new(window, cx).placeholder(tr!("placeholder.source_name")));
    let url_input =
        cx.new(|cx| InputState::new(window, cx).placeholder("https://example.com/index.json"));
    let this = cx.weak_entity();

    window.open_dialog(cx, move |dialog, _, cx| {
        let name_input = name_input.clone();
        let url_input = url_input.clone();
        let p = Palette::of(cx);
        let submit_view = this.clone();
        dialog
            .w(px(560.))
            .title(tr!("dialog.add_source"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(form_field(
                        &p,
                        &tr!("dialog.source_name"),
                        Input::new(&name_input).into_any_element(),
                    ))
                    .child(form_field(
                        &p,
                        &tr!("dialog.source_url"),
                        Input::new(&url_input).into_any_element(),
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .p(px(10.))
                            .rounded(px(10.))
                            .border_1()
                            .border_color(p.tint(p.warn, 0.4))
                            .bg(p.tint(p.warn, 0.12))
                            .text_size(px(11.5))
                            .text_color(p.warn)
                            .child(icon(IconKey::Warning, px(14.), p.warn))
                            .child(tr!("dialog.source_admin_hint")),
                    ),
            )
            .footer(
                div().flex().gap(px(8.)).child(
                    widgets::btn(
                        cx,
                        &p,
                        "src-add-ok",
                        BtnKind::Primary,
                        BtnSize::Regular,
                        tr!("common.add"),
                    )
                    .on_click(move |_, window, cx| {
                        let name = name_input.read(cx).value().to_string();
                        let url = url_input.read(cx).value().to_string();
                        let trimmed = name.trim().to_owned();
                        let _ = submit_view.update(cx, |this, cx| {
                            if trimmed.is_empty() {
                                window.push_notification(
                                    Notification::error(tr!("notification.source_name_empty")),
                                    cx,
                                );
                                return;
                            }
                            if this
                                .state
                                .sources
                                .iter()
                                .any(|s| s.name.eq_ignore_ascii_case(&trimmed))
                            {
                                window.push_notification(
                                    Notification::error(tr!(
                                        "notification.source_exists",
                                        name = trimmed
                                    )),
                                    cx,
                                );
                                return;
                            }
                            let Some(backend) = this.backend.clone() else {
                                return;
                            };
                            let args = backend.source_add_args(&trimmed, url.trim());
                            this.run_source_op(args, &tr!("notification.source_added"), window, cx);
                            window.close_dialog(cx);
                        });
                    }),
                ),
            )
    });
}

/// 编辑源 — `winget source edit` only accepts `--explicit`, so the URL is shown
/// read-only and the switch is the one property this dialog can actually write.
pub fn edit_source(view: &mut AppView, name: &str, window: &mut Window, cx: &mut Context<AppView>) {
    let Some(existing) = view.state.sources.iter().find(|s| s.name == name).cloned() else {
        return;
    };
    let source_name = existing.name.clone();
    let source_url = if existing.url.trim().is_empty() {
        "—".to_owned()
    } else {
        existing.url.clone()
    };
    let source_ty = if existing.ty.trim().is_empty() {
        "—".to_owned()
    } else {
        existing.ty.clone()
    };
    let explicit = Rc::new(Cell::new(existing.explicit));
    let this = cx.weak_entity();

    window.open_dialog(cx, move |dialog, _, cx| {
        let p = Palette::of(cx);
        let save_view = this.clone();
        let explicit = explicit.clone();
        let source_name = source_name.clone();
        dialog
            .w(px(560.))
            .title(tr!("dialog.edit_source_title", name = source_name))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(readonly_field(&p, &tr!("dialog.source_url"), &source_url))
                    .child(readonly_field(&p, &tr!("dialog.source_type"), &source_ty))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .p(px(10.))
                            .rounded(px(10.))
                            .bg(p.surface2)
                            .border_1()
                            .border_color(p.line)
                            .child(
                                div()
                                    .flex_1()
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(p.ink2)
                                            .child(tr!("dialog.explicit_label")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(p.muted)
                                            .child(tr!("dialog.explicit_hint")),
                                    ),
                            )
                            .child(widgets::switch_row(&p, "src-explicit", explicit.get(), {
                                let explicit = explicit.clone();
                                move |value, _, _| explicit.set(*value)
                            })),
                    ),
            )
            .footer(
                div().flex().gap(px(8.)).child(
                    widgets::btn(
                        cx,
                        &p,
                        "src-edit-ok",
                        BtnKind::Primary,
                        BtnSize::Regular,
                        tr!("common.save"),
                    )
                    .on_click(move |_, window, cx| {
                        let flag = explicit.get();
                        let name = source_name.clone();
                        let _ = save_view.update(cx, |this, cx| {
                            let Some(backend) = this.backend.clone() else {
                                return;
                            };
                            // A backend without an in-place source edit
                            // (flatpak remotes) has nothing to run.
                            let Some(args) = backend.source_edit_args(&name, Some(flag)) else {
                                window.close_dialog(cx);
                                return;
                            };
                            this.run_source_op(
                                args,
                                &tr!("notification.source_updated"),
                                window,
                                cx,
                            );
                            window.close_dialog(cx);
                        });
                    }),
                ),
            )
    });
}

/// 确认卸载 — a danger alert carrying the equivalent command.
pub fn confirm_uninstall(
    view: &mut AppView,
    id: &str,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    let Some(pkg) = view
        .state
        .packages
        .iter()
        .find(|p| p.id.eq_ignore_ascii_case(id))
        .cloned()
    else {
        return;
    };
    let pkg_id = pkg.id.clone();
    let command = view
        .backend
        .as_ref()
        .map(|backend| {
            format!(
                "{} {}",
                backend.cli_name(),
                backend.uninstall_args(&pkg.id).join(" ")
            )
        })
        .unwrap_or_default();
    let this = cx.weak_entity();

    window.open_alert_dialog(cx, move |alert, _, cx| {
        let p = Palette::of(cx);
        let run_view = this.clone();
        let pkg_id = pkg_id.clone();
        alert
            .title(tr!("dialog.uninstall_title", name = pkg.name))
            .description(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(p.ink2)
                            .child(tr!("dialog.uninstall_body")),
                    )
                    .child(widgets::code_chip(&p, command.clone())),
            )
            .ok_text(tr!("common.uninstall"))
            .ok_variant(ButtonVariant::Danger)
            .cancel_text(tr!("common.cancel"))
            .show_cancel(true)
            .on_ok(move |_: &ClickEvent, window, cx| {
                let id = pkg_id.clone();
                let _ = run_view.update(cx, |this, cx| {
                    let Some(backend) = this.backend.clone() else {
                        return;
                    };
                    let args = backend.uninstall_args(&id);
                    this.run_uninstall(args, &id, window, cx);
                });
                true
            })
    });
}

/// 删除源 — a danger confirm that reassigns the default when it was the target.
pub fn confirm_remove_source(
    _view: &mut AppView,
    name: &str,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    let source_name = name.to_owned();
    let this = cx.weak_entity();

    window.open_alert_dialog(cx, move |alert, _, cx| {
        let p = Palette::of(cx);
        let run_view = this.clone();
        let source_name = source_name.clone();
        alert
            .title(tr!("dialog.remove_source_title", name = source_name))
            .description(
                div()
                    .text_size(px(12.5))
                    .text_color(p.ink2)
                    .child(tr!("dialog.remove_source_body")),
            )
            .ok_text(tr!("common.delete"))
            .ok_variant(ButtonVariant::Danger)
            .cancel_text(tr!("common.cancel"))
            .show_cancel(true)
            .on_ok(move |_: &ClickEvent, window, cx| {
                let name = source_name.clone();
                let _ = run_view.update(cx, |this, cx| {
                    let Some(backend) = this.backend.clone() else {
                        return;
                    };
                    let args = backend.source_remove_args(&name);
                    this.run_source_op(args, &tr!("notification.source_removed"), window, cx);
                });
                true
            })
    });
}

/// 导出清单 — the `winget export` skeleton plus a copy button.
pub fn export_manifest(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    let json = view.state.export_skeleton();
    let this = cx.weak_entity();

    window.open_dialog(cx, move |dialog, _, cx| {
        let p = Palette::of(cx);
        let json_copy = json.clone();
        let copy_view = this.clone();
        dialog
            .w(px(640.))
            .title(tr!("dialog.export_title"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(p.ink2)
                            .child(tr!("dialog.export_body")),
                    )
                    .child(
                        div()
                            .id("json-view")
                            .max_h(px(360.))
                            .overflow_y_scroll()
                            .p(px(12.))
                            .rounded(px(10.))
                            .bg(p.surface2)
                            .border_1()
                            .border_color(p.line)
                            .font_family("Cascadia Code")
                            .text_size(px(11.5))
                            .text_color(p.ink2)
                            .child(json.clone()),
                    ),
            )
            .footer(
                div().flex().gap(px(8.)).child(
                    widgets::btn(
                        cx,
                        &p,
                        "json-copy",
                        BtnKind::Outline,
                        BtnSize::Regular,
                        tr!("dialog.copy_json"),
                    )
                    .on_click(move |_, window, cx| {
                        let _ = copy_view.update(cx, |this, cx| {
                            this.copy(&json_copy, window, cx);
                        });
                    }),
                ),
            )
    });
}

/// A labelled form field: caption above the input.
fn form_field(_p: &Palette, label: &str, control: AnyElement) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .text_size(px(12.))
                .font_medium()
                .child(label.to_owned()),
        )
        .child(control)
}

/// A labelled read-only value, for properties winget does not let this UI edit.
fn readonly_field(p: &Palette, label: &str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .text_size(px(12.))
                .font_medium()
                .child(label.to_owned()),
        )
        .child(
            div()
                .p(px(9.))
                .rounded(px(9.))
                .bg(p.surface3)
                .border_1()
                .border_color(p.line)
                .text_size(px(12.))
                .text_color(p.ink2)
                .child(value.to_owned()),
        )
}
