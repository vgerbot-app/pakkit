//! The Scoop backend: Windows portable apps via `scoop`.
//!
//! Scoop is unprivileged by design (per-user installs under `~/scoop`), so
//! nothing here ever elevates — no UAC help, no pkexec. Every spawn is a
//! bare `scoop` call.
//!
//! Discovery resolves the `scoop.exe` shim binary scoop installs for
//! itself (`~/scoop/shims/scoop.exe`): `Command::new` cannot spawn the
//! `scoop.ps1`/`scoop.cmd` entry points directly, while the `.exe` shim
//! internally invokes PowerShell. TODO: verify the shim filename on a real
//! Windows scoop install and document it here.
//!
//! Buckets are the visible source kind (like chocolatey's feeds): they can
//! be listed, added and removed. Everything else on the winget profile is
//! off.
//!
//! Scoop is Windows-only, so the parsers in [`parse`] are tested against
//! fixtures reconstructed from Scoop's own output code
//! (github.com/ScoopInstaller/Scoop: `libexec/scoop-*.ps1`,
//! `lib/buckets.ps1`, `supporting/formats/ScoopTypes.Format.ps1xml`); see
//! the `parse` module docs for the per-command provenance.

pub mod parse;

use std::path::PathBuf;

use super::exec::{self, Output};
use super::types::{
    BackendError, InstallOptions, ListRow, PackageDetail, SearchFilters, SearchRow,
};
use super::{BackendKind, Capabilities, PackageBackend};
use crate::model::Source;

/// A resolved scoop executable.
#[derive(Debug)]
pub struct Scoop {
    exe: PathBuf,
}

impl Scoop {
    /// Wrap an explicit executable path.
    pub fn new(exe: impl Into<PathBuf>) -> Self {
        Self { exe: exe.into() }
    }

    /// Locate `scoop.exe` on `PATH`, then probe `%USERPROFILE%\scoop\shims`.
    pub fn discover() -> Result<Self, BackendError> {
        let path = std::env::var_os("PATH").ok_or(BackendError::NotFound("scoop"))?;
        let on_path = std::env::split_paths(&path)
            .flat_map(|dir| [dir.join("scoop.exe"), dir.join("scoop")])
            .find(|candidate| candidate.is_file());
        on_path
            .or_else(|| {
                std::env::var_os("USERPROFILE").map(|home| {
                    PathBuf::from(home)
                        .join("scoop")
                        .join("shims")
                        .join("scoop.exe")
                })
            })
            .filter(|candidate| candidate.is_file())
            .map(Self::new)
            .ok_or(BackendError::NotFound("scoop"))
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
            Err(BackendError::InvalidPath("scoop", path))
        }
    }
}

impl PackageBackend for Scoop {
    fn kind(&self) -> BackendKind {
        BackendKind::Scoop
    }

