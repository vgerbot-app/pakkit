//! Typed winget command builders.
//!
//! Every builder returns the argument list only, so the mapping from UI state
//! to command line is unit-testable without spawning anything. Argument
//! exclusivity rules (e.g. `--purge` XOR `--preserve`) are enforced here rather
//! than trusted to callers.

pub mod exec;
pub mod exit;
pub mod parse;

use super::types::{InstallOptions, SOURCE_ALL, SearchFilters};

pub use exec::Winget;
pub use exit::{describe, format_code, is_benign, needs_elevation};
pub use parse::{
    SourceExport, parse_export_json, parse_list, parse_search, parse_show, parse_source_export,
    parse_source_list,
};

/// `--count` accepts 1..=1000; anything else is a hard winget error.
pub const COUNT_MAX: u32 = 1000;

fn clamp_count(count: u32) -> u32 {
    count.clamp(1, COUNT_MAX)
}

/// `winget search`
pub fn search(
    query: &str,
    source: Option<&str>,
    filters: SearchFilters,
    count: Option<u32>,
) -> Vec<String> {
    let mut args = vec!["search".to_owned()];
    if !query.is_empty() {
        args.push("-q".into());
        args.push(query.to_owned());
    }
    for (enabled, flag) in [
        (filters.exact, "--exact"),
        (filters.moniker, "--moniker"),
        (filters.tag, "--tag"),
        (filters.command, "--command"),
    ] {
        if enabled {
            args.push(flag.into());
        }
    }
    if let Some(source) = normalize_source(source) {
        args.push("--source".into());
        args.push(source.to_owned());
    }
    if let Some(count) = count {
        args.push("--count".into());
        args.push(clamp_count(count).to_string());
    }
    args
}

/// `winget list`
pub fn list(source: Option<&str>) -> Vec<String> {
    let mut args = vec!["list".to_owned()];
    if let Some(source) = source.filter(|s| !s.is_empty()) {
        args.push("--source".into());
        args.push(source.to_owned());
    }
    args
}

/// `winget list --upgrade-available`
///
/// `--include-unknown` and `--include-pinned` are only valid alongside
/// `--upgrade-available`, so they are emitted from here and nowhere else.
pub fn list_upgrade_available(include_unknown: bool, include_pinned: bool) -> Vec<String> {
    let mut args = vec!["list".to_owned(), "--upgrade-available".into()];
    if include_unknown {
        args.push("--include-unknown".into());
    }
    if include_pinned {
        args.push("--include-pinned".into());
    }
    args
}

/// `winget show`
pub fn show(id: &str, source: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "show".to_owned(),
        "--id".into(),
        id.to_owned(),
        "--exact".into(),
    ];
    if let Some(source) = normalize_source(source) {
        args.push("--source".into());
        args.push(source.to_owned());
    }
    args
}

/// `winget install`
pub fn install(id: &str, options: &InstallOptions) -> Vec<String> {
    let mut args = vec![
        "install".to_owned(),
        "--id".into(),
        id.to_owned(),
        "--exact".into(),
    ];
    if let Some(source) = normalize_source(options.source.as_deref()) {
        args.push("--source".into());
        args.push(source.to_owned());
    }
    args.push("--architecture".into());
    args.push(options.arch.as_str().into());
    args.push("--scope".into());
    args.push(options.scope.as_str().into());
    if let Some(location) = options.location.as_deref()
        && !location.trim().is_empty()
    {
        args.push("--location".into());
        args.push(location.trim().to_owned());
    }
    if options.silent {
        args.push("--silent".into());
    }
    // `--accept-source-agreements` is a common flag; only the package
    // agreement here depends on the user's consent toggle.
    if options.accept {
        args.push("--accept-package-agreements".into());
    }
    args
}

/// `winget uninstall`
///
/// Never emits `--purge` alongside `--preserve`, nor `--all-versions` with
/// `--version`: the prototype's uninstall acts on the installed version, so
/// neither purge flag is passed.
pub fn uninstall(id: &str) -> Vec<String> {
    vec![
        "uninstall".to_owned(),
        "--id".into(),
        id.to_owned(),
        "--exact".into(),
    ]
}

