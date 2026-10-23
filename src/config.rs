//! Settings persistence.
//!
//! The whole [`Settings`] struct is written as JSON to
//! `dirs::config_dir()/pakkit/settings.json` (`%APPDATA%\pakkit` on
//! Windows). Every operation is best-effort: a read-only profile or an exotic
//! config dir must never crash the app, so failures fall back to defaults.

use std::path::PathBuf;

use crate::state::{Language, Settings};

fn path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pakkit").join("settings.json"))
}

/// Read the persisted settings, or detect a sensible default on first run.
pub fn load() -> Settings {
    let Some(path) = path() else {
        return first_run();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return first_run();
    };
    let mut settings: Settings = serde_json::from_str(&text).unwrap_or_default();
    settings.language = validated_language(&settings.language);
    settings
}

/// Write the settings. Sync and tiny (~500 bytes), so saving on every change
/// is fine.
pub fn save(settings: &Settings) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(path, json);
    }
}

/// No config file yet: start from the OS display language so non-Chinese
/// users are not dropped into a UI they cannot read. The choice is persisted
/// on the first settings change.
fn first_run() -> Settings {
    Settings {
        language: detect_os_language(),
        ..Settings::default()
    }
}

/// Clamp a persisted tag to the shipped set; anything unknown re-detects.
fn validated_language(tag: &str) -> String {
    if Language::ALL.iter().any(|l| l.tag() == tag) {
        tag.to_owned()
    } else {
        detect_os_language()
    }
}

/// Map an OS locale (`zh-CN`, `zh-Hant-TW`, `ja-JP`, `en-US`, …) onto the
/// five shipped languages, defaulting to the project's base locale.
fn detect_os_language() -> String {
    let raw = sys_locale::get_locale().unwrap_or_default();
    let l = raw.to_lowercase();
    if l.starts_with("zh") {
        if l.contains("tw") || l.contains("hk") || l.contains("hant") {
            Language::ZhTW.tag().to_owned()
        } else {
            Language::ZhCN.tag().to_owned()
        }
    } else if l.starts_with("ja") {
        Language::Ja.tag().to_owned()
    } else if l.starts_with("ko") {
        Language::Ko.tag().to_owned()
    } else if l.starts_with("en") {
        Language::En.tag().to_owned()
    } else {
        Language::default().tag().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_tags_fall_back_to_a_shipped_language() {
        let fixed = validated_language("klingon");
        assert!(Language::ALL.iter().any(|l| l.tag() == fixed));
        assert_eq!(validated_language("zh-TW"), "zh-TW");
    }
}
