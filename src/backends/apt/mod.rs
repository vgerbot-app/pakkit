//! The APT backend: Debian/Ubuntu packages via apt/apt-get.
//!
//! Read-only queries (`list`/`search`/`show`) run through the `apt` binary
//! with `LC_ALL=C` pinned — its headers and `Key: value` labels are
//! gettext-localized otherwise. Mutations need root, so they route through
//! `pkexec env DEBIAN_FRONTEND=noninteractive apt-get …`: polkit raises the
//! auth prompt, no sudo wrapper is needed. The queue drawer still displays
//! `apt install -y <id>` (cli_name + args) while the real spawn is the
//! pkexec pipeline; that divergence is deliberate, matching how flatpak
//! displays its own polkit-elevated commands.
//!
//! Repository management is out of scope — deb822 sources need root and key
//! handling to change — so every capability flag is off and the UI hides the
//! whole sources surface. apt itself never prompts (`-y` plus the
//! noninteractive frontend), so there is no silent/agreements surface either.

pub mod parse;

use std::path::{Path, PathBuf};

use super::exec::{self, Output};
use super::types::{BackendError, InstallOptions, ListRow, PackageDetail, SearchFilters, SearchRow};
use super::{BackendKind, Capabilities, PackageBackend};
use crate::model::Source;

/// A resolved apt executable.
#[derive(Debug, Clone)]
pub struct Apt {
    exe: PathBuf,
}

impl Apt {
    /// Wrap an explicit executable path.
    pub fn new(exe: impl Into<PathBuf>) -> Self {
        Self { exe: exe.into() }
    }

    /// Locate `apt` on `PATH`.
    pub fn discover() -> Result<Self, BackendError> {
        let path = std::env::var_os("PATH").ok_or(BackendError::NotFound("apt"))?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("apt"))
            .find(|candidate| candidate.is_file())
            .map(Self::new)
            .ok_or(BackendError::NotFound("apt"))
    }

    /// Honor an operator-supplied path, falling back to discovery.
    ///
    /// An empty configured path means "auto-detect".
    pub fn resolve(configured: &str) -> Result<Self, BackendError> {
        let trimmed = configured.trim();
        if trimmed.is_empty() {
            return Self::discover();
        }
        let path = PathBuf::from(trimmed);
        if path.is_file() {
            Ok(Self::new(path))
        } else {
            Err(BackendError::InvalidPath("apt", path))
        }
    }
}

/// Whether `args` is a query that can run unprivileged through `apt`
/// directly; anything else is a mutation routed through pkexec + apt-get.
fn is_read_only(args: &[&str]) -> bool {
    matches!(
        args.first().copied(),
        Some("list" | "search" | "show" | "--version")
    )
}

impl PackageBackend for Apt {
    fn kind(&self) -> BackendKind {
        BackendKind::Apt
    }

