//! The Chocolatey backend: Windows packages via the `choco` CLI.
//!
//! Chocolatey targets v2.x, where `choco list` already means "installed
//! only" (the v1 `--local-only` flag was removed) and `--limit-output`
//! emits stable pipe-delimited rows (`name|version`) that every listing
//! parser here reads. `choco info` stays human-readable, so its parser
//! walks `key: value` lines and tolerates fields it does not know.
//!
//! Chocolatey needs an elevated shell for mutations but reports that as a
//! plain non-zero exit with an access-denied message — there are no typed
//! exit codes like winget's — so the error text is surfaced verbatim and
//! every spawn is a plain `choco …`.

pub mod parse;

use std::path::PathBuf;

use super::exec::{self, Output};
use super::types::{
    BackendError, InstallOptions, ListRow, PackageDetail, SOURCE_ALL, SearchFilters, SearchRow,
};
use super::{BackendKind, Capabilities, PackageBackend};
use crate::model::Source;

/// A resolved choco executable.
#[derive(Debug, Clone)]
pub struct Chocolatey {
    exe: PathBuf,
}

impl Chocolatey {
    /// Wrap an explicit executable path.
    pub fn new(exe: impl Into<PathBuf>) -> Self {
        Self { exe: exe.into() }
    }

    /// Locate `choco.exe`: `PATH` first, then the install root the
    /// Chocolatey installer itself records in `%ChocolateyInstall%`.
    pub fn discover() -> Result<Self, BackendError> {
        if let Some(path) = Self::on_path() {
            return Ok(Self::new(path));
        }
        if let Some(path) = Self::install_root()
            && path.is_file()
        {
            return Ok(Self::new(path));
        }
        Err(BackendError::NotFound("choco"))
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
            Err(BackendError::InvalidPath("choco", path))
        }
    }

    fn on_path() -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("choco.exe"))
            .find(|candidate| candidate.is_file())
    }

    /// `%ChocolateyInstall%\bin\choco.exe` — the standard install puts the
    /// real binary there and only shims onto `PATH`.
    fn install_root() -> Option<PathBuf> {
        let root = std::env::var_os("ChocolateyInstall")?;
        Some(PathBuf::from(root).join("bin").join("choco.exe"))
    }

    /// `--source` accepts one feed; the "all sources" sentinel and an empty
    /// value both mean "let choco pick".
    fn normalize_source(source: Option<&str>) -> Option<&str> {
        source
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != SOURCE_ALL)
    }
}

impl PackageBackend for Chocolatey {
    fn kind(&self) -> BackendKind {
        BackendKind::Chocolatey
    }

    /// Chocolatey has real source management (`choco source …`), but no
    /// export manifest, no install targeting and no generic silent flag —
    /// packages run their installers unattended by design. `batch_upgrade`
    /// is on: one `choco upgrade all` transaction covers every outdated
    /// package.
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

    fn run(&self, args: &[&str]) -> Result<Output, BackendError> {
        exec::run_process(&self.exe, &[], args)
    }

    fn version(&self) -> Result<String, BackendError> {
        let out = self.run(&["--version"])?;
        Ok(out
            .stdout
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default()
            .to_owned())
    }

    /// choco exit codes are small integers; print them as such.
    fn format_exit_code(&self, code: u32) -> String {
        code.to_string()
    }

    /// `search <query> --limit-output` — a missing query argument lists the
    /// whole feed, so the empty-query gate (`empty_query_searches_catalog`
    /// stays off) keeps this verbatim.
    fn search_args(
        &self,
        query: &str,
        _source: Option<&str>,
        _filters: SearchFilters,
        count: Option<u32>,
    ) -> Vec<String> {
        let mut args = vec![
            "search".to_owned(),
            query.to_owned(),
            "--limit-output".to_owned(),
        ];
        if let Some(count) = count {
            args.push("--page-size".to_owned());
            args.push(count.to_string());
        }
        args
    }

    /// `list --limit-output` — v2 scopes `list` to installed packages.
    fn list_args(&self, _source: Option<&str>) -> Vec<String> {
        vec!["list".to_owned(), "--limit-output".to_owned()]
    }

    /// `outdated --limit-output` — exits 0 whether or not anything is stale.
    fn upgrades_args(&self) -> Vec<String> {
        vec!["outdated".to_owned(), "--limit-output".to_owned()]
    }

    /// `info <id>` — no `--limit-output`: the pipe form carries only
    /// id|version, while the human form has the description the detail
    /// dialog shows.
    fn show_args(&self, id: &str, _source: Option<&str>) -> Vec<String> {
        vec!["info".to_owned(), id.to_owned()]
    }

    /// `install <id> -y` — `-y` answers choco's own confirmation prompts;
    /// package scripts handle unattended setup themselves.
    fn install_args(&self, id: &str, options: &InstallOptions) -> Vec<String> {
        let mut args = vec!["install".to_owned(), id.to_owned(), "-y".to_owned()];
        if let Some(source) = Self::normalize_source(options.source.as_deref()) {
            args.push("--source".to_owned());
            args.push(source.to_owned());
        }
        args
    }

