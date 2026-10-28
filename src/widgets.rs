//! Shared widgets.
//!
//! Every entry point is a plain function returning a composed element, so a
//! view reads as markup and there is no second component system beside
//! gpui-kit's own. Metrics come from `prototype/assets/css/app.css`; the
//! comments name the rule each one reproduces.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{StyledExt as _, h_flex};
use gpui_kit::prelude::{
    FluentBuilder as _, InteractiveElement as _, StatefulInteractiveElement as _,
};
use gpui_kit::{
    AnyElement, App, BoxShadow, ClickEvent, Div, ElementId, Hsla, IntoElement as _,
    ParentElement as _, Pixels, Rgba, SharedString, Stateful, Styled as _, Window, div, point, px,
    relative,
};

use crate::icons::{IconKey, icon};
use crate::model::{Package, Source};
use crate::queue::QueueAction;
use crate::state::{CategorySort, Language, SearchSource, SortMode, StatusFilter};
use crate::theme::{ModeChoice, Palette};

/// `.card` — surface panel with a hairline border and the 14 px radius.
pub fn card(p: &Palette) -> Div {
    div()
        .bg(p.surface)
        .border_1()
        .border_color(p.line)
        .rounded(px(14.))
}

/// `.card-hover` — `box-shadow: 0 1px 2px rgba(0,0,0,.06), 0 8px 24px -16px rgba(0,0,0,.35)`.
pub fn card_hover(p: &Palette, el: Div) -> Div {
    el.hover(move |this| {
        this.border_color(p.line_strong).shadow(vec![
            BoxShadow {
                color: Hsla { a: 0.06, ..p.ink },
                offset: point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
                inset: false,
            },
            BoxShadow {
                color: Hsla { a: 0.35, ..p.ink },
                offset: point(px(0.), px(8.)),
                blur_radius: px(24.),
                spread_radius: px(-16.),
                inset: false,
            },
        ])
    })
}

/// `shadow-pop: 0 24px 60px -20px rgba(0,0,0,.55)`.
pub fn pop_shadow(p: &Palette) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: Hsla { a: 0.55, ..p.ink },
        offset: point(px(0.), px(24.)),
        blur_radius: px(60.),
        spread_radius: px(-20.),
        inset: false,
    }]
}

/// The button metrics `app.css` defines for `.btn` / `.btn-sm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtnSize {
    /// `.btn` — 34 px tall, 9 px radius, 13 px text.
    Regular,
    /// `.btn-sm` — 29 px tall, 8 px radius, 12 px text.
    Small,
}

impl BtnSize {
    pub fn height(self) -> Pixels {
        match self {
            BtnSize::Regular => px(34.),
            BtnSize::Small => px(29.),
        }
    }

    pub fn text_size(self) -> Pixels {
        match self {
            BtnSize::Regular => px(13.),
            BtnSize::Small => px(12.),
        }
    }

    /// The glyph size a button's icon slot uses.
    pub fn glyph(self) -> f32 {
        match self {
            BtnSize::Regular => 15.,
            BtnSize::Small => 13.,
        }
    }

    /// `.btn` — 9 px radius, `.btn-sm` — 8 px.
    pub fn radius(self) -> Pixels {
        match self {
            BtnSize::Regular => px(9.),
            BtnSize::Small => px(8.),
        }
    }

    /// `0 14px` regular, `0 11px` small.
    pub fn padding_x(self) -> Pixels {
        match self {
            BtnSize::Regular => px(14.),
            BtnSize::Small => px(11.),
        }
    }
}

/// The button variants `app.css` defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtnKind {
    /// `.btn-primary` — accent fill, `on-accent` text.
    Primary,
    /// `.btn-soft` — `surface3` fill with a `line` border.
    Soft,
    /// `.btn-outline` — surface fill with a `line-strong` border.
    Outline,
    /// `.btn-danger` — danger tint, danger text.
    Danger,
}