/// `winget upgrade --id <id>`
pub fn upgrade_one(id: &str) -> Vec<String> {
    vec![
        "upgrade".to_owned(),
        "--id".into(),
        id.to_owned(),
        "--exact".into(),
    ]
}

/// `winget upgrade --all`
pub fn upgrade_all() -> Vec<String> {
    vec!["upgrade".to_owned(), "--all".into()]
}

/// `winget export -o <path>`
pub fn export_json(path: &std::path::Path, source: Option<&str>) -> Vec<String> {
    let mut args = vec!["export".to_owned(), "-o".into(), path.display().to_string()];
    if let Some(source) = normalize_source(source) {
        args.push("--source".into());
        args.push(source.to_owned());
    }
    args
}

/// `winget source list`
pub fn source_list() -> Vec<String> {
    vec!["source".to_owned(), "list".into()]
}

/// `winget source export [--name <n>]`
pub fn source_export(name: Option<&str>) -> Vec<String> {
    let mut args = vec!["source".to_owned(), "export".into()];
    if let Some(name) = name.filter(|n| !n.is_empty()) {
        args.push("--name".into());
        args.push(name.to_owned());
    }
    args
}

/// `winget source add --name <n> --arg <url> [--type <t>] [--trust-level trusted]`
///
/// Requires an elevated shell; failures surface [`exit::needs_elevation`].
pub fn source_add(name: &str, url: &str, ty: &str, trusted: bool) -> Vec<String> {
    let mut args = vec![
        "source".to_owned(),
        "add".into(),
        "--name".into(),
        name.to_owned(),
        "--arg".into(),
        url.to_owned(),
    ];
    if !ty.is_empty() {
        args.push("--type".into());
        args.push(ty.to_owned());
    }
    args.push("--trust-level".into());
    args.push(if trusted { "trusted" } else { "none" }.into());
    args
}

/// `winget source edit --name <n> [--explicit true|false]`
pub fn source_edit(name: &str, explicit: Option<bool>) -> Vec<String> {
    let mut args = vec![
        "source".to_owned(),
        "edit".into(),
        "--name".into(),
        name.to_owned(),
    ];
    if let Some(explicit) = explicit {
        args.push("--explicit".into());
        args.push(if explicit { "true" } else { "false" }.into());
    }
    args
}

/// `winget source remove --name <n>` (requires elevation)
pub fn source_remove(name: &str) -> Vec<String> {
    vec![
        "source".to_owned(),
        "remove".into(),
        "--name".into(),
        name.to_owned(),
    ]
}

/// `winget source reset [--force]` (requires elevation)
pub fn source_reset(force: bool) -> Vec<String> {
    let mut args = vec!["source".to_owned(), "reset".into()];
    if force {
        args.push("--force".into());
    }
    args
}

/// `winget source update [--name <n>]`
pub fn source_update(name: Option<&str>) -> Vec<String> {
    let mut args = vec!["source".to_owned(), "update".to_owned()];
    if let Some(name) = name.filter(|n| !n.is_empty()) {
        args.push("--name".into());
        args.push(name.to_owned());
    }
    args
}

