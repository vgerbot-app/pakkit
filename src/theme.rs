//! The prototype's palette, mapped onto gpui-kit's theme.
//!
//! gpui-kit's `ThemeColor` carries ~140 semantic tokens, but the prototype's
//! design system names its own (bg / surface / line / ink / accent…). Rather
//! than approximate, the prototype palette is defined explicitly as [`Palette`]
//! and then projected onto `ThemeColor` so built-in components inherit the
//! right colours.

use std::{rc::Rc, sync::LazyLock};

use gpui_kit::component::{Theme, ThemeConfig, ThemeMode, ThemeSet};
use gpui_kit::{App, Global, Hsla, px};
use serde::{Deserialize, Serialize};

/// `#rrggbb` → `Hsla`, the way gpui's `rgba` accepts `0xRRGGBBAA`.
const fn h(hex: u32) -> Hsla {
    // `Hsla`'s fields are public; the conversion matches gpui's own
    // `From<Rgba> for Hsla` for an opaque colour.
    let r = ((hex >> 16) & 0xff) as f32 / 255.;
    let g = ((hex >> 8) & 0xff) as f32 / 255.;
    let b = (hex & 0xff) as f32 / 255.;

    let max = if r > g {
        if r > b { r } else { b }
    } else if g > b {
        g
    } else {
        b
    };
    let min = if r < g {
        if r < b { r } else { b }
    } else if g < b {
        g
    } else {
        b
    };
    let l = (max + min) / 2.;
    let d = max - min;

    let s = if d == 0. {
        0.
    } else if l > 0.5 {
        d / (2. - max - min)
    } else {
        d / (max + min)
    };

    let raw = if d == 0. {
        0.
    } else if max == r {
        (g - b) / d
    } else if max == g {
        (b - r) / d + 2.
    } else {
        (r - g) / d + 4.
    };
    // Wrap into `0..6` without `rem_euclid`, which is not a `const fn`.
    let wrapped = if raw < 0. { raw + 6. } else { raw };

    Hsla {
        h: wrapped / 6.,
        s,
        l,
        a: 1.,
    }
}

/// The prototype's design tokens for one mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub bg: Hsla,
    pub surface: Hsla,
    pub surface2: Hsla,
    pub surface3: Hsla,
    pub line: Hsla,
    pub line_strong: Hsla,
    pub ink: Hsla,
    pub ink2: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub accent2: Hsla,
    pub on_accent: Hsla,
    pub ok: Hsla,
    pub warn: Hsla,
    pub danger: Hsla,
}

impl Palette {
    /// The `:root` column of `app.css`.
    pub const fn light() -> Self {
        Self {
            bg: h(0xf4f5f7),
            surface: h(0xffffff),
            surface2: h(0xf7f8fa),
            surface3: h(0xeceef2),
            line: h(0xe3e6eb),
            line_strong: h(0xcfd4dc),
            ink: h(0x14171c),
            ink2: h(0x40474f),
            muted: h(0x8b939d),
            accent: h(0x2563eb),
            accent2: h(0x7c3aed),
            on_accent: h(0xffffff),
            ok: h(0x16a34a),
            warn: h(0xd97706),
            danger: h(0xdc2626),
        }
    }

    /// The `.dark` column of `app.css`.
    pub const fn dark() -> Self {
        Self {
            bg: h(0x0e1014),
            surface: h(0x16191f),
            surface2: h(0x1c2028),
            surface3: h(0x252a34),
            line: h(0x272c36),
            line_strong: h(0x3a4150),
            ink: h(0xeef1f6),
            ink2: h(0xb7bec9),
            muted: h(0x767e8c),
            accent: h(0x4f8cff),
            accent2: h(0xa06bff),
            on_accent: h(0x08111f),
            ok: h(0x34d399),
            warn: h(0xfbbf24),
            danger: h(0xf87171),
        }
    }

    /// The palette matching the current gpui-kit theme mode.
    pub fn of(cx: &App) -> Self {
        if Theme::global(cx).mode.is_dark() {
            Self::dark()
        } else {
            Self::light()
        }
    }

    /// `accent` at `alpha`, matching Tailwind's `color-mix(… N%, transparent)`.
    pub fn tint(&self, color: Hsla, alpha: f32) -> Hsla {
        Hsla { a: alpha, ..color }
    }

    /// A category tile colour from [`crate::model::CategoryId::color`].
    pub fn from_hex(hex: &str) -> Hsla {
        let value = hex.trim_start_matches('#');
        let parsed = u32::from_str_radix(value, 16).unwrap_or(0);
        h(parsed)
    }

    /// Replace `accent` with a user-chosen colour.
    pub fn with_accent(mut self, hex: &str) -> Self {
        self.accent = Self::from_hex(hex);
        self
    }
}

impl Global for Palette {}

/// The installed palette.
pub fn palette(cx: &App) -> Palette {
    *cx.global::<Palette>()
}

