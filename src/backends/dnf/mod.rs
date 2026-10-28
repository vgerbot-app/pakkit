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
use super::types::{
    BackendError, InstallOptions, ListRow, PackageDetail, SearchFilters, SearchRow,
};
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dnf() -> Dnf {
        Dnf::new("dnf")
    }

    #[test]
    fn mutation_args_are_built_for_dnf() {
        let b = dnf();
        assert_eq!(
            b.install_args("curl", &InstallOptions::default()),
            vec!["install", "-y", "curl"]
        );
        assert_eq!(b.uninstall_args("curl"), vec!["remove", "-y", "curl"]);
        assert_eq!(b.upgrade_args("curl"), vec!["upgrade", "-y", "curl"]);
        assert_eq!(b.upgrade_all_args(), vec!["upgrade", "-y"]);
        assert_eq!(b.source_update_args(None), vec!["makecache", "--refresh"]);
    }

    #[test]
    fn query_args_stay_minimal() {
        let b = dnf();
        // dnf has no source/filter/count flags; the UI never offers them
        // and every one of these inputs is ignored.
        assert_eq!(
            b.search_args("sudo", Some("ignored"), SearchFilters::default(), Some(10)),
            vec!["search", "sudo"]
        );
        assert_eq!(b.list_args(None), vec!["list", "--installed"]);
        assert_eq!(b.upgrades_args(), vec!["check-update"]);
        assert_eq!(b.show_args("bash", None), vec!["info", "bash"]);
    }

    #[test]
    fn an_empty_query_does_not_search_the_catalog() {
        // Unlike apt, `dnf search ""` is an error ("no argument given"), so
        // the empty query stays client-side with the installed set.
        assert!(!dnf().empty_query_searches_catalog());
    }

    #[test]
    fn sources_are_out_of_scope() {
        let b = dnf();
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
        let b = dnf();
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
    fn read_only_routing_covers_queries_only() {
        assert!(is_read_only(&["search", "sudo"]));
        assert!(is_read_only(&["list", "--installed"]));
        assert!(is_read_only(&["info", "bash"]));
        assert!(is_read_only(&["check-update"]));
        assert!(is_read_only(&["--version"]));
        assert!(!is_read_only(&["install", "-y", "curl"]));
        assert!(!is_read_only(&["remove", "-y", "curl"]));
        assert!(!is_read_only(&["upgrade", "-y"]));
        assert!(!is_read_only(&["makecache", "--refresh"]));
        assert!(!is_read_only(&[]));
    }

    #[test]
    fn capabilities_hide_the_entire_winget_surface() {
        let caps = dnf().capabilities();
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
        assert_eq!(dnf().format_exit_code(100), "100");
        assert_eq!(dnf().format_exit_code(1), "1");
    }

    #[test]
    fn check_update_100_is_benign() {
        let b = dnf();
        assert!(b.exit_is_benign(100));
        assert!(!b.exit_is_benign(0));
        assert!(!b.exit_is_benign(1));
    }

    #[test]
    fn an_invalid_configured_path_is_rejected() {
        let result = Dnf::resolve("/definitely/not/a/dnf");
        assert!(matches!(result, Err(BackendError::InvalidPath("dnf", _))));
    }
}