/// A `Button` styled to one of the prototype's button variants.
///
/// The stock variants cannot reproduce all four: `.btn-soft`'s fill/border
/// pair and `.btn-danger`'s tint have no equivalent, so they go through
/// [`ButtonCustomVariant`] — whose fields are private, hence the builder and
/// the `cx` argument.
pub fn btn(
    cx: &App,
    p: &Palette,
    id: impl Into<ElementId>,
    kind: BtnKind,
    size: BtnSize,
    label: impl Into<SharedString>,
) -> Button {
    let custom = |bg: Hsla, fg: Hsla, hover: Hsla| {
        ButtonCustomVariant::new(cx)
            .color(bg)
            .foreground(fg)
            .hover(hover)
            .active(hover)
    };
    let button = match kind {
        BtnKind::Primary => {
            let hover = brighten(p.accent, 1.08);
            Button::new(id)
                .custom(custom(p.accent, p.on_accent, hover))
                // The custom variant folds `color` into the surface at 20%,
                // which reads as a disabled fill; the fill is set outright instead.
                // The hover/active rules are appended after this and layer over it.
                .bg(p.accent)
        }
        BtnKind::Soft => Button::new(id).custom(custom(p.surface3, p.ink, p.surface3)),
        BtnKind::Outline => Button::new(id).custom(custom(p.surface, p.ink2, p.surface3)),
        BtnKind::Danger => Button::new(id).custom(custom(
            p.tint(p.danger, 0.14),
            p.danger,
            p.tint(p.danger, 0.22),
        )),
    };
    button
        .label(label)
        .text_size(size.text_size())
        .px(size.padding_x())
        .h(size.height())
        .rounded(size.radius())
}

/// The hairline the prototype's `.btn` rules carry; the custom variant paints
/// no border of its own.
pub fn button_border(p: &Palette, kind: BtnKind, button: Button) -> Button {
    match kind {
        BtnKind::Primary | BtnKind::Soft => button.border_1().border_color(p.line),
        BtnKind::Outline => button.border_1().border_color(p.line_strong),
        BtnKind::Danger => button.border_1().border_color(p.tint(p.danger, 0.3)),
    }
}

/// The `.btn-primary:hover { filter: brightness(1.08) }` adjustment.
fn brighten(color: Hsla, factor: f32) -> Hsla {
    let rgb = color.to_rgb();
    Hsla::from(Rgba {
        r: (rgb.r * factor).min(1.),
        g: (rgb.g * factor).min(1.),
        b: (rgb.b * factor).min(1.),
        a: rgb.a,
    })
}

/// `.btn-ghost` — borderless, 12 px, muted, darkening to `ink` on hover.
///
/// The hover colours come from a custom variant, never from `Div::hover`:
/// `Button` forwards `InteractiveElement` to its base div, and `Button::render`
/// registers the base's hover style itself — setting one here would panic with
/// "hover style already set".
pub fn btn_ghost(
    cx: &App,
    p: &Palette,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
) -> Button {
    Button::new(id)
        .label(label)
        .custom(
            ButtonCustomVariant::new(cx)
                .color(Hsla::default())
                .foreground(p.muted)
                .hover(p.surface3)
                .active(p.surface3),
        )
        .text_size(px(12.))
        .px(px(9.))
        .py(px(4.))
        .rounded(px(7.))
}

/// `.icon-btn` — a square, borderless icon hit target.
pub fn icon_btn(id: impl Into<ElementId>, key: IconKey, side: f32, color: Hsla) -> Button {
    Button::new(id)
        .icon(icon(key, px(side * 0.6), color))
        .ghost()
        .rounded(px(8.))
        .size(px(side))
}

/// The `.badge` pill variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PillKind {
    /// `.badge-line` — `surface3` fill, `ink2` text.
    Line,
    /// `.badge-accent`.
    Accent,
    /// `.badge-ok`.
    Ok,
    /// `.badge-warn`.
    Warn,
    /// `.badge-danger`.
    Danger,
}

