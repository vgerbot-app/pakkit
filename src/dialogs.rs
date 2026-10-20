//! Modal dialogs, opened through `WindowExt`.
//!
//! A dialog's builder is a *render* closure that runs again on layer rebuilds,
//! so it must be an `Fn`: every persistent entity (the text inputs) and every
//! listener is created before the dialog opens. Inside the builder the view is
//! reached through a captured [`WeakEntity`], because the builder only gets
//! `&mut App`, never the view's own context.

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


use crate::icons::{IconKey, icon};
use crate::shell::AppView;
use crate::theme::Palette;
use crate::widgets::{self, BtnKind, BtnSize, PillKind};
use crate::winget;

/// 软件包详情 — the large modal a card's 查看详情 opens.
pub fn pkg_detail(view: &mut AppView, id: &str, window: &mut Window, cx: &mut Context<AppView>) {
    let Some(pkg) = view
        .state
        .packages
        .iter()
        .find(|p| p.id.eq_ignore_ascii_case(id))
        .cloned()
    else {
        return;
    };

    let command = view.state.install_command(&pkg.id);
    let uninstall_id = pkg.id.clone();
    let install_id = pkg.id.clone();
    let busy = crate::queue::is_busy(&view.state, &pkg.id);
    let action = widgets::package_action(&pkg);
    let (label, _) = widgets::install_label(&pkg, busy, action);
    let related: Vec<_> = view
        .state
        .packages
        .iter()
        .filter(|p| p.category == pkg.category && p.key() != pkg.key())
        .take(3)
        .cloned()
        .collect();
    let this = cx.weak_entity();

    window.open_dialog(cx, move |dialog, _, cx| {
        let p = Palette::of(cx);
        let copy_view = this.clone();
        let uninstall_view = this.clone();
        let install_view = this.clone();
        let command = command.clone();
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
                            .child(widgets::pkg_chip(&p, &pkg, 64., 32.))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(6.))
                                    .when(pkg.installed, |this| {
                                        this.child(widgets::pill(&p, PillKind::Ok, tr!("pkg.installed")))
                                    })
                                    .when(pkg.available.is_some(), |this| {
                                        this.child(widgets::pill(
                                            &p,
                                            PillKind::Accent,
                                            tr!("pkg.updatable"),
                                        ))
                                    })
                                    .child(widgets::pill(&p, PillKind::Line, pkg.source_label()))
                                    .when_some(pkg.tags.first().cloned(), |this, tag| {
                                        this.child(widgets::pill(&p, PillKind::Line, tag))
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(relative(1.6))
                            .text_color(p.ink2)
                            .child(
                                pkg.description
                                    .clone()
                                    .unwrap_or_else(|| tr!("dialog.no_description")),
                            ),
                    )
                    .when_some(pkg.rating, |this, rating| {
                        this.child(widgets::stars(&p, rating))
                    })
                    .child(stat_grid(&p, &pkg))
                    .child(command_block(&p, &command, copy_view.clone()))
                    .when(!related.is_empty(), |this| {
                        this.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .child(widgets::section_label(&p, tr!("dialog.related")))
                                .children(related.iter().map(|rel| {
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(10.))
                                        .child(widgets::pkg_chip(&p, rel, 32., 16.))
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
                                })),
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
                                move |_, _window, cx| {
                                    let _ = uninstall_view.update_in(cx, |this, window, cx| {
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
                                move |_, _window, cx| {
                                    let _ = install_view.update_in(cx, |this, window, cx| {
                                        this.enqueue(&id, action, window, cx);
                                    });
                                }
                            }),
                        ),
                    ),
            )
    });
}

/// The 8-field stat grid the detail dialog shows.
fn stat_grid(p: &Palette, pkg: &crate::model::Package) -> impl IntoElement {
    let fields: [(String, String); 8] = [
        (tr!("dialog.version"), pkg.version.clone()),
        (tr!("dialog.publisher"), non_empty(&pkg.publisher)),
        (
            tr!("dialog.size"),
            pkg.size.clone().unwrap_or_else(|| "—".to_owned()),
        ),
        (
            tr!("dialog.license"),
            pkg.license.clone().unwrap_or_else(|| "—".to_owned()),
        ),
        (
            tr!("dialog.updated"),
            pkg.updated.clone().unwrap_or_else(|| "—".to_owned()),
        ),
        (
            tr!("dialog.installs"),
            pkg.installs
                .map_or_else(|| "—".to_owned(), |n| n.to_string()),
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
                    .on_click(move |_, _window, cx| {
                        let name = name_input.read(cx).value().to_string();
                        let url = url_input.read(cx).value().to_string();
                        let trimmed = name.trim().to_owned();
                        let _ = submit_view.update_in(cx, |this, window, cx| {
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
                            let args = winget::source_add(
                                &trimmed,
                                url.trim(),
                                "Microsoft.PreIndexed.Package",
                                true,
                            );
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
                    .on_click(move |_, _window, cx| {
                        let flag = explicit.get();
                        let name = source_name.clone();
                        let _ = save_view.update_in(cx, |this, window, cx| {
                            let args = winget::source_edit(&name, Some(flag));
                            this.run_source_op(args, &tr!("notification.source_updated"), window, cx);
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
    let command = format!("winget uninstall --id {} --exact --source winget", pkg.id);
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
            .on_ok(move |_: &ClickEvent, _window, cx| {
                let id = pkg_id.clone();
                let _ = run_view.update_in(cx, |this, window, cx| {
                    let args = winget::uninstall(&id);
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
            .on_ok(move |_: &ClickEvent, _window, cx| {
                let name = source_name.clone();
                let _ = run_view.update_in(cx, |this, window, cx| {
                    if this.state.default_source == name {
                        this.state.default_source = "winget".to_owned();
                        this.state.settings.default_source = "winget".to_owned();
                        this.persist_settings();
                    }
                    let args = winget::source_remove(&name);
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

fn non_empty(value: &str) -> String {
    if value.trim().is_empty() {
        "—".to_owned()
    } else {
        value.to_owned()
    }
}
