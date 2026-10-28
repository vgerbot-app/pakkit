//! The apk backend: Alpine Linux packages via `apk` (apk-tools v2/v3).
//!
//! Read-only queries (`search`/`list`/`info`/`version`) run bare with
//! `LC_ALL=C` pinned. Mutations wrap in `pkexec apk …` — unlike apt/dnf/
//! zypper there is no PackageKit path: PackageKit is rarely installed on
//! Alpine, so the probe would always fail anyway. A stock Alpine system
//! lacks polkit/pkexec too; the resulting spawn failure surfaces as the
//! queue item's error text. `doas`/`sudo` are deliberately not used: the
//! app runs children with null stdin, so an interactive password prompt
//! could never be answered.
//!
//! Repository management is out of scope — `/etc/apk/repositories` is a
//! single root-owned file — so every capability flag is off, exactly like
//! apt.

pub mod parse;

use std::path::{Path, PathBuf};

use super::exec::{self, Output};
use super::types::{BackendError, InstallOptions, ListRow, PackageDetail, SearchFilters, SearchRow};
use super::{BackendKind, Capabilities, PackageBackend};
use crate::model::Source;

/// A resolved apk executable.
#[derive(Debug)]
pub struct Apk {
    exe: PathBuf,
}

impl Apk {
    /// Wrap an explicit executable path.
    pub fn new(exe: impl Into<PathBuf>) -> Self {
        Self { exe: exe.into() }
    }

    /// Locate `apk` on `PATH`.
    pub fn discover() -> Result<Self, BackendError> {
        let path = std::env::var_os("PATH").ok_or(BackendError::NotFound("apk"))?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("apk"))
            .find(|candidate| candidate.is_file())
            .map(Self::new)
            .ok_or(BackendError::NotFound("apk"))
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
            Err(BackendError::InvalidPath("apk", path))
        }
    }
}

/// Whether `args` is a query that can run unprivileged; anything else is a
/// mutation routed through pkexec.
fn is_read_only(args: &[&str]) -> bool {
    matches!(
        args.first().copied(),
        Some("search" | "list" | "info" | "version" | "--version")
    )
}

impl PackageBackend for Apk {
    fn kind(&self) -> BackendKind {
        BackendKind::Apk
    }