/// `.badge` — 10.5 px, 4×7 px padding, 6 px radius, a ~15 %-alpha tint.
pub fn pill(p: &Palette, kind: PillKind, text: impl Into<SharedString>) -> Div {
    let (bg, fg) = match kind {
        PillKind::Line => (p.surface3, p.ink2),
        PillKind::Accent => (p.tint(p.accent, 0.15), p.accent),
        PillKind::Ok => (p.tint(p.ok, 0.15), p.ok),
        PillKind::Warn => (p.tint(p.warn, 0.16), p.warn),
        PillKind::Danger => (p.tint(p.danger, 0.15), p.danger),
    };
    let border = match kind {
        PillKind::Line => p.line,
        PillKind::Accent => p.tint(p.accent, 0.35),
        PillKind::Ok => p.tint(p.ok, 0.35),
        PillKind::Warn => p.tint(p.warn, 0.35),
        PillKind::Danger => p.tint(p.danger, 0.35),
    };
    div()
        .text_size(px(10.5))
        .line_height(relative(1.))
        .px(px(7.))
        .py(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_color(fg)
        .font_medium()
        .whitespace_nowrap()
        .child(text.into())
}

/// `.seg` — the segmented control: a `surface3` track holding 27 px buttons.
///
/// Built from plain `div`s rather than `ToggleGroup::segmented()` because the
/// 3 px track padding and 10 px radius are not expressible on the component.
pub fn seg_control<T: Clone + PartialEq + 'static>(
    p: &Palette,
    id: &'static str,
    items: &[(T, String)],
    selected: T,
    on_select: impl Fn(T, &mut Window, &mut App) + 'static,
) -> Div {
    let on_select = Rc::new(on_select);
    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .p(px(3.))
        .rounded(px(10.))
        .bg(p.surface3)
        .border_1()
        .border_color(p.line)
        .children(items.iter().enumerate().map(|(ix, (value, label))| {
            let value = value.clone();
            let is_on = value == selected;
            let handler = on_select.clone();
            let idle = p.muted;
            let hot = p.ink;
            div()
                .id((id, ix))
                .flex()
                .items_center()
                .h(px(27.))
                .px(px(12.))
                .rounded(px(7.))
                .text_size(px(12.5))
                .whitespace_nowrap()
                .cursor_pointer()
                .when(is_on, |this| {
                    this.bg(p.surface)
                        .text_color(p.ink)
                        .font_semibold()
                        .shadow(vec![BoxShadow {
                            color: Hsla { a: 0.12, ..p.ink },
                            offset: point(px(0.), px(1.)),
                            blur_radius: px(3.),
                            spread_radius: px(0.),
                            inset: false,
                        }])
                })
                .when(!is_on, |this| {
                    this.text_color(idle)
                        .hover(move |this| this.text_color(hot))
                })
                .on_click(move |_, window, cx| handler(value.clone(), window, cx))
                .child(label.clone())
        }))
}

/// The compact stand-in for [`seg_control`] once the options no longer fit a
/// narrow window: a small outline button naming the current value, opening a
/// popup menu with one checkable item per option.
pub fn seg_dropdown<T: Clone + PartialEq + 'static>(
    cx: &App,
    p: &Palette,
    id: impl Into<ElementId>,
    items: &[(T, String)],
    selected: T,
    on_select: impl Fn(T, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let on_select = Rc::new(on_select);
    let current = items
        .iter()
        .find(|(value, _)| *value == selected)
        .map(|(_, label)| label.clone())
        .unwrap_or_default();
    let menu_items = items.to_vec();
    button_border(
        p,
        BtnKind::Outline,
        btn(
            cx,
            p,
            id,
            BtnKind::Outline,
            BtnSize::Small,
            format!("{current} ▾"),
        ),
    )
    .dropdown_menu(move |menu, _, _| {
        menu_items.iter().fold(menu, |menu, (value, label)| {
            let value = value.clone();
            let is_on = value == selected;
            let handler = on_select.clone();
            menu.item(
                PopupMenuItem::new(label.clone())
                    .checked(is_on)
                    .on_click(move |_, window, cx| handler(value.clone(), window, cx)),
            )
        })
    })
    .into_any_element()
}

/// A bare text toggle — the prototype's `.badge.badge-line` pill used as a
/// button (the category-detail sibling pills).
pub fn pill_button(
    p: &Palette,
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .text_size(px(10.5))
        .px(px(7.))
        .py(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(p.line)
        .bg(p.surface3)
        .text_color(p.ink2)
        .font_medium()
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |this| this.border_color(p.line_strong))
        .on_click(on_click)
        .child(text.into())
}

