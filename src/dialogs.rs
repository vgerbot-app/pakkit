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
use crate::theme::{FONT_BODY, FONT_META, FONT_SECTION, HAIRLINE, Palette};
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
            .title(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(FONT_SECTION))
                            .font_semibold()
                            .child(pkg.name.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(8.))
                            .text_size(px(FONT_META))
                            .text_color(p.muted)
                            .child(pkg.id.clone())
                            .child(pkg.category.label()),
                    ),
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
                                .text_size(px(FONT_META))
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
                                        .bg(p.surface)
                                        .border(px(HAIRLINE))
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
                                                            .text_size(px(FONT_BODY))
                                                            .truncate()
                                                            .child(rel.name.clone()),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(FONT_META))
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
            .border(px(HAIRLINE))
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
                .child(
                    div()
                        .text_size(px(FONT_META))
                        .text_color(p.muted)
                        .child(label),
                )
                .child(div().text_size(px(FONT_BODY)).truncate().child(value))
        }))
}

/// Multi-line prose: one div per line so the line breaks `winget show` prints
/// in its Description and Release Notes sections survive the render.
fn prose_lines(p: &Palette, text: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .text_size(px(FONT_BODY))
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
                .rounded(px(8.))
                .bg(p.surface2)
                .child(div().text_color(p.accent).child("$"))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_family("Cascadia Code")
                        .text_size(px(FONT_META))
                        .text_color(p.ink2)
                        .child(command),
                )
                .child(
                    widgets::icon_btn("cmd-copy", IconKey::Copy, 24., p.muted, tr!("common.copy"))
                        .on_click(move |_, window, cx| {
                            let _ = view.update(cx, |this, cx| {
                                this.copy(&copy_command, window, cx);
                            });
                        }),
                ),
        )
}

