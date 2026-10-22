//! Backend-neutral data shared by every package-manager backend.
//!
//! These types describe packages, sources and errors the way the UI consumes
//! them; each backend's parser fills in what its CLI provides and leaves the
//! rest empty or `None`. winget-only shapes (`SourceExport` and the trust
//! model) stay in [`crate::backends::winget`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One row of a package search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRow {
    pub name: String,
    pub id: String,
    pub version: String,
    /// How the backend matched this row in the last search, e.g. winget's
    /// `Moniker: vscode`. Backends with plain substring search leave it `None`.
    pub match_kind: Option<String>,
    pub source: String,
    /// A one-line description, when the backend's search output carries one
    /// (flatpak does; winget does not).
    #[serde(default)]
    pub description: Option<String>,
}

/// One row of an installed-packages or upgrades listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRow {
    pub name: String,
    pub id: String,
    pub version: String,
    /// The newer version the backend can move to; `None` when nothing is
    /// available.
    pub available: Option<String>,
    pub source: String,
}

/// One row of a source/remote listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRow {
    pub name: String,
    pub argument: String,
    pub explicit: bool,
}

/// The package metadata the UI renders in the detail dialog.
///
/// This is a superset of what any single backend supplies: winget fills most
/// fields, flatpak only a few. Empty fields render as `—` in the UI.
///
/// `description` and `release_notes` keep their original line breaks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageDetail {
    pub name: String,
    pub id: String,
    pub version: String,
    pub publisher: String,
    pub publisher_url: String,
    pub publisher_support_url: String,
    pub moniker: String,
    pub description: String,
    pub homepage: String,
    pub license: String,
    pub license_url: String,
    pub copyright: String,
    pub release_notes: String,
    pub release_notes_url: String,
    pub tags: Vec<String>,
    pub installer_type: String,
    pub installer_url: String,
    pub installer_sha256: String,
    pub release_date: String,
    pub offline_distribution: String,
}

/// Target CPU architecture for installs that offer one (winget
/// `--architecture`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Arch {
    #[default]
    X64,
    X86,
    Arm64,
}

impl Arch {
    pub const ALL: [Arch; 3] = [Arch::X64, Arch::X86, Arch::Arm64];

    pub const fn as_str(self) -> &'static str {
        match self {
            Arch::X64 => "x64",
            Arch::X86 => "x86",
            Arch::Arm64 => "arm64",
        }
    }
}

/// Install scope for backends that distinguish per-user from machine-wide
/// installs (winget `--scope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Scope {
    #[default]
    User,
    Machine,
}

impl Scope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Scope::User => "user",
            Scope::Machine => "machine",
        }
    }
}

/// Options shared by install and upgrade.
///
/// Backends read the fields they understand: winget honours all of them,
/// flatpak only `source` (as the remote).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstallOptions {
    pub source: Option<String>,
    pub arch: Arch,
    pub scope: Scope,
    pub location: Option<String>,
    pub silent: bool,
    pub accept: bool,
}

/// Filters for advanced search, mirroring the advanced-search dialog.
/// winget maps these to flags; backends without filter support ignore them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchFilters {
    pub exact: bool,
    pub moniker: bool,
    pub tag: bool,
    pub command: bool,
}

/// A source name that means "search every configured source".
pub const SOURCE_ALL: &str = "all";

/// Everything that can go wrong talking to a package-manager CLI.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// The backend's CLI could not be located on this machine.
    #[error("未找到 winget.exe（请确认已安装“应用安装程序”）")]
    NotFound,

    /// The process could not be started at all.
    #[error("无法启动 winget：{0}")]
    Spawn(#[source] std::io::Error),

    /// The CLI's output could not be understood.
    #[error("无法解析 winget 输出：{0}")]
    Parse(String),

    /// The CLI ran and returned a non-benign non-zero exit code.
    #[error("winget 退出码 {code:#010X}：{message}")]
    Exit { code: u32, message: String },

    /// A JSON document the CLI produced was invalid.
    #[error("winget JSON 输出无效：{0}")]
    Json(#[from] serde_json::Error),

    /// A path could not be prepared (e.g. an export temp file).
    #[error("文件操作失败：{0}")]
    Io(#[from] std::io::Error),

    /// An explicit CLI path was configured but does not exist.
    #[error("配置的 winget 路径不存在：{}", .0.display())]
    InvalidPath(PathBuf),
}

impl BackendError {
    /// The message in the current UI language. `Display` keeps the base-locale
    /// text so logs stay greppable regardless of the interface language.
    pub fn localized(&self) -> String {
        match self {
            BackendError::NotFound => tr!("error.not_found"),
            BackendError::Spawn(error) => tr!("error.spawn", error = error.to_string()),
            BackendError::Parse(message) => tr!("error.parse", error = message.clone()),
            BackendError::Exit { code, message } => {
                tr!(
                    "error.exit",
                    code = format!("{code:#010X}"),
                    message = message.clone()
                )
            }
            BackendError::Json(error) => tr!("error.json", error = error.to_string()),
            BackendError::Io(error) => tr!("error.io", error = error.to_string()),
            BackendError::InvalidPath(path) => {
                tr!("error.invalid_path", path = path.display().to_string())
            }
        }
    }
}
