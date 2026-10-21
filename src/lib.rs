//! WingetGUI — a winget client for Windows built on gpui-kit.

// The app's translations, embedded from `locales/` at compile time. `zh-CN` is
// the base locale (the UI was written in it); `en` is the second net. Every
// `t!` lookup happens at call time against the process-wide current locale,
// so `rust_i18n::set_locale` plus a re-render switches the UI language live.
rust_i18n::i18n!("locales", fallback = ["zh-CN", "en"]);

/// `t!` returns `Cow<'_, str>`; UI code overwhelmingly wants an owned
/// `String`, so every call site goes through this wrapper instead.
#[macro_export]
macro_rules! tr {
    ($($args:tt)*) => {
        ::rust_i18n::t!($($args)*).into_owned()
    };
}

pub mod cache;
pub mod config;
pub mod dialogs;
pub mod icons;
pub mod model;
pub mod queue;
pub mod shell;
pub mod state;
pub mod theme;
pub mod views;
pub mod widgets;
pub mod winget;

/// Pin the process-wide locale to the base language for tests that assert
/// translated text. `rust-i18n` defaults to `"en"` when `set_locale` was
/// never called; nothing in the test suite sets another locale, so a one-time
/// pin is race-free (every caller blocks in `call_once` before asserting).
#[cfg(test)]
pub(crate) fn ensure_zh_locale() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| rust_i18n::set_locale("zh-CN"));
}

#[cfg(test)]
mod i18n_tests {
    use rust_i18n::t;
    // Dev-only dependency used by tests/locale_parity.rs; referenced here so
    // the lib test target does not trip `unused-crate-dependencies`.
    use serde_yaml as _;

    #[test]
    fn every_locale_ships_the_base_namespaces() {
        for locale in ["zh-CN", "zh-TW", "en", "ja", "ko"] {
            assert!(
                !t!("nav.settings", locale = locale).is_empty(),
                "{locale} is missing nav.settings"
            );
            assert_ne!(
                t!("nav.settings", locale = locale),
                "nav.settings",
                "{locale} falls back to the key itself"
            );
        }
    }

    #[test]
    fn interpolation_substitutes_placeholders() {
        assert_eq!(t!("status.ready", locale = "zh-CN"), "就绪");
        let text = t!("status.searching", locale = "zh-CN", query = "vscode");
        assert_eq!(text, "搜索：vscode");
    }
}