    /// `uninstall <id> -y`
    fn uninstall_args(&self, id: &str) -> Vec<String> {
        vec!["uninstall".to_owned(), id.to_owned(), "-y".to_owned()]
    }

    /// `upgrade <id> -y`
    fn upgrade_args(&self, id: &str) -> Vec<String> {
        vec!["upgrade".to_owned(), id.to_owned(), "-y".to_owned()]
    }

    /// `upgrade all -y` — every outdated package in one transaction; pinned
    /// packages stay untouched.
    fn upgrade_all_args(&self) -> Vec<String> {
        vec!["upgrade".to_owned(), "all".to_owned(), "-y".to_owned()]
    }

    /// `source add --name <name> --source <url>`
    fn source_add_args(&self, name: &str, url: &str) -> Vec<String> {
        vec![
            "source".to_owned(),
            "add".to_owned(),
            "--name".to_owned(),
            name.to_owned(),
            "--source".to_owned(),
            url.to_owned(),
        ]
    }

    /// Chocolatey has no in-place source edit; sources are re-added.
    fn source_edit_args(&self, _name: &str, _explicit: Option<bool>) -> Option<Vec<String>> {
        None
    }

    /// `source remove --name <name>`
    fn source_remove_args(&self, name: &str) -> Vec<String> {
        vec![
            "source".to_owned(),
            "remove".to_owned(),
            "--name".to_owned(),
            name.to_owned(),
        ]
    }

    /// Feeds are plain URLs with no metadata to sync; there is no update
    /// command, so the UI's refresh button stays unreachable.
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

    /// `source list --limit-output`, mapped onto the shared source model:
    /// a disabled feed renders as unverified.
    fn list_sources(&self) -> Result<Vec<Source>, BackendError> {
        let out = self.run_checked(&["source", "list", "--limit-output"])?;
        Ok(parse::parse_source_list(&out.stdout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choco() -> Chocolatey {
        Chocolatey::new("choco")
    }

    #[test]
    fn mutation_args_are_plain_choco_commands() {
        let b = choco();
        assert_eq!(
            b.install_args("git", &InstallOptions::default()),
            vec!["install", "git", "-y"]
        );
        assert_eq!(b.uninstall_args("git"), vec!["uninstall", "git", "-y"]);
        assert_eq!(b.upgrade_args("git"), vec!["upgrade", "git", "-y"]);
        assert_eq!(b.upgrade_all_args(), vec!["upgrade", "all", "-y"]);
    }

    #[test]
    fn install_passes_a_real_source_but_not_the_all_sentinel() {
        let b = choco();
        let internal = InstallOptions {
            source: Some("internal".to_owned()),
            ..InstallOptions::default()
        };
        assert_eq!(
            b.install_args("git", &internal),
            vec!["install", "git", "-y", "--source", "internal"]
        );
        let all = InstallOptions {
            source: Some(SOURCE_ALL.to_owned()),
            ..InstallOptions::default()
        };
        assert_eq!(b.install_args("git", &all), vec!["install", "git", "-y"]);
    }

    #[test]
    fn query_args() {
        let b = choco();
        assert!(!b.empty_query_searches_catalog());
        assert_eq!(
            b.search_args("git", Some("ignored"), SearchFilters::default(), Some(25)),
            vec!["search", "git", "--limit-output", "--page-size", "25"]
        );
        assert_eq!(b.list_args(None), vec!["list", "--limit-output"]);
        assert_eq!(b.upgrades_args(), vec!["outdated", "--limit-output"]);
        assert_eq!(b.show_args("git", None), vec!["info", "git"]);
    }

    #[test]
    fn sources_have_add_and_remove_but_no_edit_or_sync() {
        let b = choco();
        assert_eq!(
            b.source_add_args("internal", "https://nexus.example.com/choco/"),
            vec![
                "source",
                "add",
                "--name",
                "internal",
                "--source",
                "https://nexus.example.com/choco/"
            ]
        );
        assert_eq!(b.source_edit_args("internal", Some(true)), None);
        assert_eq!(
            b.source_remove_args("internal"),
            vec!["source", "remove", "--name", "internal"]
        );
        assert_eq!(b.source_update_args(None), Vec::<String>::new());
    }

    #[test]
    fn capabilities() {
        let caps = choco().capabilities();
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
        assert_eq!(choco().format_exit_code(1), "1");
        assert_eq!(choco().format_exit_code(1603), "1603");
    }

    #[test]
    fn an_invalid_configured_path_is_rejected() {
        let result = Chocolatey::resolve("/definitely/not/a/choco");
        assert!(matches!(result, Err(BackendError::InvalidPath("choco", _))));
    }
}
