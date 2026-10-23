//! Pakkit — a winget front-end built on gpui-kit.

// `serde`/`serde_json`/`thiserror` are used by the library half of this crate;
// the binary target links them but never names them, which the dependency lint
// cannot see across the two targets.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(unused_crate_dependencies)]

use gpui_kit::component::TitleBar;
use gpui_kit::{App, AppContext as _, Bounds, Point, WindowBounds, WindowOptions, px, size};

use pakkit::shell::AppView;
use pakkit::theme;

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            theme::init(cx);
            let options = WindowOptions {
                window_min_size: Some(size(px(720.), px(520.))),
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(1280.), px(820.)),
                })),
                ..TitleBar::window_options()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| AppView::new(window, cx))
            })
            .expect("未能创建窗口");
        });
}
