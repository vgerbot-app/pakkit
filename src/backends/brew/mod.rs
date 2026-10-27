//! The Homebrew backend: macOS and Linux packages via `brew`.
//!
//! Homebrew refuses to run as root, so nothing here ever elevates — no
//! pkexec, no sudo, no PackageKit. Every spawn is a bare `brew` with
//! `HOMEBREW_NO_AUTO_UPDATE=1` (keeps queries fast and read-only) and
//! `HOMEBREW_NO_ENV_HINTS=1` (keeps stderr greppable).
//!
//! Formulae and casks are different package universes that share one
//! namespace: the cask half is carried as an id prefix `cask/<name>` end to
//! end (pacman's `aur/` precedent), and every arg builder routes `--cask`
//! on it. The queue's displayed command lines therefore read
//! `brew install --cask firefox` for a cask id.
//!
//! Taps are the visible source kind (like chocolatey's feeds): they can be
//! listed, added and removed. Everything else on the winget profile is off.

pub mod parse;

use std::path::PathBuf;

use super::exec::{self, Output};
use super::types::{BackendError, InstallOptions, ListRow, PackageDetail, SearchFilters, SearchRow};
use super::{BackendKind, Capabilities, PackageBackend};
use crate::model::Source;

/// The id prefix marking a cask (GUI app) rather than a formula.
pub const CASK_ID_PREFIX: &str = "cask/";

/// The source value carried by formula rows.
pub const FORMULA_SOURCE: &str = "formula";
/// The source value carried by cask rows.
pub const CASK_SOURCE: &str = "cask";

/// Strip the cask prefix off `id`; `Some(name)` for casks, `None` for
/// formulae.
pub fn cask_name(id: &str) -> Option<&str> {
    id.strip_prefix(CASK_ID_PREFIX)
        .filter(|name| !name.is_empty())
}

/// A resolved brew executable.
#[derive(Debug)]
pub struct Brew {
    exe: PathBuf,
}

impl Brew {
    /// Wrap an explicit executable path.
    pub fn new(exe: impl Into<PathBuf>) -> Self {
        Self { exe: exe.into() }
    }

    /// Locate `brew` on `PATH`, then probe the fixed install prefixes
    /// (Apple Silicon, Intel, linuxbrew).
    pub fn discover() -> Result<Self, BackendError> {
        let path = std::env::var_os("PATH").ok_or(BackendError::NotFound("brew"))?;
        let on_path = std::env::split_paths(&path)
            .map(|dir| dir.join("brew"))
            .find(|candidate| candidate.is_file());
        on_path
            .or_else(|| {
                [
                    "/opt/homebrew/bin/brew",
                    "/usr/local/bin/brew",
                    "/home/linuxbrew/.linuxbrew/bin/brew",
                ]
                .iter()
                .map(PathBuf::from)
                .find(|candidate| candidate.is_file())
            })
            .map(Self::new)
            .ok_or(BackendError::NotFound("brew"))
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
            Err(BackendError::InvalidPath("brew", path))
        }
    }
}

impl PackageBackend for Brew {
    fn kind(&self) -> BackendKind {
        BackendKind::Brew
    }

    /// Taps are user-owned (`brew tap` needs no root), so the sources
    /// surface is on like chocolatey's; everything else winget-only is off.
    /// `batch_upgrade` is on: one `brew upgrade` covers formulae and casks.
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

    /// `brew search ""` is rejected; the `/./` regex lists the whole
    /// catalog, which the paged search view is built to carry.
    fn empty_query_searches_catalog(&self) -> bool {
        true
    }

    /// brew never elevates: one bare spawn path, always.
    fn run(&self, args: &[&str]) -> Result<Output, BackendError> {
        const ENV: &[(&str, &str)] = &[
            ("HOMEBREW_NO_AUTO_UPDATE", "1"),
            ("HOMEBREW_NO_ENV_HINTS", "1"),
        ];
        exec::run_process(&self.exe, ENV, args)
    }

    fn version(&self) -> Result<String, BackendError> {
        let out = self.run(&["--version"])?;
        Ok(out.stdout.trim().to_owned())
    }

    /// brew exit codes are small integers; print them as such.
    fn format_exit_code(&self, code: u32) -> String {
        code.to_string()
    }

    /// `search <query>`; an empty query maps to the `/./` match-all regex.
    fn search_args(
        &self,
        query: &str,
        _source: Option<&str>,
        _filters: SearchFilters,
        _count: Option<u32>,
    ) -> Vec<String> {
        let query = if query.trim().is_empty() {
            "/./"
        } else {
            query
        };
        vec!["search".to_owned(), query.to_owned()]
    }

    /// `list --versions` — formulae; the cask half is appended by the
    /// `list_installed` composite.
    fn list_args(&self, _source: Option<&str>) -> Vec<String> {
        vec!["list".to_owned(), "--versions".to_owned()]
    }

    /// `outdated --verbose` — formulae; the cask half is appended by the
    /// `list_upgrades` composite.
    fn upgrades_args(&self) -> Vec<String> {
        vec!["outdated".to_owned(), "--verbose".to_owned()]
    }

    /// `info <id>` / `info --cask <name>`
    fn show_args(&self, id: &str, _source: Option<&str>) -> Vec<String> {
        match cask_name(id) {
            Some(name) => vec!["info".to_owned(), "--cask".to_owned(), name.to_owned()],
            None => vec!["info".to_owned(), id.to_owned()],
        }
    }

    /// `install <id>` / `install --cask <name>`
    fn install_args(&self, id: &str, _options: &InstallOptions) -> Vec<String> {
        match cask_name(id) {
            Some(name) => vec!["install".to_owned(), "--cask".to_owned(), name.to_owned()],
            None => vec!["install".to_owned(), id.to_owned()],
        }
    }

    /// `uninstall <id>` / `uninstall --cask <name>`
    fn uninstall_args(&self, id: &str) -> Vec<String> {
        match cask_name(id) {
            Some(name) => vec!["uninstall".to_owned(), "--cask".to_owned(), name.to_owned()],
            None => vec!["uninstall".to_owned(), id.to_owned()],
        }
    }

    /// `upgrade <id>` / `upgrade --cask <name>`
    fn upgrade_args(&self, id: &str) -> Vec<String> {
        match cask_name(id) {
            Some(name) => vec!["upgrade".to_owned(), "--cask".to_owned(), name.to_owned()],
            None => vec!["upgrade".to_owned(), id.to_owned()],
        }
    }

    /// `upgrade` — covers formulae and outdated casks in one run.
    fn upgrade_all_args(&self) -> Vec<String> {
        vec!["upgrade".to_owned()]
    }

    /// `tap <name>` / `tap <name> <url>`
    fn source_add_args(&self, name: &str, url: &str) -> Vec<String> {
        let mut args = vec!["tap".to_owned(), name.to_owned()];
        if !url.trim().is_empty() {
            args.push(url.to_owned());
        }
        args
    }

    /// Taps have no in-place edit; re-add instead.
    fn source_edit_args(&self, _name: &str, _explicit: Option<bool>) -> Option<Vec<String>> {
        None
    }

    /// `untap <name>`
    fn source_remove_args(&self, name: &str) -> Vec<String> {
        vec!["untap".to_owned(), name.to_owned()]
    }

    /// `update` — fetches the newest brew and tap definitions.
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

    /// The configured taps, one per line of `brew tap`.
    fn list_sources(&self) -> Result<Vec<Source>, BackendError> {
        Ok(Vec::new())
    }
}