/// 官网直装详情 — snapshot the selected catalog variant, show exactly what
/// will run or download, and only enqueue after explicit confirmation.
pub fn official_detail(
    view: &mut AppView,
    variant_id: &str,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    view.prepare_official(variant_id, cx);
    let selected = view.state.official.catalog.as_ref().and_then(|catalog| {
        catalog.apps.iter().find_map(|app| {
            app.variants
                .iter()
                .find(|variant| variant.id == variant_id)
                .map(|variant| (app.clone(), variant.clone()))
        })
    });
    let Some((app, variant)) = selected else {
        window.push_notification(
            Notification::error(tr!("notification.official_variant_missing")),
            cx,
        );
        return;
    };
    let language = view.state.settings.language.clone();
    let dynamic = matches!(
        &variant.method,
        crate::official::OfficialMethod::OfficialPage {
            release: Some(_),
            ..
        }
    );
    let queue_id = variant.id.clone();
    let resolution_id = variant.id.clone();
    let this = cx.weak_entity();

    window.open_dialog(cx, move |dialog, _, cx| {
        let p = Palette::of(cx);
        let source_view = this.clone();
        let homepage_view = this.clone();
        let copy_view = this.clone();
        let queue_view = this.clone();
        let retry_view = this.clone();
        let fallback_view = this.clone();
        let queue_id = queue_id.clone();
        let resolution = this
            .read_with(cx, |view, _| {
                view.state.official.resolutions.get(&resolution_id).cloned()
            })
            .ok()
            .flatten();
        let resolved = match &resolution {
            Some(crate::official::OfficialResolutionState::Resolved(resolved)) => Some(resolved),
            _ => None,
        };
        let resolving = dynamic
            && !matches!(
                &resolution,
                Some(crate::official::OfficialResolutionState::Resolved(_))
            );
        let failed_message = match &resolution {
            Some(crate::official::OfficialResolutionState::Failed(message)) => {
                Some(message.clone())
            }
            _ => None,
        };
        let resolution_failed = failed_message.is_some();
        let official_url = match &variant.method {
            crate::official::OfficialMethod::OfficialPage { url, .. } => Some(url.clone()),
            _ => None,
        };
        let accent = Palette::from_hex(&app.accent);
        let (method_label, _) = official_method_labels(&variant.method);
        let method_label = if dynamic {
            tr!("official.method.release")
        } else {
            method_label
        };
        let payload = if let Some(resolved) = resolved {
            resolved.resolved_url.clone()
        } else {
            match &variant.method {
                crate::official::OfficialMethod::Script { command, .. } => command.clone(),
                crate::official::OfficialMethod::Artifact { url, .. }
                | crate::official::OfficialMethod::OfficialPage { url, .. } => url.clone(),
            }
        };
        let payload_label = if dynamic && resolved.is_some() {
            tr!("official.detail.resolved_url")
        } else if dynamic {
            tr!("official.detail.download_url")
        } else if matches!(
            variant.method,
            crate::official::OfficialMethod::Script { .. }
        ) {
            tr!("official.detail.command")
        } else {
            tr!("official.detail.download_url")
        };
        let shell_label = if dynamic {
            "GitHub Release".to_owned()
        } else {
            match &variant.method {
                crate::official::OfficialMethod::Script {
                    shell: crate::official::OfficialShell::Powershell,
                    ..
                } => "PowerShell".to_owned(),
                crate::official::OfficialMethod::Script { .. } => "Shell".to_owned(),
                _ => tr!("official.detail.https_direct"),
            }
        };
        let action_semantics = if dynamic {
            if let Some(resolved) = resolved {
                match resolved.install_action {
                    crate::official::resolver::OfficialInstallAction::InstallPackage => {
                        tr!("official.detail.semantic_package_install")
                    }
                    crate::official::resolver::OfficialInstallAction::RunInstaller => {
                        tr!("official.detail.semantic_installer")
                    }
                    crate::official::resolver::OfficialInstallAction::OpenInstaller => {
                        tr!("official.detail.semantic_open_installer")
                    }
                    crate::official::resolver::OfficialInstallAction::DownloadPortable => {
                        tr!("official.detail.semantic_portable")
                    }
                }
            } else if failed_message.is_some() {
                tr!("official.resolve.failed_short")
            } else {
                tr!("official.resolve.resolving")
            }
        } else {
            match &variant.method {
                crate::official::OfficialMethod::Script { .. } if variant.interactive => {
                    tr!("official.detail.semantic_terminal")
                }
                crate::official::OfficialMethod::Script { .. } => {
                    tr!("official.detail.semantic_script")
                }
                crate::official::OfficialMethod::Artifact {
                    action: crate::official::ArtifactAction::RunInstaller,
                    ..
                } => tr!("official.detail.semantic_installer"),
                crate::official::OfficialMethod::Artifact {
                    action: crate::official::ArtifactAction::DownloadPackage,
                    ..
                } => tr!("official.detail.semantic_package"),
                crate::official::OfficialMethod::Artifact {
                    action: crate::official::ArtifactAction::DownloadPortable,
                    ..
                } => tr!("official.detail.semantic_portable"),
                crate::official::OfficialMethod::OfficialPage { .. } => {
                    tr!("official.detail.semantic_page")
                }
            }
        };
        let no_sha = matches!(
            &variant.method,
            crate::official::OfficialMethod::Artifact { sha256: None, .. }
        );
        let resolved_metadata = resolved.map(|resolved| {
            let size = resolved.size.map_or_else(
                || tr!("official.resolve.unknown_size"),
                |size| format!("{size} bytes / {:.1} MiB", size as f64 / 1024.0 / 1024.0),
            );
            format!(
                "{}\n{}\n{}\nSHA-256: {}\n{}: {}",
                tr!(
                    "official.resolve.version",
                    version = resolved.version.as_deref().unwrap_or("-")
                ),
                tr!("official.resolve.file", file = resolved.file_name),
                tr!("official.resolve.size", size = size),
                resolved.sha256,
                tr!("official.resolve.format"),
                resolved.format.extension().to_ascii_uppercase(),
            )
        });
        let requires_admin =
            resolved.map_or(variant.requires_admin, |resolved| resolved.requires_admin);

        dialog
            .w(px(680.))
            .title(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .size(px(44.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(12.))
                            .bg(p.tint(accent, 0.12))
                            .text_color(accent)
                            .text_size(px(FONT_SECTION))
                            .font_semibold()
                            .child(app.mark.clone()),
                    )
                    .child(
                        div()
                            .child(
                                div()
                                    .text_size(px(FONT_SECTION))
                                    .font_semibold()
                                    .child(app.name.clone()),
                            )
                            .child(
                                div()
                                    .mt(px(3.))
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(8.))
                                    .text_size(px(FONT_META))
                                    .text_color(p.muted)
                                    .child(app.publisher.clone())
                                    .child(method_label.clone()),
                            ),
                        ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(13.))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(7.))
                            .child(widgets::pill(&p, PillKind::Line, variant.os.label()))
                            .child(widgets::pill(
                                &p,
                                PillKind::Line,
                                variant
                                    .architectures
                                    .iter()
                                    .map(|arch| arch.label())
                                    .collect::<Vec<_>>()
                                    .join(" / "),
                            ))
                            .child(widgets::pill(
                                &p,
                                if requires_admin {
                                    PillKind::Warn
                                } else {
                                    PillKind::Ok
                                },
                                if requires_admin {
                                    tr!("official.detail.admin")
                                } else {
                                    tr!("official.current_user")
                                },
                            ))
                            .child(widgets::pill(
                                &p,
                                PillKind::Line,
                                if variant.interactive {
                                    tr!("official.detail.interactive")
                                } else {
                                    tr!("official.detail.non_interactive")
                                },
                            )),
                    )
                    .child(official_link_row(
                        &p,
                        tr!("official.detail.source"),
                        variant.source_url.clone(),
                        source_view,
                    ))
                    .child(official_link_row(
                        &p,
                        tr!("official.detail.homepage"),
                        app.homepage.clone(),
                        homepage_view,
                    ))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(8.))
                            .p(px(10.))
                            .rounded(px(9.))
                            .bg(p.surface2)
                            .text_size(px(FONT_META))
                            .text_color(p.ink2)
                            .child(icon(IconKey::Info, px(14.), p.accent))
                            .child(action_semantics),
                    )
                    .child(official_payload_block(
                        &p,
                        &payload_label,
                        &shell_label,
                        &payload,
                        copy_view,
                    ))
                    .when_some(resolved_metadata, |this, metadata| {
                        this.child(
                            div()
                                .p(px(10.))
                                .rounded(px(9.))
                                .bg(p.surface2)
                                .text_size(px(FONT_META))
                                .text_color(p.ink2)
                                .child(metadata),
                        )
                    })
                    .when_some(failed_message, |this, message| {
                        this.child(
                            div()
                                .flex()
                                .items_start()
                                .gap(px(8.))
                                .p(px(10.))
                                .rounded(px(9.))
                                .bg(p.tint(p.danger, 0.10))
                                .text_size(px(FONT_META))
                                .text_color(p.danger)
                                .child(icon(IconKey::Warning, px(14.), p.danger))
                                .child(message),
                        )
                    })
                    .when(no_sha, |this| {
                        this.child(
                            div()
                                .flex()
                                .items_start()
                                .gap(px(8.))
                                .p(px(10.))
                                .rounded(px(9.))
                                .bg(p.tint(p.warn, 0.10))
                                .text_size(px(FONT_META))
                                .text_color(p.warn)
                                .child(icon(IconKey::Warning, px(14.), p.warn))
                                .child(tr!("official.detail.no_sha")),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(8.))
                            .p(px(10.))
                            .rounded(px(9.))
                            .bg(p.surface2)
                            .text_size(px(FONT_META))
                            .text_color(p.ink2)
                            .child(icon(IconKey::Info, px(14.), p.accent))
                            .child(variant.note.get(&language).to_owned()),
                    ),
            )
            .footer(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.))
                    .when_some(official_url, |this, url| {
                        this.child(
                            widgets::btn(
                                cx,
                                &p,
                                "official-detail-fallback",
                                BtnKind::Outline,
                                BtnSize::Regular,
                                tr!("official.detail.open_official"),
                            )
                            .on_click(move |_, window, cx| {
                                let _ = fallback_view.update(cx, |this, cx| {
                                    this.open_official_url(url.clone(), window, cx);
                                });
                            }),
                        )
                    })
                    .when(resolution_failed, |this| {
                        let retry_id = queue_id.clone();
                        this.child(
                            widgets::btn(
                                cx,
                                &p,
                                "official-detail-retry",
                                BtnKind::Outline,
                                BtnSize::Regular,
                                tr!("official.resolve.retry"),
                            )
                            .on_click(move |_, _, cx| {
                                let _ = retry_view.update(cx, |this, cx| {
                                    this.retry_official_resolution(&retry_id, cx);
                                });
                            }),
                        )
                    })
                    .child(
                        widgets::btn(
                            cx,
                            &p,
                            "official-detail-cancel",
                            BtnKind::Outline,
                            BtnSize::Regular,
                            tr!("common.cancel"),
                        )
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        widgets::btn(
                            cx,
                            &p,
                            "official-detail-queue",
                            BtnKind::Primary,
                            BtnSize::Regular,
                            if dynamic
                                && resolved.is_some_and(|resolved| {
                                    resolved.install_action
                                        == crate::official::resolver::OfficialInstallAction::DownloadPortable
                                })
                            {
                                tr!("official.detail.queue")
                            } else if dynamic {
                                tr!("official.detail.install")
                            } else {
                                tr!("official.detail.queue")
                            },
                        )
                        .disabled(resolving)
                        .on_click(move |_, window, cx| {
                            let _ = queue_view.update(cx, |this, cx| {
                                this.enqueue_official(&queue_id, window, cx);
                            });
                            window.close_dialog(cx);
                        }),
                    ),
            )
            .border(px(HAIRLINE))
    });
}

