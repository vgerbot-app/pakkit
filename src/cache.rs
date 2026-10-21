//! On-disk caches under `dirs::cache_dir()/wingetgui/`.
//!
//! Two kinds of data live here:
//!
//! - `details/<id>.json` — parsed `winget show` output, so a reopened detail
//!   dialog and the favicon resolver survive restarts without re-running
//!   winget. Expires after [`DETAIL_TTL`].
//! - `icons/<host>.img` — a site's favicon, fetched because a cached detail
//!   pointed at that host. Keyed by host so every package from one publisher
//!   shares a file. Expires after [`ICON_TTL`]. A fetch that failed leaves an
//!   empty `icons/<host>.none` marker instead, which suppresses retries for
//!   [`NONE_TTL`].

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::winget::PackageDetail;

/// How long a cached `winget show` result is trusted.
pub const DETAIL_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
/// How long a fetched favicon is trusted.
pub const ICON_TTL: Duration = Duration::from_secs(30 * 24 * 3600);
/// How long a failed favicon fetch suppresses retries.
pub const NONE_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

/// The cache root, creating the directory tree on first use.
pub fn root() -> Option<PathBuf> {
    let root = dirs::cache_dir()?.join("wingetgui");
    std::fs::create_dir_all(root.join("details")).ok()?;
    std::fs::create_dir_all(root.join("icons")).ok()?;
    Some(root)
}

/// `details/<id-lowercase>.json` for one package.
pub fn detail_path(id: &str) -> Option<PathBuf> {
    Some(
        root()?
            .join("details")
            .join(format!("{}.json", id.to_lowercase())),
    )
}

/// `icons/<host>.img` — the favicon bytes gpui renders directly.
pub fn icon_path(host: &str) -> Option<PathBuf> {
    Some(root()?.join("icons").join(format!("{host}.img")))
}

/// `icons/<host>.none` — the failed-fetch marker.
pub fn none_marker_path(host: &str) -> Option<PathBuf> {
    Some(root()?.join("icons").join(format!("{host}.none")))
}

/// The host a package's favicon comes from: its homepage when known, else the
/// publisher URL. Bare hosts without a scheme get `https://` prepended.
/// localhost and bare IPs are rejected — they never carry a useful favicon.
pub fn host_of(detail: &PackageDetail) -> Option<String> {
    let candidate = [&detail.homepage, &detail.publisher_url]
        .into_iter()
        .map(String::as_str)
        .map(str::trim)
        .find(|s| !s.is_empty())?;
    let with_scheme = if candidate.contains("://") {
        candidate.to_owned()
    } else {
        format!("https://{candidate}")
    };
    let url = url::Url::parse(&with_scheme).ok()?;
    let host = url.host_str()?.to_owned();
    if host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    Some(host)
}

/// Whether `path` exists and is younger than `ttl`.
pub fn fresh(path: &Path, ttl: Duration) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let Ok(mtime) = meta.modified() else {
        return false;
    };
    mtime.elapsed().map(|age| age < ttl).unwrap_or(false)
}