/// Install the global and switch gpui-kit to the prototype's default (dark).
///
/// The bundled `themes/pakkit.json` is applied first so that the tokens
/// [`apply`] does not name (the built-in components' own colours) still follow
/// the prototype instead of gpui-kit's default dark theme.
pub fn init(cx: &mut App) {
    // `Theme::update` needs the global, which `change` installs.
    Theme::change(ThemeMode::Dark, None, cx);
    install_bundled_theme(cx);
    cx.set_global(Palette::dark());
    apply(cx);
}

/// The parsed `themes/pakkit.json`, baked in so the app themes itself
/// regardless of the working directory it was launched from.
fn bundled_theme() -> Option<&'static ThemeConfig> {
    static CONFIG: LazyLock<Option<ThemeConfig>> = LazyLock::new(|| {
        let set: ThemeSet = serde_json::from_str(include_str!("../themes/pakkit.json")).ok()?;
        set.themes.into_iter().find(|theme| theme.mode.is_dark())
    });
    CONFIG.as_ref()
}

/// Load the bundled theme set into the registry and make its dark theme current.
fn install_bundled_theme(cx: &mut App) {
    let Some(config) = bundled_theme() else {
        // A missing or malformed file is not fatal: `apply` still carries the
        // prototype palette the widgets read directly.
        return;
    };
    let config = Rc::new(config.clone());
    Theme::update(cx, |theme| theme.apply_config(&config));
}

/// Project the stored [`Palette`] onto `ThemeColor` and the font/radius tokens.
pub fn apply(cx: &mut App) {
    let p = palette(cx);
    Theme::update(cx, move |theme| {
        theme.colors.background = p.bg;
        theme.colors.foreground = p.ink;
        theme.colors.border = p.line;
        theme.colors.muted = p.surface3;
        theme.colors.muted_foreground = p.muted;
        theme.colors.popover = p.surface;
        theme.colors.popover_foreground = p.ink;
        theme.colors.secondary = p.surface2;
        theme.colors.secondary_foreground = p.ink2;
        theme.colors.secondary_hover = p.surface3;
        theme.colors.input = p.surface2;
        theme.colors.primary = p.accent;
        theme.colors.primary_foreground = p.on_accent;
        theme.colors.primary_hover = p.accent;
        theme.colors.ring = p.accent;
        theme.colors.accent = p.accent;
        theme.colors.accent_foreground = p.on_accent;
        theme.colors.success = p.ok;
        theme.colors.success_foreground = p.on_accent;
        theme.colors.warning = p.warn;
        theme.colors.warning_foreground = p.on_accent;
        theme.colors.danger = p.danger;
        theme.colors.danger_foreground = p.on_accent;
        theme.colors.title_bar = p.surface;
        theme.colors.title_bar_border = p.line;
        theme.colors.status_bar = p.surface;
        theme.colors.status_bar_border = p.line;
        theme.colors.sidebar = p.surface;
        theme.colors.sidebar_border = p.line;
        theme.colors.sidebar_foreground = p.ink2;
        theme.colors.sidebar_accent = p.tint(p.accent, 0.14);
        theme.colors.sidebar_accent_foreground = p.accent;
        theme.colors.list = p.surface;
        theme.colors.list_hover = p.surface2;
        theme.colors.list_active = p.tint(p.accent, 0.14);
        theme.colors.list_active_border = p.accent;
        theme.colors.progress_bar = p.accent;
        theme.colors.skeleton = p.surface3;
        theme.colors.scrollbar = Hsla {
            a: 0.,
            ..p.line_strong
        };
        theme.colors.scrollbar_thumb = p.line_strong;
        theme.colors.scrollbar_thumb_hover = p.muted;
        theme.colors.switch = p.tint(p.accent, 0.85);
        theme.colors.switch_thumb = h(0xffffff);
        // `app.css`'s backdrop: rgba(4,6,10,.5).
        theme.colors.overlay = Hsla {
            a: 0.5,
            ..h(0x04_06_0a)
        };

        theme.radius = px(9.);
        theme.radius_lg = px(14.);
        theme.font_family = "Segoe UI Variable Text".into();
        theme.mono_font_family = "Cascadia Code".into();
        theme.font_size = px(13.);
        theme.mono_font_size = px(12.);
    });
}

/// Switch light/dark and keep every copy of the theme in step.
///
/// `System` resolves the platform appearance and stores the resolved mode, so
/// the palette and the sidebar label agree on what is shown.
pub fn set_mode(cx: &mut App, mode: ModeChoice) {
    match mode {
        ModeChoice::Dark => Theme::change(ThemeMode::Dark, None, cx),
        ModeChoice::Light => Theme::change(ThemeMode::Light, None, cx),
        ModeChoice::System => Theme::sync_system_appearance(None, cx),
    }
    cx.set_global(Palette::of(cx));
    apply(cx);
}

