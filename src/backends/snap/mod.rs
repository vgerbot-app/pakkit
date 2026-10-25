//! The snap backend: Snap Store packages via the `snap` CLI.
//!
//! snapd owns privilege escalation itself — an unprivileged `snap install`
//! raises the daemon's own polkit prompt — so, like flatpak, every spawn is
//! a plain `snap …` with `LC_ALL=C` pinned and there is no pkexec path.
//!
//! The Snap Store is the only source snapd serves, so every capability flag
//! is off (as with apt) and the UI hides the sources surface.

pub mod parse;

use std::path::PathBuf;

use super::exec::{self, Output};
use super::types::{
    BackendError, InstallOptions, ListRow, PackageDetail, SearchFilters, SearchRow,
};
use super::{BackendKind, Capabilities, PackageBackend};
use crate::model::Source;

/// A resolved snap executable.
#[derive(Debug, Clone)]
pub struct Snap {
    exe: PathBuf,
}

impl Snap {
    /// Wrap an explicit executable path.
    pub fn new(exe: impl Into<PathBuf>) -> Self {
        Self { exe: exe.into() }
    }

    /// Locate `snap` on `PATH`.
    pub fn discover() -> Result<Self, BackendError> {
        let path = std::env::var_os("PATH").ok_or(BackendError::NotFound("snap"))?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("snap"))
            .find(|candidate| candidate.is_file())
            .map(Self::new)
            .ok_or(BackendError::NotFound("snap"))
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
            Err(BackendError::InvalidPath("snap", path))
        }
    }
}

impl PackageBackend for Snap {
    fn kind(&self) -> BackendKind {
        BackendKind::Snap
    }

    /// snap exposes none of the winget-profile surface: one store, no
    /// export, no install targeting. `batch_upgrade` is on: one
    /// `snap refresh` transaction (and one snapd polkit authorization)
    /// covers every outdated snap.
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            sources_view: false,
            source_add: false,
            source_edit: false,
            export: false,
            install_location: false,
            silent: false,
            agreements: false,
            search_filters: false,
            batch_upgrade: true,
        }
    }

    fn run(&self, args: &[&str]) -> Result<Output, BackendError> {
        exec::run_process(&self.exe, &[("LC_ALL", "C")], args)
    }

    fn version(&self) -> Result<String, BackendError> {
        let out = self.run(&["version"])?;
        // `snap version` prints one `key  version` line per component
        // (snap, snapd, series, kernel, …); the snapd line is the one that
        // matters for transactions.
        for line in out.stdout.lines() {
            if let Some(version) = line.strip_prefix("snapd") {
                return Ok(format!("snapd {}", version.trim()));
            }
        }
        Ok(out.stdout.trim().to_owned())
    }

    /// snap exit codes are small integers; print them as such.
    fn format_exit_code(&self, code: u32) -> String {
        code.to_string()
    }

    /// `find <query>` — a missing query argument errors, so the empty-query
    /// gate (`empty_query_searches_catalog` stays off) keeps this verbatim.
    fn search_args(
        &self,
        query: &str,
        _source: Option<&str>,
        _filters: SearchFilters,
        _count: Option<u32>,
    ) -> Vec<String> {
        vec!["find".to_owned(), query.to_owned()]
    }

    /// `list`
    fn list_args(&self, _source: Option<&str>) -> Vec<String> {
        vec!["list".to_owned()]
    }

    /// `refresh --list` — exits 0 with an empty table when everything is
    /// current.
    fn upgrades_args(&self) -> Vec<String> {
        vec!["refresh".to_owned(), "--list".to_owned()]
    }

    /// `info <id>`
    fn show_args(&self, id: &str, _source: Option<&str>) -> Vec<String> {
        vec!["info".to_owned(), id.to_owned()]
    }

    /// `install <id>`
    fn install_args(&self, id: &str, _options: &InstallOptions) -> Vec<String> {
        vec!["install".to_owned(), id.to_owned()]
    }

    /// `remove <id>` — snapshot data stays behind (no `--purge`).
    fn uninstall_args(&self, id: &str) -> Vec<String> {
        vec!["remove".to_owned(), id.to_owned()]
    }

    /// `refresh <id>`
    fn upgrade_args(&self, id: &str) -> Vec<String> {
        vec!["refresh".to_owned(), id.to_owned()]
    }

    /// `refresh` — every outdated snap in one transaction.
    fn upgrade_all_args(&self) -> Vec<String> {
        vec!["refresh".to_owned()]
    }

    /// snapd serves the Snap Store only; there is nothing to manage, and
    /// the UI hides the whole sources surface, so these are unreachable.
    fn source_add_args(&self, _name: &str, _url: &str) -> Vec<String> {
        Vec::new()
    }

    fn source_edit_args(&self, _name: &str, _explicit: Option<bool>) -> Option<Vec<String>> {
        None
    }

    fn source_remove_args(&self, _name: &str) -> Vec<String> {
        Vec::new()
    }

    fn source_update_args(&self, _name: Option<&str>) -> Vec<String> {
        Vec::new()
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

    /// The Snap Store is not surfaced (`sources_view` is off).
    fn list_sources(&self) -> Result<Vec<Source>, BackendError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> Snap {
        Snap::new("snap")
    }

    #[test]
    fn mutation_args_are_plain_snap_commands() {
        let b = snap();
        assert_eq!(
            b.install_args("firefox", &InstallOptions::default()),
            vec!["install", "firefox"]
        );
        assert_eq!(b.uninstall_args("firefox"), vec!["remove", "firefox"]);
        assert_eq!(b.upgrade_args("firefox"), vec!["refresh", "firefox"]);
        assert_eq!(b.upgrade_all_args(), vec!["refresh"]);
    }

    #[test]
    fn query_args() {
        let b = snap();
        assert!(!b.empty_query_searches_catalog());
        assert_eq!(
            b.search_args(
                "firefox",
                Some("ignored"),
                SearchFilters::default(),
                Some(10)
            ),
            vec!["find", "firefox"]
        );
        assert_eq!(b.list_args(None), vec!["list"]);
        assert_eq!(b.upgrades_args(), vec!["refresh", "--list"]);
        assert_eq!(b.show_args("firefox", None), vec!["info", "firefox"]);
    }

    #[test]
    fn sources_are_out_of_scope() {
        let b = snap();
        assert_eq!(
            b.source_add_args("x", "https://example.com"),
            Vec::<String>::new()
        );
        assert_eq!(b.source_edit_args("x", Some(true)), None);
        assert_eq!(b.source_remove_args("x"), Vec::<String>::new());
        assert_eq!(b.source_update_args(None), Vec::<String>::new());
    }

    #[test]
    fn capabilities_hide_the_entire_winget_surface() {
        let caps = snap().capabilities();
        assert!(!caps.sources_view);
        assert!(!caps.source_add);
        assert!(!caps.source_edit);
        assert!(!caps.export);
        assert!(!caps.install_location);
        assert!(!caps.silent);
        assert!(!caps.agreements);
        assert!(!caps.search_filters);
        assert!(caps.batch_upgrade);
    }

    #[test]
    fn exit_codes_format_as_plain_integers() {
        assert_eq!(snap().format_exit_code(1), "1");
        assert_eq!(snap().format_exit_code(255), "255");
    }

    #[test]
    fn an_invalid_configured_path_is_rejected() {
        let result = Snap::resolve("/definitely/not/a/snap");
        assert!(matches!(result, Err(BackendError::InvalidPath("snap", _))));
    }
}