/// Write `bytes` to `path` atomically (sibling temp file + rename), so a
/// crash mid-write never leaves a torn cache entry.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// All unexpired cached details. Expired and unparseable files are removed.
pub fn load_details() -> Vec<PackageDetail> {
    let Some(dir) = root().map(|r| r.join("details")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        if !fresh(&path, DETAIL_TTL) {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        match serde_json::from_str::<PackageDetail>(&text) {
            Ok(detail) => out.push(detail),
            Err(_) => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    out
}

/// Cached favicons as `(host, state)` pairs: fresh `.img` files load as
/// `Ok(path)`, fresh `.none` markers as `Err(())` (the negative cache).
/// Expired entries are removed and simply absent.
pub fn load_icons() -> Vec<(String, Result<PathBuf, ()>)> {
    let Some(dir) = root().map(|r| r.join("icons")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
            continue;
        };
        match path.extension().and_then(|e| e.to_str()) {
            Some("img") => {
                if fresh(&path, ICON_TTL) {
                    out.push((stem, Ok(path)));
                } else {
                    let _ = std::fs::remove_file(&path);
                }
            }
            Some("none") => {
                if fresh(&path, NONE_TTL) {
                    out.push((stem, Err(())));
                } else {
                    let _ = std::fs::remove_file(&path);
                }
            }
            _ => {}
        }
    }
    out
}

/// Fetch a site's favicon: the site's own `/favicon.ico` first, Google's
/// favicon service as a fallback. Returns the image bytes when a response
/// looks like a real icon (magic-byte checked — an HTML 404 page or an empty
/// body must never be cached as an image).
pub fn fetch_favicon(host: &str) -> Option<Vec<u8>> {
    let own = format!("https://{host}/favicon.ico");
    let fallback = format!("https://www.google.com/s2/favicons?domain={host}&sz=64");
    [own, fallback]
        .into_iter()
        .find_map(|url| fetch_image(&url))
}

/// GET `url` with a short timeout and a size cap; return the body when the
/// status is 2xx and the bytes start with a known image signature.
fn fetch_image(url: &str) -> Option<Vec<u8>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .into();
    let mut response = agent.get(url).call().ok()?;
    if response.status() != ureq::http::StatusCode::OK {
        return None;
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_vec()
        .ok()?;
    looks_like_image(&bytes).then_some(bytes)
}

/// Cheap signature check: ICO, PNG, JPEG, GIF or WebP.
fn looks_like_image(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x00, 0x00, 0x01, 0x00])
        || bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47])
        || bytes.starts_with(&[0xFF, 0xD8])
        || bytes.starts_with(b"GIF8")
        || (bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail(homepage: &str, publisher_url: &str) -> PackageDetail {
        PackageDetail {
            homepage: homepage.to_owned(),
            publisher_url: publisher_url.to_owned(),
            ..PackageDetail::default()
        }
    }

    #[test]
    fn host_prefers_homepage() {
        let d = detail(
            "https://code.visualstudio.com/docs",
            "https://microsoft.com",
        );
        assert_eq!(host_of(&d).as_deref(), Some("code.visualstudio.com"));
    }

    #[test]
    fn host_falls_back_to_publisher_url() {
        let d = detail("", "https://www.7-zip.org/");
        assert_eq!(host_of(&d).as_deref(), Some("www.7-zip.org"));
    }

    #[test]
    fn host_adds_missing_scheme() {
        let d = detail("example.com/app", "");
        assert_eq!(host_of(&d).as_deref(), Some("example.com"));
    }

    #[test]
    fn host_rejects_localhost_and_ips() {
        assert_eq!(host_of(&detail("http://localhost:8080", "")), None);
        assert_eq!(host_of(&detail("http://127.0.0.1/x", "")), None);
        assert_eq!(host_of(&detail("", "")), None);
    }

    #[test]
    fn image_signatures() {
        assert!(looks_like_image(&[0x00, 0x00, 0x01, 0x00, 0x01]));
        assert!(looks_like_image(b"\x89PNG\r\n\x1a\nrest"));
        assert!(looks_like_image(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(looks_like_image(b"GIF89a...."));
        assert!(looks_like_image(b"RIFF\x00\x00\x00\x00WEBP...."));
        assert!(!looks_like_image(b"<html>404</html>"));
        assert!(!looks_like_image(&[]));
    }

    #[test]
    fn atomic_write_leaves_target() {
        let dir = std::env::temp_dir().join(format!("wingetgui-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("entry.bin");
        atomic_write(&path, b"abc").expect("write succeeds");
        assert_eq!(std::fs::read(&path).expect("read back"), b"abc");
        assert!(fresh(&path, Duration::from_secs(60)));
        assert!(!fresh(&path, Duration::ZERO));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
