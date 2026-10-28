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
use super::types::{
    BackendError, InstallOptions, ListRow, PackageDetail, SearchFilters, SearchRow,
};
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

#[cfg(test)]
mod tests {
    use super::*;

    fn zypper() -> Zypper {
        Zypper::new("zypper")
    }

    #[test]
    fn mutation_args_follow_the_packagekit_contract() {
        let b = zypper();
        assert_eq!(
            b.install_args("curl", &InstallOptions::default()),
            vec!["--non-interactive", "install", "curl"]
        );
        assert_eq!(
            b.uninstall_args("curl"),
            vec!["--non-interactive", "remove", "curl"]
        );
        assert_eq!(
            b.upgrade_args("curl"),
            vec!["--non-interactive", "update", "curl"]
        );
        assert_eq!(b.upgrade_all_args(), vec!["--non-interactive", "update"]);
        assert_eq!(
            b.source_update_args(None),
            vec!["--non-interactive", "refresh"]
        );
    }

    #[test]
    fn query_args_carry_the_noninteractive_flag() {
        let b = zypper();
        assert_eq!(
            b.search_args("sudo", Some("ignored"), SearchFilters::default(), Some(10)),
            vec!["--non-interactive", "search", "sudo"]
        );
        assert_eq!(
            b.list_args(None),
            vec!["--non-interactive", "packages", "--installed-only"]
        );
        assert_eq!(b.upgrades_args(), vec!["--non-interactive", "list-updates"]);
        assert_eq!(
            b.show_args("sudo", None),
            vec!["--non-interactive", "info", "sudo"]
        );
    }

    #[test]
    fn sources_are_out_of_scope() {
        let b = zypper();
        assert_eq!(
            b.source_add_args("x", "https://example.com"),
            Vec::<String>::new()
        );
        assert_eq!(b.source_edit_args("x", Some(true)), None);
        assert_eq!(b.source_remove_args("x"), Vec::<String>::new());
    }

    /// Every mutation shape the arg builders produce must be plannable as a
    /// PackageKit operation — otherwise that operation silently drops to the
    /// per-task pkexec prompts.
    #[cfg(target_os = "linux")]
    #[test]
    fn mutation_shapes_are_packagekit_plannable() {
        use crate::backends::packagekit;
        let b = zypper();
        let shapes: Vec<Vec<String>> = vec![
            b.install_args("curl", &InstallOptions::default()),
            b.uninstall_args("curl"),
            b.upgrade_args("curl"),
            b.upgrade_all_args(),
            b.source_update_args(None),
        ];
        for shape in shapes {
            let refs: Vec<&str> = shape.iter().map(String::as_str).collect();
            assert!(packagekit::plan(&refs).is_some(), "unplannable: {refs:?}");
        }
    }

    #[test]
    fn read_only_routing_skips_the_global_flag() {
        // The leading `--non-interactive` global flag is skipped before the
        // subcommand is matched.
        assert!(is_read_only(&["--non-interactive", "search", "sudo"]));
        assert!(is_read_only(&[
            "--non-interactive",
            "packages",
            "--installed-only"
        ]));
        assert!(is_read_only(&["--non-interactive", "list-updates"]));
        assert!(is_read_only(&["--non-interactive", "info", "sudo"]));
        assert!(is_read_only(&["--version"]));
        // Mutations are never read-only.
        assert!(!is_read_only(&["--non-interactive", "install", "sudo"]));
        assert!(!is_read_only(&["--non-interactive", "remove", "sudo"]));
        assert!(!is_read_only(&["--non-interactive", "update", "sudo"]));
        assert!(!is_read_only(&["--non-interactive", "update"]));
        assert!(!is_read_only(&["--non-interactive", "refresh"]));
        assert!(!is_read_only(&[]));
    }

    #[test]
    fn capabilities_hide_everything_but_batch_upgrade() {
        let caps = zypper().capabilities();
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
        assert_eq!(zypper().format_exit_code(102), "102");
        assert_eq!(zypper().format_exit_code(1), "1");
    }

    #[test]
    fn informational_exit_codes_are_benign() {
        let b = zypper();
        // 102 (reboot needed) and 103 (restart needed) report success.
        assert!(b.exit_is_benign(102));
        assert!(b.exit_is_benign(103));
        // 100 (ZYPPER_EXIT_ERR_BUG) and 101 (ZYPPER_EXIT_ERR_SYNTAX) do not.
        assert!(!b.exit_is_benign(100));
        assert!(!b.exit_is_benign(101));
    }

    #[test]
    fn an_invalid_configured_path_is_rejected() {
        let result = Zypper::resolve("/definitely/not/a/zypper");
        assert!(matches!(
            result,
            Err(BackendError::InvalidPath("zypper", _))
        ));
    }
}