/// A `Switch` coloured with the app accent, matching `.switch`.
pub fn switch_row(
    p: &Palette,
    id: impl Into<ElementId>,
    checked: bool,
    on_change: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> Switch {
    Switch::new(id)
        .checked(checked)
        .color(p.accent)
        .on_change(on_change)
}

/// `.field` — a 38 px input with a `surface2` fill and a `line` border.
pub fn field(p: &Palette, state: &gpui_kit::Entity<InputState>) -> Input {
    Input::new(state)
        .appearance(true)
        .bordered(true)
        .cleanable(true)
        .h(px(38.))
        .rounded(px(9.))
        .bg(p.surface2)
        .border_color(p.line)
}

/// `.page-head` — 24 px side padding, 20 px top, a 21 px title.
pub fn page_head(
    p: &Palette,
    title: impl Into<SharedString>,
    subtitle: Option<SharedString>,
    actions: Vec<AnyElement>,
) -> Div {
    div()
        .px(px(24.))
        .pt(px(20.))
        .pb(px(16.))
        .flex()
        .items_end()
        .gap(px(16.))
        .flex_wrap()
        .child(
            div()
                .min_w_0()
                .child(div().text_size(px(21.)).font_semibold().child(title.into()))
                .when_some(subtitle, |this, subtitle| {
                    this.child(
                        div()
                            .text_size(px(12.5))
                            .text_color(p.muted)
                            .mt(px(4.))
                            .child(subtitle),
                    )
                }),
        )
        .child(
            div()
                .ml_auto()
                .flex()
                .items_center()
                .gap(px(8.))
                .children(actions),
        )
}

/// `.sec-title` — 12 px, semibold, muted. Letter-spacing is not part of
/// gpui's text style, so `.tracking` from the prototype has no equivalent.
pub fn section_label(p: &Palette, text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(12.))
        .font_semibold()
        .text_color(p.muted)
        .child(text.into())
}

/// The hero's 92 px stat tile.
pub fn stat_tile(
    p: &Palette,
    key: IconKey,
    value: impl Into<SharedString>,
    label: impl Into<SharedString>,
) -> Div {
    div()
        .w(px(92.))
        .px(px(12.))
        .py(px(12.))
        .rounded(px(12.))
        .bg(p.surface2)
        .border_1()
        .border_color(p.line)
        .text_center()
        .child(icon(key, px(16.), p.accent))
        .child(
            div()
                .text_size(px(18.))
                .font_semibold()
                .mt(px(6.))
                .child(value.into()),
        )
        .child(
            div()
                .text_size(px(10.5))
                .text_color(p.muted)
                .child(label.into()),
        )
}

/// The meta row a package card and the source cards share: glyph + text.
pub fn meta_item(p: &Palette, key: IconKey, text: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(4.))
        .whitespace_nowrap()
        .text_color(p.muted)
        .child(icon(key, px(12.), p.muted))
        .child(text.into())
}

/// `.bar` + `.bar > i` — a 6 px `surface3` track with an accent fill.
pub fn progress_bar(p: &Palette, value: f32) -> Div {
    let value = value.clamp(0., 100.);
    div()
        .h(px(6.))
        .w_full()
        .rounded(px(99.))
        .bg(p.surface3)
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(relative(value / 100.))
                .rounded(px(99.))
                .bg(p.accent),
        )
}

/// `.bar.bar-indeterminate` — the 40 % accent band the running rows show.
///
/// The prototype shimmers it with a keyframe animation; gpui has no CSS
/// animations, so the band is drawn statically. The shell advances `offset`
/// from its ticker to move it.
pub fn progress_indeterminate(p: &Palette, offset: f32) -> Div {
    let width = 40.;
    let left = (offset.clamp(0., 100.) / 100.) * (100. - width);
    div()
        .h(px(6.))
        .w_full()
        .rounded(px(99.))
        .bg(p.surface3)
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(relative(width / 100.))
                .rounded(px(99.))
                .bg(p.accent)
                .ml(relative(left / 100.)),
        )
}

/// A rounded `surface3` bar — the building block of skeleton placeholders.
pub fn skeleton_bar(p: &Palette, w: f32, h: f32) -> Div {
    div().w(px(w)).h(px(h)).rounded(px(6.)).bg(p.surface3)
}

