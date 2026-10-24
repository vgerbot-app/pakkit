//! Regenerates `assets/pakkit.ico` from the brand tile.
//!
//! Run after changing the logo: `cargo run --example make_icon`.
//! `build.rs` embeds the result into the Windows exe as icon resource ID 1.

// Examples link the package's deps but only name a few of them.
#![allow(unused_crate_dependencies)]

use std::io::Write as _;

use pakkit::icons::brand_tile_image;

/// The frame sizes Windows picks from for the taskbar, Alt-Tab and Explorer.
const SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

fn main() -> std::io::Result<()> {
    let mut frames = Vec::new();
    for size in SIZES {
        let image = brand_tile_image(size).expect("brand tile rasterizes");
        let mut png = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("PNG encoding succeeds");
        frames.push((size, png.into_inner()));
    }

    // ICO = ICONDIR header, one ICONDIRENTRY per frame, then the PNG payloads
    // (PNG-compressed frames are supported since Vista).
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]); // reserved, type = icon
    ico.extend_from_slice(&(frames.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * frames.len() as u32;
    for (size, png) in &frames {
        let dim = u8::try_from(*size).unwrap_or(0); // 0 encodes 256
        ico.extend_from_slice(&[dim, dim, 0, 0]); // width, height, palette, reserved
        ico.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in &frames {
        ico.extend_from_slice(png);
    }

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pakkit.ico");
    std::fs::File::create(path)?.write_all(&ico)?;
    println!("wrote {path}: {} bytes, {} frames", ico.len(), frames.len());
    Ok(())
}