    /// Buckets are user-writable, so the sources surface is on like
    /// chocolatey's; everything else winget-only is off. `batch_upgrade` is
    /// on: one `scoop update *` covers every outdated app.
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            sources_view: true,
            source_add: true,
            source_edit: false,
            export: false,
            install_location: false,
            silent: false,
            agreements: false,
            search_filters: false,
            batch_upgrade: true,
        }
    }

    /// Scoop never elevates: one bare spawn path, always.
    fn run(&self, args: &[&str]) -> Result<Output, BackendError> {
        exec::run_process(&self.exe, &[], args)
    }

    fn version(&self) -> Result<String, BackendError> {
        let out = self.run(&["--version"])?;
        Ok(out.stdout.trim().to_owned())
    }

    /// scoop exit codes are small integers; print them as such.
    fn format_exit_code(&self, code: u32) -> String {
        code.to_string()
    }

    /// `search <query>` — scoop rejects an empty query, which
    /// `empty_query_searches_catalog` (false) already keeps client-side.
    fn search_args(
        &self,
        query: &str,
        _source: Option<&str>,
        _filters: SearchFilters,
        _count: Option<u32>,
    ) -> Vec<String> {
        vec!["search".to_owned(), query.to_owned()]
    }

    /// `list`
    fn list_args(&self, _source: Option<&str>) -> Vec<String> {
        vec!["list".to_owned()]
    }

    /// `status` — installed apps with a newer manifest version.
    fn upgrades_args(&self) -> Vec<String> {
        vec!["status".to_owned()]
    }

    /// `info <id>`
    fn show_args(&self, id: &str, _source: Option<&str>) -> Vec<String> {
        vec!["info".to_owned(), id.to_owned()]
    }

    /// `install <id>`
    fn install_args(&self, id: &str, _options: &InstallOptions) -> Vec<String> {
        vec!["install".to_owned(), id.to_owned()]
    }

    /// `uninstall <id>`
    fn uninstall_args(&self, id: &str) -> Vec<String> {
        vec!["uninstall".to_owned(), id.to_owned()]
    }

    /// `update <id>`
    fn upgrade_args(&self, id: &str) -> Vec<String> {
        vec!["update".to_owned(), id.to_owned()]
    }

    /// `update *` — a literal asterisk argument; the child is spawned
    /// without a shell, so no glob expansion happens.
    fn upgrade_all_args(&self) -> Vec<String> {
        vec!["update".to_owned(), "*".to_owned()]
    }

    /// `bucket add <name>` / `bucket add <name> <url>`
    fn source_add_args(&self, name: &str, url: &str) -> Vec<String> {
        let mut args = vec!["bucket".to_owned(), "add".to_owned(), name.to_owned()];
        if !url.trim().is_empty() {
            args.push(url.to_owned());
        }
        args
    }

    /// Buckets have no in-place edit; re-add instead.
    fn source_edit_args(&self, _name: &str, _explicit: Option<bool>) -> Option<Vec<String>> {
        None
    }

    /// `bucket rm <name>`
    fn source_remove_args(&self, name: &str) -> Vec<String> {
        vec!["bucket".to_owned(), "rm".to_owned(), name.to_owned()]
    }

    /// `update` — updates scoop itself and every bucket's manifests.
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

    /// `bucket list` — one row per configured bucket. The table's `Source`
    /// column (a git remote, or a local path for non-git buckets) maps to
    /// `url`; buckets are user-managed, hence trusted and explicit.
    fn list_sources(&self) -> Result<Vec<Source>, BackendError> {
        let out = self.run_checked(&["bucket", "list"])?;
        Ok(parse::parse_bucket_list(&out.stdout)
            .into_iter()
            .map(|(name, url)| Source {
                name,
                url,
                ty: "scoop-bucket".to_owned(),
                trusted: true,
                explicit: true,
                packages: None,
                updated: None,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scoop() -> Scoop {
        Scoop::new("scoop")
    }

    #[test]
    fn mutation_args_are_plain_scoop_commands() {
        let b = scoop();
        assert_eq!(
            b.install_args("git", &InstallOptions::default()),
            vec!["install", "git"]
        );
        assert_eq!(b.uninstall_args("git"), vec!["uninstall", "git"]);
        assert_eq!(b.upgrade_args("git"), vec!["update", "git"]);
        // A literal asterisk: the child is spawned without a shell.
        assert_eq!(b.upgrade_all_args(), vec!["update", "*"]);
    }

    #[test]
    fn query_args() {
        let b = scoop();
        assert!(!b.empty_query_searches_catalog());
        assert_eq!(
            b.search_args("git", Some("main"), SearchFilters::default(), Some(25)),
            vec!["search", "git"]
        );
        assert_eq!(b.list_args(None), vec!["list"]);
        assert_eq!(b.upgrades_args(), vec!["status"]);
        assert_eq!(b.show_args("git", None), vec!["info", "git"]);
    }

    #[test]
    fn bucket_add_with_and_without_url_and_rm() {
        let b = scoop();
        assert_eq!(
            b.source_add_args("extras", ""),
            vec!["bucket", "add", "extras"]
        );
        assert_eq!(
            b.source_add_args("mybucket", "https://example.com/mybucket.git"),
            vec![
                "bucket",
                "add",
                "mybucket",
                "https://example.com/mybucket.git"
            ]
        );
        assert_eq!(b.source_edit_args("extras", Some(true)), None);
        assert_eq!(
            b.source_remove_args("extras"),
            vec!["bucket", "rm", "extras"]
        );
        assert_eq!(b.source_update_args(None), vec!["update"]);
    }

    #[test]
    fn capabilities() {
        let caps = scoop().capabilities();
        assert!(caps.sources_view);
        assert!(caps.source_add);
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
        assert_eq!(scoop().format_exit_code(1), "1");
        assert_eq!(scoop().format_exit_code(1603), "1603");
    }

    #[test]
    fn an_invalid_configured_path_is_rejected() {
        let result = Scoop::resolve("/definitely/not/a/scoop");
        assert!(matches!(result, Err(BackendError::InvalidPath("scoop", _))));
    }
}
