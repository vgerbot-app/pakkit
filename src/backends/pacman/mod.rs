//! The pacman backend: Arch Linux repo packages, plus AUR via `yay`.
//!
//! Read-only queries (`-Ss`/`-Q…`/`-Si`) run through the `pacman` binary
//! with `LC_ALL=C` pinned — its `Key : value` labels are gettext-localized
//! otherwise.
//!
//! Mutations need root. Like the apt backend, they go to PackageKit over
//! D-Bus first (its polkit actions cache an authorization for ~5 minutes,
//! so a burst of queue items prompts once), falling back to `pkexec pacman
//! …` when PackageKit is unreachable or the argument shape is unrecognized.
//! The queue drawer still displays `pacman -S --noconfirm <id>` regardless
//! of which pipeline runs; that divergence is deliberate, matching apt.
//!
//! AUR support is folded in when `yay` is on PATH: searches additionally
//! query `yay -Ss --aur`, and AUR rows carry the id prefix `aur/` end to
//! end, so the queue — which forwards only the package id — can still route
//! them. AUR installs/upgrades produce the marker-prefixed shape
//! `--from-aur -S --noconfirm <name>`, which [`Pacman::run`] reroutes to
//! the `yay` binary directly: yay builds as the calling user and elevates
//! through sudo itself, so it must not be wrapped in pkexec. With the app's
//! null stdin an interactive sudo prompt fails; the stderr then lands in
//! the queue's error-details dialog (passwordless sudo is required for AUR
//! installs). Once built, AUR packages live in pacman's local database, so
//! uninstalls are plain `pacman -R`.
//!
//! Repository management is out of scope — repos are root-owned entries in
//! `/etc/pacman.conf` — so every capability flag is off (as with apt) and
//! the UI hides the whole sources surface.

pub mod parse;

use std::collections::HashSet;
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

/// PackageKit probe states for [`Pacman::pk`].
#[cfg(target_os = "linux")]
const PK_UNPROBED: u8 = 0;
#[cfg(target_os = "linux")]
const PK_AVAILABLE: u8 = 1;
#[cfg(target_os = "linux")]
const PK_UNAVAILABLE: u8 = 2;

/// Every spawn pins the C locale; yay and pacman both gettext-localize.
const ENV: &[(&str, &str)] = &[("LC_ALL", "C")];

/// The id prefix marking AUR packages, e.g. `aur/firefox-nightly`.
pub const AUR_ID_PREFIX: &str = "aur/";

/// The marker argument prefixing AUR mutation shapes. Not a real pacman
/// flag — it exists so [`Pacman::run`] can reroute the shape to `yay` on
/// arguments alone, the queue's only channel.
pub const AUR_MARKER: &str = "--from-aur";

/// The bare package name of an `aur/`-prefixed id.
fn aur_name(id: &str) -> Option<&str> {
    id.strip_prefix(AUR_ID_PREFIX)
        .filter(|name| !name.is_empty())
}

/// A resolved pacman executable, plus an optionally discovered yay.
#[derive(Debug)]
pub struct Pacman {
    exe: PathBuf,
    /// The AUR helper, when one was found on PATH at resolve time. A system
    /// without yay simply never surfaces AUR rows.
    yay: Option<PathBuf>,
    /// Lazily probed PackageKit availability (see the `PK_*` constants).
    #[cfg(target_os = "linux")]
    pk: AtomicU8,
}

impl Pacman {
    /// Wrap an explicit executable path and an optional yay path.
    pub fn new(exe: impl Into<PathBuf>, yay: Option<PathBuf>) -> Self {
        Self {
            exe: exe.into(),
            yay,
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

    /// Locate `pacman` on `PATH`, probing for `yay` alongside it.
    pub fn discover() -> Result<Self, BackendError> {
        let path = std::env::var_os("PATH").ok_or(BackendError::NotFound("pacman"))?;
        let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
        let exe = dirs
            .iter()
            .map(|dir| dir.join("pacman"))
            .find(|candidate| candidate.is_file())
            .ok_or(BackendError::NotFound("pacman"))?;
        let yay = dirs
            .iter()
            .map(|dir| dir.join("yay"))
            .find(|candidate| candidate.is_file());
        Ok(Self::new(exe, yay))
    }

    /// Honor an operator-supplied path, falling back to discovery.
    ///
    /// An empty configured path means "auto-detect". The path configures
    /// the pacman binary only; yay is always auto-probed.
    pub fn resolve(configured: &str) -> Result<Self, BackendError> {
        let trimmed = configured.trim();
        if trimmed.is_empty() {
            return Self::discover();
        }
        let path = PathBuf::from(trimmed);
        if !path.is_file() {
            return Err(BackendError::InvalidPath("pacman", path));
        }
        let yay = std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join("yay"))
                .find(|candidate| candidate.is_file())
        });
        Ok(Self::new(path, yay))
    }
}