    /// apk exposes none of the winget-profile surface, exactly like apt.
    /// `batch_upgrade` is on: one `upgrade` transaction instead of
    /// per-package upgrades.
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
        const ENV: &[(&str, &str)] = &[("LC_ALL", "C")];
        if is_read_only(args) {
            return exec::run_process(&self.exe, ENV, args);
        }
        let mut full: Vec<&str> = Vec::with_capacity(args.len() + 1);
        full.push("apk");
        full.extend_from_slice(args);
        exec::run_process(Path::new("pkexec"), ENV, &full)
    }

    fn version(&self) -> Result<String, BackendError> {
        let out = self.run(&["--version"])?;
        Ok(out.stdout.trim().to_owned())
    }

    /// apk exit codes are small integers; print them as such.
    fn format_exit_code(&self, code: u32) -> String {
        code.to_string()
    }

    /// 126/127 arrive from pkexec dismissal/denial; give them real messages.
    fn describe_exit(&self, code: u32) -> String {
        match code {
            126 => tr!("exitcode.auth_dismissed"),
            127 => tr!("exitcode.auth_denied"),
            _ => tr!("exitcode.unknown"),
        }
    }

    /// `search -v <query>` — `-v` adds versions and descriptions.
    fn search_args(
        &self,
        query: &str,
        _source: Option<&str>,
        _filters: SearchFilters,
        _count: Option<u32>,
    ) -> Vec<String> {
        vec!["search".to_owned(), "-v".to_owned(), query.to_owned()]
    }

    /// `list --installed`
    fn list_args(&self, _source: Option<&str>) -> Vec<String> {
        vec!["list".to_owned(), "--installed".to_owned()]
    }

    /// `version -l '<'` — packages whose installed version is behind.
    fn upgrades_args(&self) -> Vec<String> {
        vec!["version".to_owned(), "-l".to_owned(), "<".to_owned()]
    }

    /// `info -a <id>` — all detail fields.
    fn show_args(&self, id: &str, _source: Option<&str>) -> Vec<String> {
        vec!["info".to_owned(), "-a".to_owned(), id.to_owned()]
    }

    /// `add <id>`
    fn install_args(&self, id: &str, _options: &InstallOptions) -> Vec<String> {
        vec!["add".to_owned(), id.to_owned()]
    }

    /// `del <id>`
    fn uninstall_args(&self, id: &str) -> Vec<String> {
        vec!["del".to_owned(), id.to_owned()]
    }

    /// `upgrade <id>` — verified on apk-tools 2.14.6 (Alpine 3.21):
    /// `apk upgrade [<PACKAGES>...]` accepts package constraints and
    /// upgrades only the listed packages plus needed dependencies
    /// (`apk upgrade -s musl` → `Upgrading musl (1.2.5-r9 -> 1.2.5-r12)`).
    fn upgrade_args(&self, id: &str) -> Vec<String> {
        vec!["upgrade".to_owned(), id.to_owned()]
    }

    /// `upgrade`
    fn upgrade_all_args(&self) -> Vec<String> {
        vec!["upgrade".to_owned()]
    }

    /// apk repos live in the single root-owned `/etc/apk/repositories`; the
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

    /// `update` — refreshes the repository indexes.
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

    /// apk sources are not surfaced (`sources_view` is off).
    fn list_sources(&self) -> Result<Vec<Source>, BackendError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apk() -> Apk {
        Apk::new("apk")
    }

    #[test]
    fn mutation_args_wrap_plain_apk_verbs() {
        let b = apk();
        assert_eq!(
            b.install_args("sudo", &InstallOptions::default()),
            vec!["add", "sudo"]
        );
        assert_eq!(b.uninstall_args("sudo"), vec!["del", "sudo"]);
        // Verified on apk-tools 2.14.6: `upgrade <pkg>` accepts a single
        // package constraint, so there is no `add --upgrade` fallback.
        assert_eq!(b.upgrade_args("sudo"), vec!["upgrade", "sudo"]);
        assert_eq!(b.upgrade_all_args(), vec!["upgrade"]);
        assert_eq!(b.source_update_args(None), vec!["update"]);
    }

    #[test]
    fn query_args_pin_versions_and_details() {
        let b = apk();
        assert_eq!(
            b.search_args("sudo", Some("ignored"), SearchFilters::default(), Some(10)),
            vec!["search", "-v", "sudo"]
        );
        assert_eq!(b.list_args(None), vec!["list", "--installed"]);
        assert_eq!(b.upgrades_args(), vec!["version", "-l", "<"]);
        assert_eq!(b.show_args("sudo", None), vec!["info", "-a", "sudo"]);
    }

    #[test]
    fn sources_are_out_of_scope() {
        let b = apk();
        assert_eq!(
            b.source_add_args("x", "https://example.com"),
            Vec::<String>::new()
        );
        assert_eq!(b.source_edit_args("x", Some(true)), None);
        assert_eq!(b.source_remove_args("x"), Vec::<String>::new());
    }

    #[test]
    fn read_only_routing_covers_queries_only() {
        assert!(is_read_only(&["search", "-v", "sudo"]));
        assert!(is_read_only(&["list", "--installed"]));
        assert!(is_read_only(&["info", "-a", "sudo"]));
        assert!(is_read_only(&["version", "-l", "<"]));
        assert!(is_read_only(&["--version"]));
        assert!(!is_read_only(&["add", "sudo"]));
        assert!(!is_read_only(&["del", "sudo"]));
        assert!(!is_read_only(&["upgrade"]));
        assert!(!is_read_only(&["update"]));
        assert!(!is_read_only(&[]));
    }

    #[test]
    fn capabilities_hide_everything_but_batch_upgrade() {
        let caps = apk().capabilities();
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
        assert_eq!(apk().format_exit_code(1), "1");
        assert_eq!(apk().format_exit_code(126), "126");
    }

    #[test]
    fn an_invalid_configured_path_is_rejected() {
        let result = Apk::resolve("/definitely/not/an/apk");
        assert!(matches!(result, Err(BackendError::InvalidPath("apk", _))));
    }
}