fn official_method_labels(method: &crate::official::OfficialMethod) -> (String, String) {
    match method {
        crate::official::OfficialMethod::Script { .. } => {
            (tr!("official.method.script"), tr!("official.action.script"))
        }
        crate::official::OfficialMethod::Artifact { action, .. } => match action {
            crate::official::ArtifactAction::RunInstaller => (
                tr!("official.method.installer"),
                tr!("official.action.installer"),
            ),
            crate::official::ArtifactAction::DownloadPackage => (
                tr!("official.method.package"),
                tr!("official.action.package"),
            ),
            crate::official::ArtifactAction::DownloadPortable => (
                tr!("official.method.portable"),
                tr!("official.action.portable"),
            ),
        },
        crate::official::OfficialMethod::OfficialPage { .. } => (
            tr!("official.method.official_page"),
            tr!("official.action.official_page"),
        ),
    }
}

fn official_link_row(
    p: &Palette,
    label: String,
    url: String,
    view: gpui_kit::WeakEntity<AppView>,
) -> impl IntoElement {
    let open_url = url.clone();
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .p(px(10.))
        .rounded(px(9.))
        .bg(p.surface2)
        .child(icon(IconKey::Shield, px(14.), p.ok))
        .child(
            div()
                .text_size(px(FONT_META))
                .text_color(p.muted)
                .child(label),
        )
        .child(
            div()
                .id(SharedString::from(format!("official-link-{url}")))
                .flex_1()
                .min_w_0()
                .text_size(px(FONT_META))
                .text_color(p.accent)
                .cursor_pointer()
                .hover(|this| this.underline())
                .on_click(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.open_official_url(open_url.clone(), window, cx);
                    });
                })
                .child(url),
        )
}

