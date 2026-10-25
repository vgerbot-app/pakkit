//! The APT backend: Debian/Ubuntu packages via apt/apt-get.
//!
//! Read-only queries (`list`/`search`/`show`) run through the `apt` binary
//! with `LC_ALL=C` pinned — its headers and `Key: value` labels are
//! gettext-localized otherwise.
//!
//! Mutations need root. On Linux they go to PackageKit over D-Bus first
//! ([`packagekit`]): its polkit actions for install/update are
//! `auth_admin_keep`, so one authorization covers a burst of queue items for
//! ~5 minutes instead of every task prompting. When PackageKit is
//! unreachable (or the argument shape is unrecognized), the mutation falls
//! back to `pkexec env DEBIAN_FRONTEND=noninteractive apt-get …`, which
//! prompts per spawn. The queue drawer still displays `apt install -y <id>`
//! (cli_name + args) regardless of which pipeline runs; that divergence is
//! deliberate, matching how flatpak displays its own polkit-elevated
//! commands.
//!
//! Two semantic differences versus the pkexec pipeline are accepted:
//! `upgrade -y` maps to GetUpdates + UpdatePackages, which may install new
//! dependencies (plain `apt-get upgrade` never does); and removals use
//! polkit action `package-remove`, which is `auth_admin` — one prompt per
//! removal, not cached.
//!
//! Repository management is out of scope — deb822 sources need root and key
//! handling to change — so every capability flag is off and the UI hides the
//! whole sources surface. apt itself never prompts (`-y` plus the
//! noninteractive frontend), so there is no silent/agreements surface either.

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

/// PackageKit probe states for [`Apt::pk`].
#[cfg(target_os = "linux")]
const PK_UNPROBED: u8 = 0;
#[cfg(target_os = "linux")]
const PK_AVAILABLE: u8 = 1;
#[cfg(target_os = "linux")]
const PK_UNAVAILABLE: u8 = 2;

/// A resolved apt executable.
#[derive(Debug)]
pub struct Apt {
    exe: PathBuf,
    /// Lazily probed PackageKit availability (see the `PK_*` constants).
    /// Mutations prefer the D-Bus path; the probe result is cached for the
    /// process lifetime since packagekitd installs do not appear mid-run.
    #[cfg(target_os = "linux")]
    pk: AtomicU8,
}

impl Apt {
    /// Wrap an explicit executable path.
    pub fn new(exe: impl Into<PathBuf>) -> Self {
        Self {
            exe: exe.into(),
            #[cfg(target_os = "linux")]
            pk: AtomicU8::new(PK_UNPROBED),
        }
    }

    /// Whether PackageKit answered a D-Bus probe (cached after the first
    /// call). Off Linux there is no PackageKit, so mutations always take
    /// the pkexec pipeline.
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

    /// Locate `apt` on `PATH`.
    pub fn discover() -> Result<Self, BackendError> {
        let path = std::env::var_os("PATH").ok_or(BackendError::NotFound("apt"))?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("apt"))
            .find(|candidate| candidate.is_file())
            .map(Self::new)
            .ok_or(BackendError::NotFound("apt"))
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
            Err(BackendError::InvalidPath("apt", path))
        }
    }
}

/// Whether `args` is a query that can run unprivileged through `apt`
/// directly; anything else is a mutation routed through pkexec + apt-get.
fn is_read_only(args: &[&str]) -> bool {
    matches!(
        args.first().copied(),
        Some("list" | "search" | "show" | "--version")
    )
}

impl PackageBackend for Apt {
    fn kind(&self) -> BackendKind {
        BackendKind::Apt
    }

