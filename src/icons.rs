//! Embedded icon set.
//!
//! The prototype's `ICONS` map in `prototype/assets/js/data.js` holds 24×24
//! inline SVGs stroked at width 1.9. They are embedded verbatim so the GUI
//! renders exactly the glyphs the prototype shows, independent of the
//! gpui-kit icon catalog. Each SVG uses `stroke="currentColor"` so the
//! element's `text_color` drives the glyph colour.

use std::sync::Arc;

use gpui_kit::component::{Icon, Sizable as _, Size};
use gpui_kit::{Hsla, Styled as _};

/// One prototype glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconKey {
    // Navigation
    Home,
    Grid,
    Search,
    Check,
    Refresh,
    Source,
    Settings,
    Queue,
    Theme,
    // Chrome
    Terminal,
    Info,
    Close,
    Plus,
    Trash,
    Download,
    Star,
    Chevron,
    Back,
    Shield,
    Bolt,
    External,
    Clock,
    File,
    Cpu,
    Filter,
    Warning,
    Copy,
    Edit,
    // Category glyphs
    Code,
    Office,
    Media,
    Wrench,
    Flask,
    Gamepad,
    Network,
    Driver,
    // Spare slot for a glyph a view needs that the prototype never drew.
    Package,
    // The Pakkit logo: an open-top box.
    Logo,
    Play,
}

impl IconKey {
    /// The `IconKey` a category tile uses.
    pub const fn for_category(category: crate::model::CategoryId) -> Self {
        category.icon()
    }