    /// apt exposes none of the winget-profile surface: no source management
    /// (repos are root-owned deb822 files), no export, no install targeting,
    /// and no interactivity to switch off.
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            sources_view: false,
            source_add: false,
            source_edit: false,
            export: false,
            install_arch: false,
            install_scope: false,
            install_location: false,
            silent: false,
            agreements: false,
            search_filters: false,
        }
    }

    fn run(&self, args: &[&str]) -> Result<Output, BackendError> {
        const ENV: &[(&str, &str)] = &[("LC_ALL", "C")];
        if is_read_only(args) {
            exec::run_process(&self.exe, ENV, args)
        } else {
            let mut full: Vec<&str> = Vec::with_capacity(args.len() + 3);
            full.extend(["env", "DEBIAN_FRONTEND=noninteractive", "apt-get"]);
            full.extend_from_slice(args);
            exec::run_process(Path::new("pkexec"), ENV, &full)
        }
    }

    fn version(&self) -> Result<String, BackendError> {
        let out = self.run(&["--version"])?;
        Ok(out.stdout.trim().to_owned())
    }

    /// apt exit codes are small integers (100 for most errors); print them
    /// as such.
    fn format_exit_code(&self, code: u32) -> String {
        code.to_string()
    }

    /// `search <query>`
    ///
    /// The query is pushed even when empty: `apt search ""` matches the
    /// whole catalog, which the paged search view is built to carry. There
    /// are no source, filter or count flags; the UI never offers them.
    fn search_args(
        &self,
        query: &str,
        _source: Option<&str>,
        _filters: SearchFilters,
        _count: Option<u32>,
    ) -> Vec<String> {
        vec!["search".to_owned(), query.to_owned()]
    }

    /// `list --installed`
    fn list_args(&self, _source: Option<&str>) -> Vec<String> {
        vec!["list".to_owned(), "--installed".to_owned()]
    }

    /// `list --upgradable`
    fn upgrades_args(&self) -> Vec<String> {
        vec!["list".to_owned(), "--upgradable".to_owned()]
    }

    /// `show <id>` — unknown packages exit 100, which the trait's default
    /// `show_package` filters out.
    fn show_args(&self, id: &str, _source: Option<&str>) -> Vec<String> {
        vec!["show".to_owned(), id.to_owned()]
    }

    /// `install -y <id>`
    fn install_args(&self, id: &str, _options: &InstallOptions) -> Vec<String> {
        vec!["install".to_owned(), "-y".to_owned(), id.to_owned()]
    }

    /// `remove -y <id>` — configuration files stay behind (no purge).
    fn uninstall_args(&self, id: &str) -> Vec<String> {
        vec!["remove".to_owned(), "-y".to_owned(), id.to_owned()]
    }

    /// `install --only-upgrade -y <id>` — apt's single-package upgrade.
    fn upgrade_args(&self, id: &str) -> Vec<String> {
        vec![
            "install".to_owned(),
            "--only-upgrade".to_owned(),
            "-y".to_owned(),
            id.to_owned(),
        ]
    }

    /// `upgrade -y` — safe semantics: never installs new dependencies and
    /// never removes packages, so dependency-shifted packages stay held
    /// back in the updates list rather than risking removals.
    fn upgrade_all_args(&self) -> Vec<String> {
        vec!["upgrade".to_owned(), "-y".to_owned()]
    }

    /// apt repos live in root-owned deb822 files with key management; the
    /// UI hides the whole sources surface, so these are unreachable.
    fn source_add_args(&self, _name: &str, _url: &str) -> Vec<String> {
        Vec::new()
    }

    fn source_edit_args(&self, _name: &str, _explicit: Option<bool>) -> Option<Vec<String>> {
        None
    }

    fn source_remove_args(&self, _name: &str) -> Vec<String> {
        Vec::new()
    }

    /// `update` — refreshes the package lists.
    fn source_update_args(&self, _name: Option<&str>) -> Vec<String> {
        vec!["update".to_owned()]
    }

    fn parse_search(&self, stdout: &str) -> Vec<SearchRow> {
        parse::parse_search(stdout)
    }

    fn parse_list(&self, stdout: &str) -> Vec<ListRow> {
        parse::parse_list(stdout)
    }

    fn parse_upgrades(&self, stdout: &str) -> Vec<ListRow> {
        parse::parse_upgrades(stdout)
    }

    fn parse_show(&self, stdout: &str) -> PackageDetail {
        parse::parse_show(stdout)
    }

    /// apt sources are not surfaced (`sources_view` is off).
    fn list_sources(&self) -> Result<Vec<Source>, BackendError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apt() -> Apt {
        Apt::new("apt")
    }

    #[test]
    fn mutation_args_are_built_for_apt_get() {
        let b = apt();
        assert_eq!(
            b.install_args("curl", &InstallOptions::default()),
            vec!["install", "-y", "curl"]
        );
        assert_eq!(b.uninstall_args("curl"), vec!["remove", "-y", "curl"]);
        assert_eq!(
            b.upgrade_args("curl"),
            vec!["install", "--only-upgrade", "-y", "curl"]
        );
        assert_eq!(b.upgrade_all_args(), vec!["upgrade", "-y"]);
        assert_eq!(b.source_update_args(None), vec!["update"]);
    }

    #[test]
    fn query_args_and_the_empty_query_catalog() {
        let b = apt();
        // An empty query is pushed verbatim: `apt search ""` lists the
        // whole catalog for the paged search view.
        assert_eq!(
            b.search_args("", None, SearchFilters::default(), None),
            vec!["search", ""]
        );
        assert_eq!(
            b.search_args("firefox", Some("ignored"), SearchFilters::default(), Some(10)),
            vec!["search", "firefox"]
        );
        assert_eq!(b.list_args(None), vec!["list", "--installed"]);
        assert_eq!(b.upgrades_args(), vec!["list", "--upgradable"]);
        assert_eq!(b.show_args("curl", None), vec!["show", "curl"]);
    }

    #[test]
    fn sources_are_out_of_scope() {
        let b = apt();
        assert_eq!(b.source_add_args("x", "https://example.com"), Vec::<String>::new());
        assert_eq!(b.source_edit_args("x", Some(true)), None);
        assert_eq!(b.source_remove_args("x"), Vec::<String>::new());
    }

    #[test]
    fn read_only_routing_covers_queries_only() {
        assert!(is_read_only(&["list", "--installed"]));
        assert!(is_read_only(&["search", "firefox"]));
        assert!(is_read_only(&["show", "curl"]));
        assert!(is_read_only(&["--version"]));
        assert!(!is_read_only(&["install", "-y", "curl"]));
        assert!(!is_read_only(&["remove", "-y", "curl"]));
        assert!(!is_read_only(&["upgrade", "-y"]));
        assert!(!is_read_only(&["update"]));
        assert!(!is_read_only(&[]));
    }

    #[test]
    fn capabilities_hide_the_entire_winget_surface() {
        let caps = apt().capabilities();
        assert!(!caps.sources_view);
        assert!(!caps.source_add);
        assert!(!caps.source_edit);
        assert!(!caps.export);
        assert!(!caps.install_arch);
        assert!(!caps.install_scope);
        assert!(!caps.install_location);
        assert!(!caps.silent);
        assert!(!caps.agreements);
        assert!(!caps.search_filters);
    }

    #[test]
    fn exit_codes_format_as_plain_integers() {
        assert_eq!(apt().format_exit_code(100), "100");
        assert_eq!(apt().format_exit_code(1), "1");
    }

    #[test]
    fn an_invalid_configured_path_is_rejected() {
        let result = Apt::resolve("/definitely/not/an/apt");
        assert!(matches!(result, Err(BackendError::InvalidPath("apt", _))));
    }
}