    /// apt exposes none of the winget-profile surface: no source management
    /// (repos are root-owned deb822 files), no export, no install targeting,
    /// and no interactivity to switch off.
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
        }
    }

    fn run(&self, args: &[&str]) -> Result<Output, BackendError> {
        const ENV: &[(&str, &str)] = &[("LC_ALL", "C")];
        if is_read_only(args) {
            return exec::run_process(&self.exe, ENV, args);
        }
        // Mutations prefer PackageKit: one polkit authorization covers a
        // burst of queue items. Transport failures and unrecognized argument
        // shapes fall through to the pkexec pipeline.
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
        let mut full: Vec<&str> = Vec::with_capacity(args.len() + 3);
        full.extend(["env", "DEBIAN_FRONTEND=noninteractive", "apt-get"]);
        full.extend_from_slice(args);
        exec::run_process(Path::new("pkexec"), ENV, &full)
    }

    fn version(&self) -> Result<String, BackendError> {
        let out = self.run(&["--version"])?;
        Ok(out.stdout.trim().to_owned())
    }

    /// apt exit codes are small integers (100 for most errors); print them
    /// as such.
    fn format_exit_code(&self, code: u32) -> String {
        code.to_string()
    }

    /// 126/127 arrive from both elevation paths (pkexec dismissal and the
    /// PackageKit mapping in [`packagekit`]); give them real messages.
    fn describe_exit(&self, code: u32) -> String {
        match code {
            126 => tr!("exitcode.auth_dismissed"),
            127 => tr!("exitcode.auth_denied"),
            _ => tr!("exitcode.unknown"),
        }
    }

    /// `apt search ""` lists the whole catalog, which the paged search view
    /// is built to carry.
    fn empty_query_searches_catalog(&self) -> bool {
        true
    }

    /// `search <query>`
    ///
    /// The query is pushed even when empty: `apt search ""` matches the
    /// whole catalog, which the paged search view is built to carry. There
    /// are no source, filter or count flags; the UI never offers them.
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

    /// `list --upgradable`
    fn upgrades_args(&self) -> Vec<String> {
        vec!["list".to_owned(), "--upgradable".to_owned()]
    }

    /// `show <id>` — unknown packages exit 100, which the trait's default
    /// `show_package` filters out.
    fn show_args(&self, id: &str, _source: Option<&str>) -> Vec<String> {
        vec!["show".to_owned(), id.to_owned()]
    }

    /// `install -y <id>`
    fn install_args(&self, id: &str, _options: &InstallOptions) -> Vec<String> {
        vec!["install".to_owned(), "-y".to_owned(), id.to_owned()]
    }

    /// `remove -y <id>` — configuration files stay behind (no purge).
    fn uninstall_args(&self, id: &str) -> Vec<String> {
        vec!["remove".to_owned(), "-y".to_owned(), id.to_owned()]
    }

    /// `install --only-upgrade -y <id>` — apt's single-package upgrade.
    fn upgrade_args(&self, id: &str) -> Vec<String> {
        vec![
            "install".to_owned(),
            "--only-upgrade".to_owned(),
            "-y".to_owned(),
            id.to_owned(),
        ]
    }

    /// `upgrade -y` — safe semantics: never installs new dependencies and
    /// never removes packages, so dependency-shifted packages stay held
    /// back in the updates list rather than risking removals.
    fn upgrade_all_args(&self) -> Vec<String> {
        vec!["upgrade".to_owned(), "-y".to_owned()]
    }

    /// apt repos live in root-owned deb822 files with key management; the
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

    /// `update` — refreshes the package lists.
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

    /// apt sources are not surfaced (`sources_view` is off).
    fn list_sources(&self) -> Result<Vec<Source>, BackendError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apt() -> Apt {
        Apt::new("apt")
    }

    #[test]
    fn mutation_args_are_built_for_apt_get() {
        let b = apt();
        assert_eq!(
            b.install_args("curl", &InstallOptions::default()),
            vec!["install", "-y", "curl"]
        );
        assert_eq!(b.uninstall_args("curl"), vec!["remove", "-y", "curl"]);
        assert_eq!(
            b.upgrade_args("curl"),
            vec!["install", "--only-upgrade", "-y", "curl"]
        );
        assert_eq!(b.upgrade_all_args(), vec!["upgrade", "-y"]);
        assert_eq!(b.source_update_args(None), vec!["update"]);
    }

    #[test]
    fn query_args_and_the_empty_query_catalog() {
        let b = apt();
        assert!(b.empty_query_searches_catalog());
        // An empty query is pushed verbatim: `apt search ""` lists the
        // whole catalog for the paged search view.
        assert_eq!(
            b.search_args("", None, SearchFilters::default(), None),
            vec!["search", ""]
        );
        assert_eq!(
            b.search_args("firefox", Some("ignored"), SearchFilters::default(), Some(10)),
            vec!["search", "firefox"]
        );
        assert_eq!(b.list_args(None), vec!["list", "--installed"]);
        assert_eq!(b.upgrades_args(), vec!["list", "--upgradable"]);
        assert_eq!(b.show_args("curl", None), vec!["show", "curl"]);
    }

    #[test]
    fn sources_are_out_of_scope() {
        let b = apt();
        assert_eq!(b.source_add_args("x", "https://example.com"), Vec::<String>::new());
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
        let b = apt();
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
        assert!(is_read_only(&["list", "--installed"]));
        assert!(is_read_only(&["search", "firefox"]));
        assert!(is_read_only(&["show", "curl"]));
        assert!(is_read_only(&["--version"]));
        assert!(!is_read_only(&["install", "-y", "curl"]));
        assert!(!is_read_only(&["remove", "-y", "curl"]));
        assert!(!is_read_only(&["upgrade", "-y"]));
        assert!(!is_read_only(&["update"]));
        assert!(!is_read_only(&[]));
    }

    #[test]
    fn capabilities_hide_the_entire_winget_surface() {
        let caps = apt().capabilities();
        assert!(!caps.sources_view);
        assert!(!caps.source_add);
        assert!(!caps.source_edit);
        assert!(!caps.export);
        assert!(!caps.install_location);
        assert!(!caps.silent);
        assert!(!caps.agreements);
        assert!(!caps.search_filters);
    }

    #[test]
    fn exit_codes_format_as_plain_integers() {
        assert_eq!(apt().format_exit_code(100), "100");
        assert_eq!(apt().format_exit_code(1), "1");
    }

    #[test]
    fn an_invalid_configured_path_is_rejected() {
        let result = Apt::resolve("/definitely/not/an/apt");
        assert!(matches!(result, Err(BackendError::InvalidPath("apt", _))));
    }
}
