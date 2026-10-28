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
//!
//! The parser fixtures under `tests/fixtures/brew_*.txt` follow brew 4.x's
//! documented non-TTY output shapes; the `homebrew/brew` container pull was
//! unavailable at authoring time, so they are written from the documented
//! forms rather than captured from a live `brew`.

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

    /// `list --versions` formula rows plus `list --cask --versions` cask
    /// rows, id-prefixed here — neither listing carries a kind marker. Each
    /// half tolerates failure as an empty list.
    fn list_installed(&self) -> Vec<ListRow> {
        let args = self.list_args(None);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut rows = self
            .run(&refs)
            .map(|out| {
                let mut rows = parse::parse_list(&out.stdout);
                for row in &mut rows {
                    row.source = FORMULA_SOURCE.to_owned();
                }
                rows
            })
            .unwrap_or_default();
        if let Ok(out) = self.run(&["list", "--cask", "--versions"]) {
            for mut row in parse::parse_list(&out.stdout) {
                row.id = format!("{CASK_ID_PREFIX}{}", row.name);
                row.source = CASK_SOURCE.to_owned();
                rows.push(row);
            }
        }
        rows
    }

    /// `outdated --verbose` formula rows plus `outdated --cask --verbose`
    /// cask rows, id-prefixed here for the same reason as
    /// [`Brew::list_installed`]. Each half tolerates failure as empty.
    fn list_upgrades(&self) -> Vec<ListRow> {
        let args = self.upgrades_args();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut rows = self
            .run(&refs)
            .map(|out| {
                let mut rows = parse::parse_upgrades(&out.stdout);
                for row in &mut rows {
                    row.source = FORMULA_SOURCE.to_owned();
                }
                rows
            })
            .unwrap_or_default();
        if let Ok(out) = self.run(&["outdated", "--cask", "--verbose"]) {
            for mut row in parse::parse_upgrades(&out.stdout) {
                row.id = format!("{CASK_ID_PREFIX}{}", row.name);
                row.source = CASK_SOURCE.to_owned();
                rows.push(row);
            }
        }
        rows
    }

    /// The configured taps, one per line of `brew tap`. An `owner/repo` tap
    /// maps to its conventional `homebrew-<repo>` GitHub URL; anything else
    /// (a fully custom tap name) leaves the URL empty.
    fn list_sources(&self) -> Result<Vec<Source>, BackendError> {
        let out = self.run_checked(&["tap"])?;
        Ok(parse::parse_tap_list(&out.stdout)
            .into_iter()
            .map(|name| {
                let url = match name.split_once('/') {
                    Some((owner, repo)) if !owner.is_empty() && !repo.is_empty() => {
                        format!("https://github.com/{owner}/homebrew-{repo}")
                    }
                    _ => String::new(),
                };
                Source {
                    name,
                    url,
                    ty: "brew-tap".to_owned(),
                    trusted: true,
                    explicit: true,
                    packages: None,
                    updated: None,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brew() -> Brew {
        Brew::new("brew")
    }

    #[test]
    fn cask_ids_route_to_the_cask_flag_shapes() {
        let b = brew();
        assert_eq!(
            b.show_args("cask/firefox", None),
            vec!["info", "--cask", "firefox"]
        );
        assert_eq!(b.show_args("firefox", None), vec!["info", "firefox"]);
        assert_eq!(
            b.install_args("cask/firefox", &InstallOptions::default()),
            vec!["install", "--cask", "firefox"]
        );
        assert_eq!(
            b.install_args("wget", &InstallOptions::default()),
            vec!["install", "wget"]
        );
        assert_eq!(
            b.uninstall_args("cask/firefox"),
            vec!["uninstall", "--cask", "firefox"]
        );
        assert_eq!(b.uninstall_args("wget"), vec!["uninstall", "wget"]);
        assert_eq!(
            b.upgrade_args("cask/firefox"),
            vec!["upgrade", "--cask", "firefox"]
        );
        assert_eq!(b.upgrade_args("wget"), vec!["upgrade", "wget"]);
        assert_eq!(b.upgrade_all_args(), vec!["upgrade"]);
    }

    #[test]
    fn query_args_and_the_empty_query_catalog() {
        let b = brew();
        assert!(b.empty_query_searches_catalog());
        assert_eq!(
            b.search_args("", None, SearchFilters::default(), None),
            vec!["search", "/./"]
        );
        assert_eq!(
            b.search_args("firefox", Some("ignored"), SearchFilters::default(), Some(10)),
            vec!["search", "firefox"]
        );
        assert_eq!(b.list_args(None), vec!["list", "--versions"]);
        assert_eq!(b.upgrades_args(), vec!["outdated", "--verbose"]);
    }

    #[test]
    fn taps_have_add_and_remove_but_no_edit() {
        let b = brew();
        assert_eq!(
            b.source_add_args("homebrew/cask", ""),
            vec!["tap", "homebrew/cask"]
        );
        assert_eq!(
            b.source_add_args("user/repo", "https://example.com/user/homebrew-repo"),
            vec!["tap", "user/repo", "https://example.com/user/homebrew-repo"]
        );
        assert_eq!(b.source_edit_args("homebrew/cask", Some(true)), None);
        assert_eq!(b.source_remove_args("homebrew/cask"), vec!["untap", "homebrew/cask"]);
        assert_eq!(b.source_update_args(None), vec!["update"]);
    }

    #[test]
    fn capabilities_show_sources_and_batch_upgrade_only() {
        let caps = brew().capabilities();
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
        assert_eq!(brew().format_exit_code(1), "1");
        assert_eq!(brew().format_exit_code(127), "127");
    }

    #[test]
    fn cask_name_strips_only_the_prefix() {
        assert_eq!(cask_name("cask/firefox"), Some("firefox"));
        assert_eq!(cask_name("firefox"), None);
        assert_eq!(cask_name("cask/"), None);
    }

    #[test]
    fn an_invalid_configured_path_is_rejected() {
        let result = Brew::resolve("/definitely/not/a/brew");
        assert!(matches!(result, Err(BackendError::InvalidPath("brew", _))));
    }
}