/// Map the UI's "all sources" sentinel to "no `--source` flag".
fn normalize_source(source: Option<&str>) -> Option<&str> {
    source.filter(|s| !s.is_empty() && *s != SOURCE_ALL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::types::{Arch, Scope};

    #[test]
    fn count_is_clamped_to_the_documented_range() {
        let args = search("vscode", None, SearchFilters::default(), Some(5000));
        let index = args
            .iter()
            .position(|a| a == "--count")
            .expect("--count emitted");
        assert_eq!(args[index + 1], "1000");

        let args = search("vscode", None, SearchFilters::default(), Some(0));
        let index = args
            .iter()
            .position(|a| a == "--count")
            .expect("--count emitted");
        assert_eq!(args[index + 1], "1");
    }

    #[test]
    fn all_sources_means_no_source_flag() {
        let args = search("vscode", Some(SOURCE_ALL), SearchFilters::default(), None);
        assert!(!args.iter().any(|a| a == "--source"));

        let args = search("vscode", Some("winget"), SearchFilters::default(), None);
        let index = args
            .iter()
            .position(|a| a == "--source")
            .expect("--source emitted");
        assert_eq!(args[index + 1], "winget");
    }

    #[test]
    fn include_flags_never_appear_without_upgrade_available() {
        let plain = list(None);
        assert!(!plain.iter().any(|a| a == "--include-unknown"));
        assert!(!plain.iter().any(|a| a == "--include-pinned"));

        let flagged = list_upgrade_available(false, true);
        assert!(flagged.contains(&"--upgrade-available".to_owned()));
        assert!(flagged.contains(&"--include-pinned".to_owned()));
        assert!(!flagged.contains(&"--include-unknown".to_owned()));

        let both = list_upgrade_available(true, true);
        assert!(both.contains(&"--upgrade-available".to_owned()));
        assert!(both.contains(&"--include-unknown".to_owned()));
        assert!(both.contains(&"--include-pinned".to_owned()));
    }

    #[test]
    fn uninstall_never_mixes_purge_and_preserve() {
        let args = uninstall("Git.Git");
        assert!(!args.contains(&"--purge".to_owned()));
        assert!(!args.contains(&"--preserve".to_owned()));
        assert!(!args.contains(&"--all-versions".to_owned()));
        assert!(!args.contains(&"--version".to_owned()));
        assert_eq!(args, vec!["uninstall", "--id", "Git.Git", "--exact"]);
    }

    #[test]
    fn install_emits_settings_in_order_and_omits_blank_location() {
        let options = InstallOptions {
            source: Some("winget".into()),
            arch: Arch::X64,
            scope: Scope::Machine,
            location: Some("   ".into()),
            silent: true,
            accept: true,
        };
        let args = install("Git.Git", &options);
        assert_eq!(
            args,
            vec![
                "install",
                "--id",
                "Git.Git",
                "--exact",
                "--source",
                "winget",
                "--architecture",
                "x64",
                "--scope",
                "machine",
                "--silent",
                // `--accept-source-agreements` is added by the runner as a
                // common flag; only the package agreement lives here.
                "--accept-package-agreements",
            ]
        );
    }

    #[test]
    fn install_includes_a_trimmed_location_when_set() {
        let options = InstallOptions {
            location: Some("  D:\\Software  ".into()),
            ..InstallOptions::default()
        };
        let args = install("Git.Git", &options);
        let index = args
            .iter()
            .position(|a| a == "--location")
            .expect("--location emitted");
        assert_eq!(args[index + 1], "D:\\Software");
    }

    #[test]
    fn source_add_names_the_trust_level_explicitly() {
        let args = source_add(
            "MyCompany",
            "https://nexus.internal/winget",
            "Microsoft.Rest",
            false,
        );
        assert_eq!(
            args,
            vec![
                "source",
                "add",
                "--name",
                "MyCompany",
                "--arg",
                "https://nexus.internal/winget",
                "--type",
                "Microsoft.Rest",
                "--trust-level",
                "none",
            ]
        );
    }

    #[test]
    fn source_export_may_target_one_source_or_all() {
        assert_eq!(source_export(None), vec!["source", "export"]);
        assert_eq!(
            source_export(Some("winget")),
            vec!["source", "export", "--name", "winget"]
        );
    }

    #[test]
    fn search_filters_map_to_their_flags() {
        let filters = SearchFilters {
            exact: true,
            moniker: false,
            tag: true,
            command: true,
        };
        let args = search("code", Some("winget"), filters, None);
        assert!(args.contains(&"--exact".to_owned()));
        assert!(args.contains(&"--tag".to_owned()));
        assert!(args.contains(&"--command".to_owned()));
        assert!(!args.contains(&"--moniker".to_owned()));
    }
}
