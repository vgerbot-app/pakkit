//! Package-manager backends.
//!
//! The app talks to one backend per platform, detected at startup. Each
//! backend module owns its CLI discovery, argument builders and output
//! parsers; [`types`] holds the data shapes they all produce and [`exec`]
//! the shared process plumbing.

pub mod apt;
pub mod chocolatey;
pub mod exec;
pub mod flatpak;
#[cfg(target_os = "linux")]
pub mod packagekit;
pub mod pacman;
pub mod snap;
pub mod types;
pub mod winget;

use std::sync::Arc;

use exec::Output;
use types::{BackendError, InstallOptions, ListRow, PackageDetail, SearchFilters, SearchRow};

use crate::model::Source;

/// The package managers this build can drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Winget,
    Chocolatey,
    Flatpak,
    Apt,
    Pacman,
    Snap,
}

impl BackendKind {
    /// The CLI's executable name, used as the prefix of every command line
    /// the UI shows (`winget install …` / `flatpak install …`).
    pub const fn cli_name(self) -> &'static str {
        match self {
            BackendKind::Winget => "winget",
            BackendKind::Chocolatey => "choco",
            BackendKind::Flatpak => "flatpak",
            BackendKind::Apt => "apt",
            BackendKind::Pacman => "pacman",
            BackendKind::Snap => "snap",
        }
    }

    /// The human-facing product name.
    pub const fn display_name(self) -> &'static str {
        match self {
            BackendKind::Winget => "winget",
            BackendKind::Chocolatey => "Chocolatey",
            BackendKind::Flatpak => "Flatpak",
            BackendKind::Apt => "APT",
            BackendKind::Pacman => "pacman",
            BackendKind::Snap => "Snap",
        }
    }

    /// Parse a persisted [`cli_name`](Self::cli_name) back into a kind.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim() {
            "winget" => Some(BackendKind::Winget),
            "choco" | "chocolatey" => Some(BackendKind::Chocolatey),
            "flatpak" => Some(BackendKind::Flatpak),
            "apt" => Some(BackendKind::Apt),
            "pacman" => Some(BackendKind::Pacman),
            "snap" => Some(BackendKind::Snap),
            _ => None,
        }
    }

    /// The backends this build lets the user pick between, platform default
    /// first. A single-entry list means no choice (and no settings row).
    pub fn available() -> &'static [BackendKind] {
        if cfg!(target_os = "linux") {
            &[
                BackendKind::Flatpak,
                BackendKind::Apt,
                BackendKind::Pacman,
                BackendKind::Snap,
            ]
        } else {
            &[BackendKind::Winget, BackendKind::Chocolatey]
        }
    }
}

/// What a backend can do, for UI feature gating.
///
/// The default is the winget profile (everything on); backends with a
/// narrower surface switch individual flags off and the matching controls
/// disappear instead of failing at run time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// The 源 view exists (sources/remotes can be listed).
    pub sources_view: bool,
    /// Sources can be added.
    pub source_add: bool,
    /// Sources can be edited in place.
    pub source_edit: bool,
    /// The installed set can be exported as a manifest document.
    pub export: bool,
    /// Installs can target an explicit location.
    pub install_location: bool,
    /// Installs can run silently.
    pub silent: bool,
    /// The backend has package/source agreements to accept.
    pub agreements: bool,
    /// The search supports the advanced filter flags.
    pub search_filters: bool,
    /// 全部更新 runs as one backend-wide transaction (`upgrade -y`) instead of
    /// one queue item per package — a single dependency resolution and a
    /// single polkit authorization. Off for winget, which upgrades per id.
    pub batch_upgrade: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            sources_view: true,
            source_add: true,
            source_edit: true,
            export: true,
            install_location: true,
            silent: true,
            agreements: true,
            search_filters: true,
            batch_upgrade: false,
        }
    }
}

/// One package-manager CLI, as the app drives it.
///
/// The `*_args` builders stay pure (and unit-testable) because the queue
/// drawer and detail dialogs display the exact command line; the `parse_*`
/// functions are pure for the same reason. Everything that spawns a process
/// goes through [`PackageBackend::run`].
pub trait PackageBackend: Send + Sync {
    // ---- identity ----
    fn kind(&self) -> BackendKind;