/// Whether `args` is a query that can run unprivileged through `pacman`
/// directly; anything else is a mutation routed through PackageKit/pkexec.
fn is_read_only(args: &[&str]) -> bool {
    matches!(
        args.first().copied(),
        Some("-Ss" | "-Q" | "-Qq" | "-Qu" | "-Qm" | "-Qmq" | "-Si" | "-Qi" | "--version")
    )
}

impl PackageBackend for Pacman {
    fn kind(&self) -> BackendKind {
        BackendKind::Pacman
    }

    /// pacman exposes none of the winget-profile surface, for the same
    /// reasons as apt: repos are root-owned `/etc/pacman.conf` entries and
    /// there is no export or install targeting. `batch_upgrade` is on: one
    /// `pacman -Syu` transaction (and one polkit authorization) covers the
    /// whole updates list.
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
        // AUR mutations go to yay directly — never through pkexec (yay must
        // run as the building user) and never through PackageKit.
        if args.first() == Some(&AUR_MARKER) {
            let yay = self.yay.as_deref().ok_or(BackendError::NotFound("yay"))?;
            return exec::run_process(yay, ENV, &args[1..]);
        }
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
        let mut full: Vec<&str> = Vec::with_capacity(args.len() + 1);
        full.push("pacman");
        full.extend_from_slice(args);
        exec::run_process(Path::new("pkexec"), ENV, &full)
    }

    fn version(&self) -> Result<String, BackendError> {
        let out = self.run(&["--version"])?;
        // `pacman --version` prints ASCII art; the version lives on the
        // `Pacman vX.Y.Z - libalpm v…` line.
        for line in out.stdout.lines() {
            if let Some(ix) = line.find("Pacman v") {
                return Ok(line[ix..].trim().to_owned());
            }
        }
        Ok(out.stdout.trim().to_owned())
    }

    /// pacman exit codes are small integers; print them as such.
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

    /// `pacman -Ss ""` lists the whole repo catalog, which the paged search
    /// view is built to carry. (The AUR half stays query-only — there is no
    /// cheap full-AUR listing.)
    fn empty_query_searches_catalog(&self) -> bool {
        true
    }

    /// `-Ss <query>` — the query is pushed even when empty (full catalog).
    fn search_args(
        &self,
        query: &str,
        _source: Option<&str>,
        _filters: SearchFilters,
        _count: Option<u32>,
    ) -> Vec<String> {
        vec!["-Ss".to_owned(), query.to_owned()]
    }

    /// `-Q` — every installed package, `name version` per line. The foreign
    /// (AUR) marking lives in [`Pacman::list_installed`].
    fn list_args(&self, _source: Option<&str>) -> Vec<String> {
        vec!["-Q".to_owned()]
    }

    /// `-Qu` — repo upgrades as `name old -> new`; fails when the sync db
    /// is stale, which the composite tolerates as an empty list.
    fn upgrades_args(&self) -> Vec<String> {
        vec!["-Qu".to_owned()]
    }

    /// `-Si <id>` — the local fallback (`-Qi`) and the AUR path live in
    /// [`Pacman::show_package`].
    fn show_args(&self, id: &str, _source: Option<&str>) -> Vec<String> {
        vec!["-Si".to_owned(), id.to_owned()]
    }

    /// `-S --noconfirm <id>`; AUR ids get the marker shape instead.
    fn install_args(&self, id: &str, _options: &InstallOptions) -> Vec<String> {
        match aur_name(id) {
            Some(name) => vec![
                AUR_MARKER.to_owned(),
                "-S".to_owned(),
                "--noconfirm".to_owned(),
                name.to_owned(),
            ],
            None => vec!["-S".to_owned(), "--noconfirm".to_owned(), id.to_owned()],
        }
    }

    /// `-R --noconfirm <name>` — the `aur/` prefix is stripped: once built,
    /// an AUR package is an ordinary entry in pacman's local database.
    fn uninstall_args(&self, id: &str) -> Vec<String> {
        let name = aur_name(id).unwrap_or(id);
        vec!["-R".to_owned(), "--noconfirm".to_owned(), name.to_owned()]
    }

    /// pacman has no upgrade-one: `-S <id>` installs or upgrades. AUR ids
    /// get the marker shape (yay rebuilds against the new PKGBUILD).
    fn upgrade_args(&self, id: &str) -> Vec<String> {
        self.install_args(id, &InstallOptions::default())
    }

    /// `-Syu --noconfirm` — repo packages only; AUR updates keep their own
    /// per-package queue items (yay cannot be elevated as a batch).
    fn upgrade_all_args(&self) -> Vec<String> {
        vec!["-Syu".to_owned(), "--noconfirm".to_owned()]
    }

    /// pacman repos live in root-owned `/etc/pacman.conf`; the UI hides the
    /// whole sources surface, so these are unreachable.
    fn source_add_args(&self, _name: &str, _url: &str) -> Vec<String> {
        Vec::new()
    }

    fn source_edit_args(&self, _name: &str, _explicit: Option<bool>) -> Option<Vec<String>> {
        None
    }

    fn source_remove_args(&self, _name: &str) -> Vec<String> {
        Vec::new()
    }

    /// `-Sy` — refreshes the sync databases.
    fn source_update_args(&self, _name: Option<&str>) -> Vec<String> {
        vec!["-Sy".to_owned()]
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

    /// Repo results from `-Ss`, plus `yay -Ss --aur` hits (id-prefixed by
    /// the parser) when yay is present and the query is non-empty.
    fn search_packages(
        &self,
        query: &str,
        source: Option<&str>,
        filters: SearchFilters,
        count: Option<u32>,
    ) -> Vec<SearchRow> {
        let args = self.search_args(query, source, filters, count);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut rows = self
            .run(&refs)
            .map(|out| self.parse_search(&out.stdout))
            .unwrap_or_default();
        if let Some(yay) = &self.yay
            && !query.is_empty()
            && let Ok(out) = exec::run_process(yay, ENV, &["-Ss", "--aur", query])
        {
            rows.extend(self.parse_search(&out.stdout));
        }
        rows
    }

    /// `-Q` rows, with `-Qmq` (foreign/AUR names) marking each matching
    /// row's id `aur/<name>` so installed state lines up with search rows.
    fn list_installed(&self) -> Vec<ListRow> {
        let mut rows = self
            .run(&["-Q"])
            .map(|out| parse::parse_list(&out.stdout))
            .unwrap_or_default();
        let foreign: HashSet<String> = self
            .run(&["-Qmq"])
            .map(|out| parse::parse_foreign_names(&out.stdout))
            .unwrap_or_default();
        for row in &mut rows {
            if foreign.contains(&row.name) {
                row.id = format!("{AUR_ID_PREFIX}{}", row.name);
                row.source = "aur".to_owned();
            }
        }
        rows
    }

    /// `-Qu` repo rows plus `yay -Qua` AUR rows (id-prefixed here — yay's
    /// update lines carry no repo marker). Each half tolerates failure.
    fn list_upgrades(&self) -> Vec<ListRow> {
        let mut rows = self
            .run(&["-Qu"])
            .map(|out| parse::parse_upgrades(&out.stdout))
            .unwrap_or_default();
        if let Some(yay) = &self.yay
            && let Ok(out) = exec::run_process(yay, ENV, &["-Qua"])
        {
            for mut row in parse::parse_upgrades(&out.stdout) {
                row.id = format!("{AUR_ID_PREFIX}{}", row.name);
                row.source = "aur".to_owned();
                rows.push(row);
            }
        }
        rows
    }

    /// AUR ids query `yay -Si`; repo ids query `pacman -Si` with a `-Qi`
    /// fallback for installed packages missing from the sync db. A failed
    /// match parses to an empty detail and is dropped.
    fn show_package(&self, id: &str, _source: Option<&str>) -> Option<PackageDetail> {
        let present = |detail: &PackageDetail| {
            !detail.version.is_empty()
                || !detail.publisher.is_empty()
                || !detail.description.is_empty()
        };
        if let Some(name) = aur_name(id) {
            let yay = self.yay.as_deref()?;
            let out = exec::run_process(yay, ENV, &["-Si", name]).ok()?;
            let mut detail = parse::parse_show(&out.stdout);
            // Keep the prefixed id so cache keys and merges line up.
            detail.id = id.to_owned();
            return present(&detail).then_some(detail);
        }
        for args in [&["-Si", id][..], &["-Qi", id][..]] {
            if let Ok(out) = self.run(args)
                && out.status == 0
            {
                let detail = parse::parse_show(&out.stdout);
                if present(&detail) {
                    return Some(detail);
                }
            }
        }
        None
    }

    /// pacman repos are not surfaced (`sources_view` is off).
    fn list_sources(&self) -> Result<Vec<Source>, BackendError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pacman() -> Pacman {
        Pacman::new("pacman", None)
    }

    fn pacman_with_yay() -> Pacman {
        Pacman::new("pacman", Some(PathBuf::from("yay")))
    }

    #[test]
    fn mutation_args_cover_repo_and_aur_shapes() {
        let b = pacman_with_yay();
        assert_eq!(
            b.install_args("curl", &InstallOptions::default()),
            vec!["-S", "--noconfirm", "curl"]
        );
        assert_eq!(
            b.install_args("aur/firefox-nightly", &InstallOptions::default()),
            vec!["--from-aur", "-S", "--noconfirm", "firefox-nightly"]
        );
        assert_eq!(b.uninstall_args("curl"), vec!["-R", "--noconfirm", "curl"]);
        // AUR packages are ordinary local-db entries once built.
        assert_eq!(
            b.uninstall_args("aur/firefox-nightly"),
            vec!["-R", "--noconfirm", "firefox-nightly"]
        );
        assert_eq!(b.upgrade_args("curl"), vec!["-S", "--noconfirm", "curl"]);
        assert_eq!(
            b.upgrade_args("aur/firefox-nightly"),
            vec!["--from-aur", "-S", "--noconfirm", "firefox-nightly"]
        );
        assert_eq!(b.upgrade_all_args(), vec!["-Syu", "--noconfirm"]);
        assert_eq!(b.source_update_args(None), vec!["-Sy"]);
    }

    #[test]
    fn query_args_and_the_empty_query_catalog() {
        let b = pacman();
        assert!(b.empty_query_searches_catalog());
        assert_eq!(
            b.search_args("", None, SearchFilters::default(), None),
            vec!["-Ss", ""]
        );
        assert_eq!(
            b.search_args(
                "firefox",
                Some("ignored"),
                SearchFilters::default(),
                Some(10)
            ),
            vec!["-Ss", "firefox"]
        );
        assert_eq!(b.list_args(None), vec!["-Q"]);
        assert_eq!(b.upgrades_args(), vec!["-Qu"]);
        assert_eq!(b.show_args("curl", None), vec!["-Si", "curl"]);
    }

    #[test]
    fn sources_are_out_of_scope() {
        let b = pacman();
        assert_eq!(
            b.source_add_args("x", "https://example.com"),
            Vec::<String>::new()
        );
        assert_eq!(b.source_edit_args("x", Some(true)), None);
        assert_eq!(b.source_remove_args("x"), Vec::<String>::new());
    }

    /// Every non-marker mutation shape must be plannable as a PackageKit
    /// operation — otherwise that operation silently drops to per-task
    /// pkexec prompts. Marker shapes must NOT plan: an AUR install routed
    /// to pkexec pacman would be nonsense.
    #[cfg(target_os = "linux")]
    #[test]
    fn mutation_shapes_are_packagekit_plannable() {
        use crate::backends::packagekit;
        let b = pacman_with_yay();
        let shapes: Vec<Vec<String>> = vec![
            b.install_args("curl", &InstallOptions::default()),
            b.uninstall_args("curl"),
            b.uninstall_args("aur/firefox-nightly"),
            b.upgrade_args("curl"),
            b.upgrade_all_args(),
            b.source_update_args(None),
        ];
        for shape in shapes {
            let refs: Vec<&str> = shape.iter().map(String::as_str).collect();
            assert!(packagekit::plan(&refs).is_some(), "unplannable: {refs:?}");
        }
        let aur_shapes: Vec<Vec<String>> = vec![
            b.install_args("aur/firefox-nightly", &InstallOptions::default()),
            b.upgrade_args("aur/firefox-nightly"),
        ];
        for shape in aur_shapes {
            let refs: Vec<&str> = shape.iter().map(String::as_str).collect();
            assert!(packagekit::plan(&refs).is_none(), "must not plan: {refs:?}");
        }
    }

    #[test]
    fn read_only_routing_covers_queries_only() {
        assert!(is_read_only(&["-Ss", "firefox"]));
        assert!(is_read_only(&["-Q"]));
        assert!(is_read_only(&["-Qu"]));
        assert!(is_read_only(&["-Qmq"]));
        assert!(is_read_only(&["-Si", "curl"]));
        assert!(is_read_only(&["-Qi", "curl"]));
        assert!(is_read_only(&["--version"]));
        assert!(!is_read_only(&["-S", "--noconfirm", "curl"]));
        assert!(!is_read_only(&["-R", "--noconfirm", "curl"]));
        assert!(!is_read_only(&["-Syu", "--noconfirm"]));
        assert!(!is_read_only(&["-Sy"]));
        assert!(!is_read_only(&["--from-aur", "-S", "--noconfirm", "x"]));
        assert!(!is_read_only(&[]));
    }

    #[test]
    fn capabilities_hide_the_entire_winget_surface() {
        let caps = pacman().capabilities();
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
        assert_eq!(pacman().format_exit_code(1), "1");
        assert_eq!(pacman().format_exit_code(127), "127");
    }

    #[test]
    fn aur_name_strips_only_the_prefix() {
        assert_eq!(aur_name("aur/firefox-nightly"), Some("firefox-nightly"));
        assert_eq!(aur_name("firefox"), None);
        assert_eq!(aur_name("aur/"), None);
    }

    #[test]
    fn an_invalid_configured_path_is_rejected() {
        let result = Pacman::resolve("/definitely/not/a/pacman");
        assert!(matches!(
            result,
            Err(BackendError::InvalidPath("pacman", _))
        ));
    }
}