/// A static placeholder mimicking the package card's layout (280 px wide,
/// 14 px padding) while a catalogue query is in flight.
pub fn skeleton_card(p: &Palette) -> Div {
    card(p)
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
                .child(div().size(px(44.)).rounded(px(12.)).bg(p.surface3))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(skeleton_bar(p, 140., 13.))
                        .child(skeleton_bar(p, 90., 11.)),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(skeleton_bar(p, 60., 11.))
                .child(skeleton_bar(p, 44., 11.)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .pt(px(10.))
                .border_t_1()
                .border_color(p.line)
                .child(div().w(px(72.)).h(px(29.)).rounded(px(8.)).bg(p.surface3))
                .child(div().w(px(52.)).h(px(20.)).rounded(px(99.)).bg(p.surface3)),
        )
}

/// The prototype's `stars()` — five 11 px glyphs filled up to the rounded
/// rating, followed by the value.
pub fn stars(p: &Palette, rating: f32) -> Div {
    let full = rating.round().clamp(0., 5.) as usize;
    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .children((0..5).map(move |ix| {
            if ix < full {
                icon(IconKey::Star, px(11.), p.warn).text_color(p.warn)
            } else {
                icon(IconKey::Star, px(11.), p.line_strong)
            }
        }))
        .child(
            div()
                .ml(px(4.))
                .text_size(px(11.))
                .text_color(p.muted)
                .child(format!("{rating:.1}")),
        )
}

/// The prototype's empty state: a tinted glyph chip over a title and a hint.
pub fn empty_state(
    p: &Palette,
    key: IconKey,
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
    actions: Vec<AnyElement>,
) -> Div {
    card(p)
        .p(px(48.))
        .text_center()
        .child(
            div()
                .size(px(56.))
                .rounded(px(16.))
                .bg(p.surface3)
                .border_1()
                .border_color(p.line)
                .flex()
                .items_center()
                .justify_center()
                .text_color(p.muted)
                .mx_auto()
                .child(icon(key, px(24.), p.muted)),
        )
        .child(
            div()
                .text_size(px(14.))
                .font_medium()
                .mt(px(16.))
                .child(title.into()),
        )
        .child(
            div()
                .text_size(px(12.5))
                .text_color(p.muted)
                .mt(px(6.))
                .child(hint.into()),
        )
        .when(!actions.is_empty(), |this| {
            this.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .mt(px(20.))
                    .children(actions),
            )
        })
}

/// The tinted icon chip a package card, a source card and the queue rows show.
///
/// The prototype writes `style="background:<color>1f;color:<color>"` — the
/// colour at 12 % over the surface, with the glyph at full strength.
pub fn tinted_chip(p: &Palette, key: IconKey, hex: &str, side: f32, glyph: f32) -> Div {
    let color = Palette::from_hex(hex);
    div()
        .size(px(side))
        .flex_none()
        .rounded(px(12.))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(p.line)
        .bg(Hsla { a: 0.12, ..color })
        .text_color(color)
        .child(icon(key, px(glyph), color))
}

/// The chip for a package, tinted by its category.
pub fn pkg_chip(p: &Palette, pkg: &Package, side: f32, glyph: f32) -> Div {
    tinted_chip(p, pkg.category.icon(), pkg.category.color(), side, glyph)
}

/// The package's favicon when the cache has one ([`AppState::icon_path_for`]),
/// else the category chip. gpui decodes and GPU-caches the image; the rounded
/// frame matches `pkg_chip`'s.
///
/// [`AppState::icon_path_for`]: crate::state::AppState::icon_path_for
pub fn pkg_icon(
    p: &Palette,
    pkg: &Package,
    icon: Option<std::path::PathBuf>,
    side: f32,
    glyph: f32,
) -> Div {
    match icon {
        Some(path) => div()
            .size(px(side))
            .flex_none()
            .rounded(px(12.))
            .overflow_hidden()
            .border_1()
            .border_color(p.line)
            .child(gpui_kit::img(path).size(px(side))),
        None => pkg_chip(p, pkg, side, glyph),
    }
}

/// `.cat-tile`'s icon chip: 40 px, tile-tinted, 12 px radius.
pub fn category_chip(p: &Palette, category: crate::model::CategoryId) -> Div {
    tinted_chip(p, category.icon(), category.color(), 40., 20.)
}