    /// Raw SVG bytes, copied from the prototype's `ICONS` map.
    pub const fn svg(self) -> &'static [u8] {
        match self {
            IconKey::Home => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="m3 10 9-7 9 7v10a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z"/></svg>"#,
            IconKey::Grid => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/></svg>"#,
            IconKey::Search => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="7"/><path d="m20 20-3.2-3.2"/></svg>"#,
            IconKey::Check => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="m4 12 5 5L20 6"/></svg>"#,
            IconKey::Refresh => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-3-6.7L21 8"/><path d="M21 3v5h-5"/></svg>"#,
            IconKey::Source => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><ellipse cx="12" cy="6" rx="8" ry="3"/><path d="M4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6"/><path d="M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3"/></svg>"#,
            IconKey::Settings => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.6 1.6 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.6 1.6 0 0 0-1.8-.3 1.6 1.6 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1A1.6 1.6 0 0 0 9 19.4a1.6 1.6 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.6 1.6 0 0 0 .3-1.8 1.6 1.6 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1A1.6 1.6 0 0 0 4.6 9a1.6 1.6 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.6 1.6 0 0 0 1.8.3H9a1.6 1.6 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.6 1.6 0 0 0 1 1.5 1.6 1.6 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.6 1.6 0 0 0-.3 1.8V9a1.6 1.6 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.6 1.6 0 0 0-1.5 1z"/></svg>"#,
            IconKey::Queue => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18M3 12h18M3 18h12"/></svg>"#,
            IconKey::Theme => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>"#,
            IconKey::Terminal => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="m5 7 5 5-5 5"/><path d="M13 17h6"/></svg>"#,
            IconKey::Info => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 8h.01"/></svg>"#,
            IconKey::Close => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M6 6l12 12M18 6 6 18"/></svg>"#,
            IconKey::Plus => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5v14M5 12h14"/></svg>"#,
            IconKey::Trash => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h16M10 11v6M14 11v6"/><path d="M6 7l1 13a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1l1-13"/><path d="M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/></svg>"#,
            IconKey::Download => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v12"/><path d="m7 10 5 5 5-5"/><path d="M5 20h14"/></svg>"#,
            IconKey::Star => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="m12 3 2.7 5.6 6.3.9-4.5 4.4 1 6.2-5.5-2.9-5.5 2.9 1-6.2L3 9.5l6.3-.9z"/></svg>"#,
            IconKey::Chevron => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="m9 6 6 6-6 6"/></svg>"#,
            IconKey::Back => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M19 12H5"/><path d="m11 18-6-6 6-6"/></svg>"#,
            IconKey::Shield => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3l8 3v6c0 5-3.5 8.4-8 9-4.5-.6-8-4-8-9V6z"/></svg>"#,
            IconKey::Bolt => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M13 2 4 14h6l-1 8 9-12h-6z"/></svg>"#,
            IconKey::External => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M14 4h6v6"/><path d="M20 4 11 13"/><path d="M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/></svg>"#,
            IconKey::Clock => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>"#,
            IconKey::File => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3h8l4 4v14a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z"/><path d="M14 3v5h5"/></svg>"#,
            IconKey::Cpu => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="6" y="6" width="12" height="12" rx="2"/><path d="M10 2v3M14 2v3M10 19v3M14 19v3M2 10h3M2 14h3M19 10h3M19 14h3"/></svg>"#,
            IconKey::Filter => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M3 5h18l-7 8v6l-4-2v-4z"/></svg>"#,
            IconKey::Warning => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/><path d="M12 9v4M12 17h.01"/></svg>"#,
            IconKey::Copy => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1"/></svg>"#,
            IconKey::Edit => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z"/></svg>"#,
            IconKey::Code => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="m8 6-6 6 6 6"/><path d="m16 6 6 6-6 6"/></svg>"#,
            IconKey::Office => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M4 4h11l5 5v11a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z"/><path d="M14 4v6h6"/></svg>"#,
            IconKey::Media => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="m10 9 6 3-6 3z"/></svg>"#,
            IconKey::Wrench => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M14.7 6.3a4 4 0 0 0 5 5l-9 9a2.8 2.8 0 0 1-4-4z"/><path d="m16 4 4 4"/></svg>"#,
            IconKey::Flask => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M9 3v7l-4.5 8A2 2 0 0 0 6.3 21h11.4a2 2 0 0 0 1.8-3L15 10V3"/><path d="M7 3h10"/><path d="M7.5 15h9"/></svg>"#,
            IconKey::Gamepad => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="7" width="20" height="11" rx="4"/><path d="M7 11v3M5.5 12.5h3"/><circle cx="16" cy="11.5" r="1"/><circle cx="18.5" cy="13.5" r="1"/></svg>"#,
            IconKey::Network => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3a15 15 0 0 1 0 18 15 15 0 0 1 0-18"/></svg>"#,
            IconKey::Driver => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="6" width="18" height="12" rx="2"/><path d="M7 10h3M7 14h6M16 10h1"/></svg>"#,
            IconKey::Package => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M21 8 12 3 3 8v8l9 5 9-5z"/><path d="m3 8 9 5 9-5"/><path d="M12 13v8"/></svg>"#,
            IconKey::Play => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="m6 4 14 8-14 8z"/></svg>"#,
            // The logo is an open variant of `Package`: same iso box, lid
            // flaps splayed outward. Mirrored in `assets/logo.svg`.
            IconKey::Logo => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M4 10l8-4-2.5-2.5-8 4z"/><path d="M12 6l8 4 2.5-2.5-8-4z"/><path d="M4 10l8 4 8-4v6l-8 4-8-4z"/><path d="M12 14v6"/></svg>"#,
        }
    }
}

/// An `Icon` element for `key`, sized and coloured.
///
/// The SVG carries `stroke="currentColor"`, so `color` reaches the glyph.
pub fn icon(key: IconKey, size: impl Into<Size>, color: Hsla) -> Icon {
    Icon::default()
        .data(key.svg())
        .with_size(size)
        .text_color(color)
}

/// Rasterizes the brand mark for the X11 `_NET_WM_ICON` property (Wayland
/// compositors resolve the taskbar icon from `pakkit.desktop` instead;
/// Windows reads the icon resource embedded by `build.rs`).
pub fn window_icon() -> Option<Arc<image::RgbaImage>> {
    brand_tile_image(256).map(Arc::new)
}

