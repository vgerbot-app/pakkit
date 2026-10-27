//! The DNF backend: Fedora/RHEL packages via dnf (dnf4 or dnf5).
//!
//! Read-only queries (`search`/`list`/`info`/`check-update`) run through the
//! `dnf` binary directly with `LC_ALL=C` pinned — its headers and labels are
//! gettext-localized otherwise.
//!
//! Mutations need root. Like apt and pacman they go to PackageKit over D-Bus
//! first ([`packagekit`]), falling back to `pkexec dnf …` (one prompt per
//! spawn) when the daemon is unreachable or the argument shape is
//! unrecognized. The queue drawer displays `dnf install -y <id>` regardless
//! of which pipeline runs.
//!
//! Repository management is out of scope — `/etc/yum.repos.d` files are
//! root-owned and key-bearing — so every capability flag is off and the UI
//! hides the whole sources surface, exactly like apt.

pub mod parse;

use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicU8, Ordering};

use super::exec::{self, Output};
#[cfg(target_os = "linux")]
use super::packagekit;
use super::types::{BackendError, InstallOptions, ListRow, PackageDetail, SearchFilters, SearchRow};
use super::{BackendKind, Capabilities, PackageBackend};
use crate::model::Source;

/// PackageKit probe states for [`Dnf::pk`].
#[cfg(target_os = "linux")]
const PK_UNPROBED: u8 = 0;
#[cfg(target_os = "linux")]
const PK_AVAILABLE: u8 = 1;
#[cfg(target_os = "linux")]
const PK_UNAVAILABLE: u8 = 2;

/// A resolved dnf executable.
#[derive(Debug)]
pub struct Dnf {
    exe: PathBuf,
    /// Lazily probed PackageKit availability, cached for the process
    /// lifetime (see the `PK_*` constants).
    #[cfg(target_os = "linux")]
    pk: AtomicU8,
}

impl Dnf {
    /// Wrap an explicit executable path.
    pub fn new(exe: impl Into<PathBuf>) -> Self {
        Self {
            exe: exe.into(),
            #[cfg(target_os = "linux")]
            pk: AtomicU8::new(PK_UNPROBED),
        }
    }

    /// Whether PackageKit answered a D-Bus probe (cached after the first
    /// call).
    #[cfg(target_os = "linux")]
    fn packagekit_usable(&self) -> bool {
        match self.pk.load(Ordering::Relaxed) {
            PK_UNPROBED => {
                let state = if packagekit::available() {
                    PK_AVAILABLE
                } else {
                    PK_UNAVAILABLE
                };
                self.pk.store(state, Ordering::Relaxed);
                state == PK_AVAILABLE
            }
            state => state == PK_AVAILABLE,
        }
    }

    /// Locate `dnf` on `PATH`, falling back to `dnf5` for dnf5-only installs.
    pub fn discover() -> Result<Self, BackendError> {
        let path = std::env::var_os("PATH").ok_or(BackendError::NotFound("dnf"))?;
        let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
        ["dnf", "dnf5"]
            .iter()
            .flat_map(|name| dirs.iter().map(move |dir| dir.join(name)))
            .find(|candidate| candidate.is_file())
            .map(Self::new)
            .ok_or(BackendError::NotFound("dnf"))
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
            Err(BackendError::InvalidPath("dnf", path))
        }
    }
}

/// Whether `args` is a query that can run unprivileged; anything else is a
/// mutation routed through PackageKit / pkexec.
fn is_read_only(args: &[&str]) -> bool {
    matches!(
        args.first().copied(),
        Some("search" | "list" | "info" | "check-update" | "--version")
    )
}

impl PackageBackend for Dnf {
    fn kind(&self) -> BackendKind {
        BackendKind::Dnf
    }

    /// dnf exposes none of the winget-profile surface, exactly like apt.
    /// `batch_upgrade` is on: one `upgrade -y` transaction instead of
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
        #[cfg(target_os = "linux")]
        if self.packagekit_usable()
            && let Some(op) = packagekit::plan(args)
        {
            match packagekit::run(&op) {
                Ok(output) => return Ok(output),
                Err(packagekit::RunError::Transaction(output)) => return Ok(output),
                Err(packagekit::RunError::Transport(_)) => {}
            }
        }
        let mut full: Vec<&str> = Vec::with_capacity(args.len() + 1);
        full.push("dnf");
        full.extend_from_slice(args);
        exec::run_process(Path::new("pkexec"), ENV, &full)
    }

    fn version(&self) -> Result<String, BackendError> {
        let out = self.run(&["--version"])?;
        Ok(out.stdout.trim().to_owned())
    }

    /// dnf exit codes are small integers; print them as such.
    fn format_exit_code(&self, code: u32) -> String {
        code.to_string()
    }

    /// 100 is `check-update`'s "updates pending" (the parse target, not a
    /// failure). PackageKit's synthetic 100 (resolution failure) only
    /// surfaces from mutations via `run`, which never consults this.
    fn exit_is_benign(&self, code: u32) -> bool {
        code == 100
    }

    /// 126/127 arrive from both elevation paths (pkexec dismissal and the
    /// PackageKit mapping); give them real messages.
    fn describe_exit(&self, code: u32) -> String {
        match code {
            126 => tr!("exitcode.auth_dismissed"),
            127 => tr!("exitcode.auth_denied"),
            _ => tr!("exitcode.unknown"),
        }
    }

    /// `search <query>` — no source/filter/count flags exist.
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

    /// `check-update` — exits 100 when updates are pending (benign).
    fn upgrades_args(&self) -> Vec<String> {
        vec!["check-update".to_owned()]
    }

    /// `info <id>`
    fn show_args(&self, id: &str, _source: Option<&str>) -> Vec<String> {
        vec!["info".to_owned(), id.to_owned()]
    }

    /// `install -y <id>`
    fn install_args(&self, id: &str, _options: &InstallOptions) -> Vec<String> {
        vec!["install".to_owned(), "-y".to_owned(), id.to_owned()]
    }

    /// `remove -y <id>`
    fn uninstall_args(&self, id: &str) -> Vec<String> {
        vec!["remove".to_owned(), "-y".to_owned(), id.to_owned()]
    }

    /// `upgrade -y <id>`
    fn upgrade_args(&self, id: &str) -> Vec<String> {
        vec!["upgrade".to_owned(), "-y".to_owned(), id.to_owned()]
    }

    /// `upgrade -y`
    fn upgrade_all_args(&self) -> Vec<String> {
        vec!["upgrade".to_owned(), "-y".to_owned()]
    }

    /// dnf repos live in root-owned files under `/etc/yum.repos.d`; the UI
    /// hides the whole sources surface, so these are unreachable.
    fn source_add_args(&self, _name: &str, _url: &str) -> Vec<String> {
        Vec::new()
    }

    fn source_edit_args(&self, _name: &str, _explicit: Option<bool>) -> Option<Vec<String>> {
        None
    }

    fn source_remove_args(&self, _name: &str) -> Vec<String> {
        Vec::new()
    }

    /// `makecache --refresh` — refreshes the metadata caches.
    fn source_update_args(&self, _name: Option<&str>) -> Vec<String> {
        vec!["makecache".to_owned(), "--refresh".to_owned()]
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

    /// dnf sources are not surfaced (`sources_view` is off).
    fn list_sources(&self) -> Result<Vec<Source>, BackendError> {
        Ok(Vec::new())
    }
}
