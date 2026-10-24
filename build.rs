//! Embeds the application icon into the Windows exe; a no-op elsewhere.
//!
//! gpui's Windows backend loads the window/taskbar icon from the exe's icon
//! resource ID 1 (`LoadImageW(module, 1, IMAGE_ICON, ..)`), which `pakkit.rc`
//! provides. Regenerate `assets/pakkit.ico` with `cargo run --example
//! make_icon` after changing the logo.

fn main() {
    println!("cargo::rerun-if-changed=assets/pakkit.ico");
    println!("cargo::rerun-if-changed=pakkit.rc");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("pakkit.rc", embed_resource::NONE)
            .manifest_required()
            .expect("embedding the icon resource must succeed for Windows builds");
    }
}