/// Replace the accent colour and re-project it.
pub fn set_accent(cx: &mut App, hex: &str) {
    let mut p = palette(cx);
    p = p.with_accent(hex);
    cx.set_global(p);
    apply(cx);
}

/// The three-way theme selector from the prototype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ModeChoice {
    #[default]
    Dark,
    Light,
    System,
}

impl ModeChoice {
    pub const ALL: [ModeChoice; 3] = [ModeChoice::Dark, ModeChoice::Light, ModeChoice::System];

    pub fn label(self) -> String {
        match self {
            ModeChoice::Dark => tr!("theme.dark"),
            ModeChoice::Light => tr!("theme.light"),
            ModeChoice::System => tr!("theme.system"),
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            ModeChoice::Dark => "dark",
            ModeChoice::Light => "light",
            ModeChoice::System => "system",
        }
    }

    /// The next entry in the sidebar's cycle order.
    pub const fn next(self) -> Self {
        match self {
            ModeChoice::Dark => ModeChoice::Light,
            ModeChoice::Light => ModeChoice::System,
            ModeChoice::System => ModeChoice::Dark,
        }
    }
}

/// The five accent swatches the settings screen offers.
pub const ACCENT_SWATCHES: [&str; 5] = ["#4f8cff", "#a06bff", "#16a34a", "#e11d48", "#d97706"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_conversion_matches_the_prototype_tokens() {
        // Spot-check against the values written in `app.css`.
        let p = Palette::dark();
        let expected = Palette::from_hex("#0e1014");
        assert_eq!(p.bg, expected);

        let white = Palette::from_hex("#ffffff");
        assert_eq!(white.l, 1.0);
        assert_eq!(white.s, 0.0);
        assert_eq!(white.a, 1.0);

        let black = Palette::from_hex("#000000");
        assert_eq!(black.l, 0.0);
    }

    #[test]
    fn hex_parsing_tolerates_a_missing_hash() {
        assert_eq!(Palette::from_hex("4f8cff"), Palette::from_hex("#4f8cff"));
    }

    #[test]
    fn light_and_dark_differ_on_every_token() {
        let (l, d) = (Palette::light(), Palette::dark());
        for (name, a, b) in [
            ("bg", l.bg, d.bg),
            ("surface", l.surface, d.surface),
            ("surface2", l.surface2, d.surface2),
            ("surface3", l.surface3, d.surface3),
            ("line", l.line, d.line),
            ("line_strong", l.line_strong, d.line_strong),
            ("ink", l.ink, d.ink),
            ("ink2", l.ink2, d.ink2),
            ("muted", l.muted, d.muted),
            ("accent", l.accent, d.accent),
            ("accent2", l.accent2, d.accent2),
            ("on_accent", l.on_accent, d.on_accent),
            ("ok", l.ok, d.ok),
            ("warn", l.warn, d.warn),
            ("danger", l.danger, d.danger),
        ] {
            assert_ne!(a, b, "{name} must differ between light and dark");
        }
    }

    #[test]
    fn mode_cycle_matches_the_prototype_order() {
        assert_eq!(ModeChoice::Dark.next(), ModeChoice::Light);
        assert_eq!(ModeChoice::Light.next(), ModeChoice::System);
        assert_eq!(ModeChoice::System.next(), ModeChoice::Dark);
    }

    #[test]
    fn accent_override_keeps_the_rest_of_the_palette() {
        let base = Palette::dark();
        let changed = base.with_accent("#16a34a");
        assert_eq!(changed.accent, Palette::from_hex("#16a34a"));
        assert_eq!(changed.bg, base.bg);
        assert_eq!(changed.ink, base.ink);
    }

    #[test]
    fn tint_sets_alpha_without_touching_hue() {
        let p = Palette::dark();
        let t = p.tint(p.accent, 0.14);
        assert_eq!(t.h, p.accent.h);
        assert_eq!(t.s, p.accent.s);
        assert_eq!(t.l, p.accent.l);
        assert_eq!(t.a, 0.14);
    }

    #[test]
    fn swatches_are_parseable_colours() {
        for swatch in ACCENT_SWATCHES {
            assert_eq!(swatch.len(), 7);
            assert!(swatch.starts_with('#'));
            let value = u32::from_str_radix(&swatch[1..], 16).expect("swatch is hex");
            assert!(value <= 0xffffff);
        }
    }

    #[test]
    fn the_bundled_theme_tracks_the_palette() {
        // The file is embedded, so a typo or a drift from `Palette` would
        // silently leave the built-in components on gpui-kit's defaults.
        let theme = bundled_theme().expect("themes/pakkit.json parses");
        assert_eq!(theme.colors.background.as_deref(), Some("#0e1014"));
        assert_eq!(theme.colors.accent.as_deref(), Some("#4f8cff"));
        assert_eq!(theme.radius, Some(9));
        assert_eq!(theme.font_family.as_deref(), Some("Segoe UI Variable Text"));
    }
}