fn official_payload_block(
    p: &Palette,
    label: &str,
    kind: &str,
    payload: &str,
    view: gpui_kit::WeakEntity<AppView>,
) -> impl IntoElement {
    let copied = payload.to_owned();
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(widgets::section_label(p, label))
        .child(
            div()
                .rounded(px(8.))
                .bg(p.surface2)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.))
                        .px(px(9.))
                        .py(px(6.))
                        .text_size(px(FONT_META))
                        .text_color(p.muted)
                        .child(kind.to_owned())
                        .child(
                            widgets::icon_btn(
                                "official-copy",
                                IconKey::Copy,
                                24.,
                                p.muted,
                                tr!("common.copy"),
                            )
                            .on_click(move |_, window, cx| {
                                let _ = view.update(cx, |this, cx| {
                                    this.copy(&copied, window, cx);
                                });
                            }),
                        ),
                )
                .child(
                    div()
                        .p(px(12.))
                        .font_family("Cascadia Code")
                        .text_size(px(FONT_META))
                        .line_height(relative(1.7))
                        .text_color(p.ink2)
                        .child(payload.to_owned()),
                ),
        )
}

/// 提交官网软件 — validate locally, then open the catalog's prefilled GitHub
/// issue URL. Opening the browser is not reported as a completed submission.
pub fn official_submission(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    let Some(base) = view
        .state
        .official
        .catalog
        .as_ref()
        .map(|catalog| catalog.submission_url.clone())
    else {
        window.push_notification(
            Notification::error(tr!("notification.official_submission_unavailable")),
            cx,
        );
        return;
    };
    let name_input =
        cx.new(|cx| InputState::new(window, cx).placeholder(tr!("placeholder.official_name")));
    let url_input = cx.new(|cx| InputState::new(window, cx).placeholder("https://…"));
    let note_input =
        cx.new(|cx| InputState::new(window, cx).placeholder(tr!("placeholder.official_note")));
    let this = cx.weak_entity();

    window.open_dialog(cx, move |dialog, _, cx| {
        let p = Palette::of(cx);
        let name_input = name_input.clone();
        let url_input = url_input.clone();
        let note_input = note_input.clone();
        let submit_view = this.clone();
        let base = base.clone();
        dialog
            .w(px(580.))
            .title(
                div()
                    .text_size(px(FONT_SECTION))
                    .child(tr!("official.submission.title"))
                    .child(
                        div()
                            .mt(px(3.))
                            .text_size(px(FONT_META))
                            .font_normal()
                            .text_color(p.muted)
                            .child(tr!("official.submission.subtitle")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(form_field(
                        &p,
                        &tr!("official.submission.name"),
                        Input::new(&name_input).into_any_element(),
                    ))
                    .child(form_field(
                        &p,
                        &tr!("official.submission.url"),
                        Input::new(&url_input).into_any_element(),
                    ))
                    .child(form_field(
                        &p,
                        &tr!("official.submission.note"),
                        Input::new(&note_input).into_any_element(),
                    ))
                    .child(
                        div()
                            .text_size(px(FONT_META))
                            .line_height(relative(1.5))
                            .text_color(p.muted)
                            .child(tr!("official.submission.hint")),
                    ),
            )
            .footer(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        widgets::btn(
                            cx,
                            &p,
                            "official-submit-cancel",
                            BtnKind::Outline,
                            BtnSize::Regular,
                            tr!("common.cancel"),
                        )
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        widgets::btn(
                            cx,
                            &p,
                            "official-submit-open",
                            BtnKind::Primary,
                            BtnSize::Regular,
                            tr!("official.submission.open"),
                        )
                        .on_click(move |_, window, cx| {
                            let name = name_input.read(cx).value().trim().to_owned();
                            let link = url_input.read(cx).value().trim().to_owned();
                            let note = note_input.read(cx).value().trim().to_owned();
                            if name.is_empty() {
                                window.push_notification(
                                    Notification::error(tr!("notification.official_name_empty")),
                                    cx,
                                );
                                return;
                            }
                            let valid_https = url::Url::parse(&link).is_ok_and(|url| {
                                url.scheme() == "https" && url.host_str().is_some()
                            });
                            if !valid_https {
                                window.push_notification(
                                    Notification::error(tr!("notification.official_url_invalid")),
                                    cx,
                                );
                                return;
                            }
                            match crate::official::submission_url(&base, &name, &link, &note) {
                                Ok(url) => {
                                    let _ = submit_view.update(cx, |this, cx| {
                                        this.open_official_url(url, window, cx);
                                    });
                                    window.close_dialog(cx);
                                }
                                Err(_) => {
                                    window.push_notification(
                                        Notification::error(tr!(
                                            "notification.official_submission_invalid"
                                        )),
                                        cx,
                                    );
                                }
                            }
                        }),
                    ),
            )
            .border(px(HAIRLINE))
    });
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
            .title(
                div()
                    .text_size(px(FONT_SECTION))
                    .child(tr!("dialog.error_title")),
            )
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
                            .rounded(px(8.))
                            .bg(p.surface2)
                            .font_family("Cascadia Code")
                            .text_size(px(FONT_META))
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
            .border(px(HAIRLINE))
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
            .title(
                div()
                    .text_size(px(FONT_SECTION))
                    .child(tr!("dialog.advanced_filter")),
            )
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
            .border(px(HAIRLINE))
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
        .child(widgets::code_chip(p, flag))
        .child(
            div()
                .flex_1()
                .text_size(px(FONT_BODY))
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
            .title(
                div()
                    .text_size(px(FONT_SECTION))
                    .child(tr!("dialog.add_source")),
            )
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
                            .border(px(HAIRLINE))
                            .border_color(p.tint(p.warn, 0.4))
                            .bg(p.tint(p.warn, 0.12))
                            .text_size(px(FONT_META))
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
            .border(px(HAIRLINE))
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
            .title(
                div()
                    .text_size(px(FONT_SECTION))
                    .child(tr!("dialog.edit_source_title", name = source_name)),
            )
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
                            .child(
                                div()
                                    .flex_1()
                                    .child(
                                        div()
                                            .text_size(px(FONT_BODY))
                                            .text_color(p.ink2)
                                            .child(tr!("dialog.explicit_label")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(FONT_META))
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
            .border(px(HAIRLINE))
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
            .title(
                div()
                    .text_size(px(FONT_SECTION))
                    .child(tr!("dialog.uninstall_title", name = pkg.name)),
            )
            .description(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(FONT_BODY))
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
            .border(px(HAIRLINE))
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
            .title(
                div()
                    .text_size(px(FONT_SECTION))
                    .child(tr!("dialog.remove_source_title", name = source_name)),
            )
            .description(
                div()
                    .text_size(px(FONT_BODY))
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
            .border(px(HAIRLINE))
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
            .title(
                div()
                    .text_size(px(FONT_SECTION))
                    .child(tr!("dialog.export_title")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(FONT_BODY))
                            .text_color(p.ink2)
                            .child(tr!("dialog.export_body")),
                    )
                    .child(
                        div()
                            .id("json-view")
                            .max_h(px(360.))
                            .overflow_y_scroll()
                            .p(px(12.))
                            .rounded(px(8.))
                            .bg(p.surface2)
                            .font_family("Cascadia Code")
                            .text_size(px(FONT_META))
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
            .border(px(HAIRLINE))
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
                .text_size(px(FONT_BODY))
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
                .text_size(px(FONT_BODY))
                .font_medium()
                .child(label.to_owned()),
        )
        .child(
            div()
                .p(px(9.))
                .rounded(px(9.))
                .bg(p.surface3)
                .border(px(HAIRLINE))
                .border_color(p.line)
                .text_size(px(FONT_BODY))
                .text_color(p.ink2)
                .child(value.to_owned()),
        )
}