    /// The executable name, prefixing every displayed command line.
    fn cli_name(&self) -> &'static str {
        self.kind().cli_name()
    }

    /// The human-facing product name.
    fn display_name(&self) -> &'static str {
        self.kind().display_name()
    }

    /// What this backend can do; defaults to the full winget profile.
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    /// Whether an empty search query should actually run against the
    /// backend, returning the full catalog into the paged search view
    /// (flatpak, apt). The default keeps winget's behavior: an empty query
    /// stays client-side and shows just the installed set.
    fn empty_query_searches_catalog(&self) -> bool {
        false
    }

    // ---- process plumbing ----
    /// Spawn the CLI with `args`, capturing output.
    fn run(&self, args: &[&str]) -> Result<Output, BackendError>;

    /// Run and fail on a non-benign non-zero exit code.
    fn run_checked(&self, args: &[&str]) -> Result<Output, BackendError> {
        let output = self.run(args)?;
        if output.status == 0 || self.exit_is_benign(output.status) {
            Ok(output)
        } else {
            Err(BackendError::Exit {
                code: output.status,
                message: exec::first_message(&output),
            })
        }
    }

    /// The CLI's own version string, e.g. `v1.29.380`.
    fn version(&self) -> Result<String, BackendError>;

    // ---- exit-code interpretation (winget overrides all four) ----
    /// Human-readable description of a failing exit code.
    fn describe_exit(&self, _code: u32) -> String {
        tr!("exitcode.unknown")
    }

    /// Whether the code means the operation needs an elevated shell.
    fn exit_needs_elevation(&self, _code: u32) -> bool {
        false
    }

    /// Whether the code describes an already-satisfied end state.
    fn exit_is_benign(&self, _code: u32) -> bool {
        false
    }

    /// Format an exit code the way the CLI itself prints it.
    fn format_exit_code(&self, code: u32) -> String {
        format!("{code:#010X}")
    }

    // ---- pure argument builders ----
    fn search_args(
        &self,
        query: &str,
        source: Option<&str>,
        filters: SearchFilters,
        count: Option<u32>,
    ) -> Vec<String>;
    fn list_args(&self, source: Option<&str>) -> Vec<String>;
    fn upgrades_args(&self) -> Vec<String>;
    fn show_args(&self, id: &str, source: Option<&str>) -> Vec<String>;
    fn install_args(&self, id: &str, options: &InstallOptions) -> Vec<String>;
    fn uninstall_args(&self, id: &str) -> Vec<String>;
    fn upgrade_args(&self, id: &str) -> Vec<String>;
    fn upgrade_all_args(&self) -> Vec<String>;
    fn source_add_args(&self, name: &str, url: &str) -> Vec<String>;
    /// `None` when the backend has no in-place source edit (flatpak remotes).
    fn source_edit_args(&self, name: &str, explicit: Option<bool>) -> Option<Vec<String>>;
    fn source_remove_args(&self, name: &str) -> Vec<String>;
    fn source_update_args(&self, name: Option<&str>) -> Vec<String>;

    // ---- pure parsers ----
    fn parse_search(&self, stdout: &str) -> Vec<SearchRow>;
    fn parse_list(&self, stdout: &str) -> Vec<ListRow>;
    fn parse_upgrades(&self, stdout: &str) -> Vec<ListRow>;
    fn parse_show(&self, stdout: &str) -> PackageDetail;

    // ---- semantic composites (defaults compose args + run + parse) ----
    /// `search`, tolerating failure as an empty result set.
    fn search_packages(
        &self,
        query: &str,
        source: Option<&str>,
        filters: SearchFilters,
        count: Option<u32>,
    ) -> Vec<SearchRow> {
        let args = self.search_args(query, source, filters, count);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run(&refs)
            .map(|out| self.parse_search(&out.stdout))
            .unwrap_or_default()
    }

    /// The installed packages, tolerating failure as an empty list.
    fn list_installed(&self) -> Vec<ListRow> {
        let args = self.list_args(None);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run(&refs)
            .map(|out| self.parse_list(&out.stdout))
            .unwrap_or_default()
    }

    /// Installed packages with a newer version available.
    fn list_upgrades(&self) -> Vec<ListRow> {
        let args = self.upgrades_args();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run(&refs)
            .map(|out| self.parse_upgrades(&out.stdout))
            .unwrap_or_default()
    }

    /// `show` metadata for one package.
    ///
    /// A failed match parses to an empty detail and is dropped, so callers
    /// never cache a blank record over a package row.
    fn show_package(&self, id: &str, source: Option<&str>) -> Option<PackageDetail> {
        let args = self.show_args(id, source);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run(&refs)
            .ok()
            .filter(|out| out.status == 0 || self.exit_is_benign(out.status))
            .map(|out| self.parse_show(&out.stdout))
            .filter(|detail| {
                !detail.version.is_empty()
                    || !detail.publisher.is_empty()
                    || !detail.description.is_empty()
            })
    }

    /// The configured sources/remotes, merged with whatever per-source
    /// metadata the backend can read.
    fn list_sources(&self) -> Result<Vec<Source>, BackendError>;

    /// Ids (lowercased) from `rows` that this backend actually manages.
    ///
    /// The default keeps every row — appropriate when the listing contains
    /// only managed packages (flatpak). winget overrides this with its
    /// `export`-file filter, which excludes ARP/MSIX-only entries.
    fn managed_ids(&self, rows: &[ListRow]) -> Vec<String> {
        rows.iter().map(|row| row.id.to_lowercase()).collect()
    }
}

/// The backend kind [`detect`] tries on this platform: winget on Windows,
/// flatpak on Linux (and winget elsewhere until another backend lands).
pub const fn platform_kind() -> BackendKind {
    if cfg!(target_os = "linux") {
        BackendKind::Flatpak
    } else {
        BackendKind::Winget
    }
}

/// Resolve `kind`'s backend, honoring an operator-supplied CLI path.
pub fn detect(
    kind: BackendKind,
    configured_path: &str,
) -> Result<Arc<dyn PackageBackend>, BackendError> {
    match kind {
        BackendKind::Winget => {
            winget::Winget::resolve(configured_path).map(|w| Arc::new(w) as Arc<dyn PackageBackend>)
        }
        BackendKind::Chocolatey => chocolatey::Chocolatey::resolve(configured_path)
            .map(|c| Arc::new(c) as Arc<dyn PackageBackend>),
        BackendKind::Flatpak => flatpak::Flatpak::resolve(configured_path)
            .map(|f| Arc::new(f) as Arc<dyn PackageBackend>),
        BackendKind::Apt => {
            apt::Apt::resolve(configured_path).map(|a| Arc::new(a) as Arc<dyn PackageBackend>)
        }
        BackendKind::Pacman => {
            pacman::Pacman::resolve(configured_path).map(|p| Arc::new(p) as Arc<dyn PackageBackend>)
        }
        BackendKind::Snap => {
            snap::Snap::resolve(configured_path).map(|s| Arc::new(s) as Arc<dyn PackageBackend>)
        }
    }
}