/// Rasterizes the brand tile at `icon_px` square pixels: the logo glyph in
/// white on the rounded accent tile, mirroring the in-app brand mark.
///
/// The glyph SVG strokes in `currentColor`, which has no embedding context
/// here, so the colours are baked in. Pixels are straight-alpha RGBA.
pub fn brand_tile_image(icon_px: u32) -> Option<image::RgbaImage> {
    let glyph = String::from_utf8_lossy(IconKey::Logo.svg());
    let inner = glyph
        .split_once('>')
        .map(|(_, rest)| rest.trim_end_matches("</svg>"))?;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">\
         <rect width=\"24\" height=\"24\" rx=\"5.5\" fill=\"#2563eb\"/>\
         <g fill=\"none\" stroke=\"#ffffff\" stroke-width=\"1.7\" stroke-linecap=\"round\" \
         stroke-linejoin=\"round\" transform=\"translate(4 4) scale(0.667)\">{inner}</g></svg>"
    );

    let tree =
        resvg::usvg::Tree::from_data(svg.as_bytes(), &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(icon_px, icon_px)?;
    let scale = icon_px as f32 / tree.size().width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    // tiny-skia pixels are premultiplied; `_NET_WM_ICON` wants straight alpha.
    let mut data = pixmap.take();
    for px in data.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a) as u8;
            }
        }
    }
    image::RgbaImage::from_raw(icon_px, icon_px, data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CategoryId;

    /// Every glyph the views reference must exist and be a well-formed SVG.
    #[test]
    fn every_icon_is_a_well_formed_svg() {
        let all = [
            IconKey::Home,
            IconKey::Grid,
            IconKey::Search,
            IconKey::Check,
            IconKey::Refresh,
            IconKey::Source,
            IconKey::Settings,
            IconKey::Queue,
            IconKey::Theme,
            IconKey::Terminal,
            IconKey::Info,
            IconKey::Close,
            IconKey::Plus,
            IconKey::Trash,
            IconKey::Download,
            IconKey::Star,
            IconKey::Chevron,
            IconKey::Back,
            IconKey::Shield,
            IconKey::Bolt,
            IconKey::External,
            IconKey::Clock,
            IconKey::File,
            IconKey::Cpu,
            IconKey::Filter,
            IconKey::Warning,
            IconKey::Copy,
            IconKey::Edit,
            IconKey::Logo,
            IconKey::Code,
            IconKey::Office,
            IconKey::Media,
            IconKey::Wrench,
            IconKey::Flask,
            IconKey::Gamepad,
            IconKey::Network,
            IconKey::Driver,
            IconKey::Package,
            IconKey::Play,
        ];
        for key in all {
            let svg = std::str::from_utf8(key.svg()).expect("icons are UTF-8");
            assert!(svg.starts_with("<svg"), "{key:?} is not an svg");
            assert!(svg.ends_with("</svg>"), "{key:?} is not closed");
            // `text_color` only reaches the glyph through currentColor.
            assert!(
                svg.contains("stroke=\"currentColor\""),
                "{key:?} must stroke with currentColor"
            );
            assert!(
                svg.contains("viewBox=\"0 0 24 24\""),
                "{key:?} is not 24x24"
            );
        }
    }

    /// The window icon rasterizes to a tile: transparent rounded corners,
    /// brand-blue body.
    #[test]
    fn window_icon_is_the_brand_tile() {
        let icon = window_icon().expect("logo rasterizes");
        assert_eq!((icon.width(), icon.height()), (256, 256));
        assert_eq!(icon.get_pixel(0, 0).0[3], 0, "corner must be transparent");
        // (128, 240) lands on the tile below the glyph: brand blue, opaque.
        assert_eq!(icon.get_pixel(128, 240).0, [0x25, 0x63, 0xeb, 0xff]);
    }

    #[test]
    fn category_glyphs_are_all_distinct() {
        let mut seen = Vec::new();
        for category in CategoryId::ALL {
            let bytes = category.icon().svg();
            assert!(
                !seen.contains(&bytes),
                "{} reuses a glyph",
                category.label()
            );
            seen.push(bytes);
        }
    }
}
