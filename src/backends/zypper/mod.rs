//! The Zypper backend: openSUSE packages via `zypper --non-interactive`.
//!
//! Every invocation carries the global `--non-interactive` flag first, so
//! the read-only classifier skips leading global flags before matching the
//! subcommand. Read-only queries (`search`/`packages`/`list-updates`/`info`)
//! run bare with `LC_ALL=C` pinned; mutations go to PackageKit over D-Bus
//! first ([`packagekit`], the zypp backend), falling back to
//! `pkexec zypper …`.
//!
//! Repository management is out of scope — `/etc/zypp/repos.d` files are
//! root-owned — so every capability flag is off, exactly like apt.
//!
//! Zypper's informational exit codes 102 (reboot needed) and 103 (restart
//! needed) report a *successful* mutation; they are benign here.

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

/// PackageKit probe states for [`Zypper::pk`].
#[cfg(target_os = "linux")]
const PK_UNPROBED: u8 = 0;
#[cfg(target_os = "linux")]
const PK_AVAILABLE: u8 = 1;
#[cfg(target_os = "linux")]
const PK_UNAVAILABLE: u8 = 2;

/// A resolved zypper executable.
#[derive(Debug)]
pub struct Zypper {
    exe: PathBuf,
    /// Lazily probed PackageKit availability, cached for the process
    /// lifetime (see the `PK_*` constants).
    #[cfg(target_os = "linux")]
    pk: AtomicU8,
}

impl Zypper {
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

    /// Locate `zypper` on `PATH`.
    pub fn discover() -> Result<Self, BackendError> {
        let path = std::env::var_os("PATH").ok_or(BackendError::NotFound("zypper"))?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("zypper"))
            .find(|candidate| candidate.is_file())
            .map(Self::new)
            .ok_or(BackendError::NotFound("zypper"))
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
            Err(BackendError::InvalidPath("zypper", path))
        }
    }
}

/// Whether `args` is a query that can run unprivileged. The global
/// `--non-interactive` flag always leads, so it is skipped before matching
/// the subcommand.
fn is_read_only(args: &[&str]) -> bool {
    let subcommand = args
        .iter()
        .copied()
        .find(|arg| !arg.starts_with("--") || *arg == "--version");
    matches!(
        subcommand,
        Some("search" | "packages" | "list-updates" | "info" | "--version")
    )
}

impl PackageBackend for Zypper {
    fn kind(&self) -> BackendKind {
        BackendKind::Zypper
    }

    /// zypper exposes none of the winget-profile surface, exactly like apt.
    /// `batch_upgrade` is on: one `update` transaction instead of
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
        full.push("zypper");
        full.extend_from_slice(args);
        exec::run_process(Path::new("pkexec"), ENV, &full)
    }

    fn version(&self) -> Result<String, BackendError> {
        let out = self.run(&["--version"])?;
        Ok(out.stdout.trim().to_owned())
    }

    /// zypper exit codes are small integers; print them as such.
    fn format_exit_code(&self, code: u32) -> String {
        code.to_string()
    }

    /// 102 (ZYPPER_EXIT_INF_REBOOT_NEEDED) and 103
    /// (ZYPPER_EXIT_INF_RESTART_NEEDED) both mean the mutation succeeded.
    fn exit_is_benign(&self, code: u32) -> bool {
        code == 102 || code == 103
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

    /// `--non-interactive search <query>` — no source/filter/count flags.
    fn search_args(
        &self,
        query: &str,
        _source: Option<&str>,
        _filters: SearchFilters,
        _count: Option<u32>,
    ) -> Vec<String> {
        vec![
            "--non-interactive".to_owned(),
            "search".to_owned(),
            query.to_owned(),
        ]
    }

    /// `--non-interactive packages --installed-only`
    fn list_args(&self, _source: Option<&str>) -> Vec<String> {
        vec![
            "--non-interactive".to_owned(),
            "packages".to_owned(),
            "--installed-only".to_owned(),
        ]
    }

    /// `--non-interactive list-updates`
    fn upgrades_args(&self) -> Vec<String> {
        vec!["--non-interactive".to_owned(), "list-updates".to_owned()]
    }

    /// `--non-interactive info <id>`
    fn show_args(&self, id: &str, _source: Option<&str>) -> Vec<String> {
        vec![
            "--non-interactive".to_owned(),
            "info".to_owned(),
            id.to_owned(),
        ]
    }

    /// `--non-interactive install <id>`
    fn install_args(&self, id: &str, _options: &InstallOptions) -> Vec<String> {
        vec![
            "--non-interactive".to_owned(),
            "install".to_owned(),
            id.to_owned(),
        ]
    }

    /// `--non-interactive remove <id>`
    fn uninstall_args(&self, id: &str) -> Vec<String> {
        vec![
            "--non-interactive".to_owned(),
            "remove".to_owned(),
            id.to_owned(),
        ]
    }

    /// `--non-interactive update <id>`
    fn upgrade_args(&self, id: &str) -> Vec<String> {
        vec![
            "--non-interactive".to_owned(),
            "update".to_owned(),
            id.to_owned(),
        ]
    }

    /// `--non-interactive update`
    fn upgrade_all_args(&self) -> Vec<String> {
        vec!["--non-interactive".to_owned(), "update".to_owned()]
    }

    /// zypper repos live in root-owned files under `/etc/zypp/repos.d`; the
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

    /// `--non-interactive refresh` — refreshes the repo metadata.
    fn source_update_args(&self, _name: Option<&str>) -> Vec<String> {
        vec!["--non-interactive".to_owned(), "refresh".to_owned()]
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

    /// zypper sources are not surfaced (`sources_view` is off).
    fn list_sources(&self) -> Result<Vec<Source>, BackendError> {
        Ok(Vec::new())
    }
}