/// The label and glyph an install button shows for a package's state.
///
/// Mirrors `pkgRow`'s three-way choice — queued, re-install, install — which
/// the updates view overrides with 更新.
pub fn install_label(pkg: &Package, queued: bool, action: QueueAction) -> (String, IconKey) {
    if queued {
        return (tr!("queue.queued"), IconKey::Clock);
    }
    match action {
        QueueAction::Upgrade => (tr!("queue.action.upgrade"), IconKey::Download),
        // Batch upgrades are enqueued by the view-level button directly;
        // `package_action` never yields this for a per-package button.
        QueueAction::UpgradeAll => (tr!("queue.action.upgrade_all"), IconKey::Download),
        QueueAction::Install if pkg.installed => (tr!("queue.reinstall"), IconKey::Refresh),
        QueueAction::Install => (tr!("queue.action.install"), IconKey::Download),
    }
}

/// Which install button a card shows: an upgrade only when a newer version is
/// known, otherwise an install.
pub fn package_action(pkg: &Package) -> QueueAction {
    if pkg.installed && pkg.available.is_some() {
        QueueAction::Upgrade
    } else {
        QueueAction::Install
    }
}

/// The category-detail sort segments.
pub fn category_sort_items() -> Vec<(CategorySort, String)> {
    CategorySort::ALL.iter().map(|s| (*s, s.label())).collect()
}

/// The search sort segments.
pub fn sort_items() -> Vec<(SortMode, String)> {
    SortMode::ALL.iter().map(|s| (*s, s.label())).collect()
}

/// The status filter segments.
pub fn status_items() -> Vec<(StatusFilter, String)> {
    StatusFilter::ALL.iter().map(|s| (*s, s.label())).collect()
}

/// The search-source segments: the live sources, then "all sources".
///
/// The prototype renders one button per configured source; an empty list
/// leaves only 全部源.
pub fn source_items(sources: &[Source]) -> Vec<(SearchSource, String)> {
    let mut items: Vec<(SearchSource, String)> = sources
        .iter()
        .map(|source| {
            (
                SearchSource::Named(source.name.clone()),
                source.name.clone(),
            )
        })
        .collect();
    items.push((SearchSource::All, tr!("source.all")));
    items
}

/// The concurrency segments.
pub fn concurrency_items() -> Vec<(u8, String)> {
    vec![
        (1, "1".to_owned()),
        (2, "2".to_owned()),
        (3, "3".to_owned()),
    ]
}

/// The theme segments.
pub fn theme_items() -> Vec<(ModeChoice, String)> {
    ModeChoice::ALL.iter().map(|m| (*m, m.label())).collect()
}

/// The language segments. Language names stay in their own script — a user
/// hunting for their language must find it without reading another one.
pub fn language_items() -> Vec<(Language, String)> {
    Language::ALL
        .iter()
        .map(|l| (*l, l.native_name().to_owned()))
        .collect()
}

/// A horizontally scrolling page body: 24 px sides, 32 px bottom.
pub fn page_body() -> Div {
    div().px(px(24.)).pb(px(32.))
}

/// The single-line style the prototype uses for monospaced command snippets.
pub fn mono(p: &Palette) -> Div {
    div()
        .font_family("Cascadia Code")
        .text_size(px(11.5))
        .text_color(p.muted)
}

/// `.row-hover` — a list row that lifts to `surface2` under the pointer.
pub fn row_hover(p: &Palette, el: Div) -> Div {
    el.hover(move |this| this.bg(p.surface2))
}

/// The inline code chip the prototype draws around a command fragment.
pub fn code_chip(p: &Palette, text: impl Into<SharedString>) -> Div {
    div()
        .font_family("Cascadia Code")
        .text_size(px(11.5))
        .px(px(6.))
        .py(px(2.))
        .rounded(px(6.))
        .bg(p.surface2)
        .border_1()
        .border_color(p.line)
        .text_color(p.ink2)
        .child(text.into())
}

/// A `h_flex` that centres its children — the shape most toolbars want.
pub fn center_row() -> Div {
    h_flex().items_center()
}
