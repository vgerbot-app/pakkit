//! Remote, explicitly user-confirmed official software installations.
//!
//! This module is deliberately independent of GPUI and package-manager
//! backends. A catalog is accepted only as a whole; no entry from an invalid
//! document is executed or displayed.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::backends::exec;

pub const DEFAULT_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/vgerbot-app/winget-gui/official-install/catalog.json";
const CATALOG_LIMIT: u64 = 4 * 1024 * 1024;
const DOWNLOAD_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const OFFICIAL_CATEGORY_ICONS: &[&str] = &[
    "code", "cpu", "driver", "file", "flask", "gamepad", "media", "network", "office", "package",
    "shield", "wrench",
];

#[derive(Debug, thiserror::Error)]
pub enum OfficialError {
    #[error("目录 JSON 无效：{0}")]
    Json(#[from] serde_json::Error),
    #[error("目录校验失败：{0}")]
    Validation(String),
    #[error("目录请求失败：{0}")]
    CatalogRequest(String),
    #[error("下载失败：{0}")]
    Download(String),
    #[error("文件操作失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("进程执行失败：{0}")]
    Process(String),
    #[error("无法打开目标：{0}")]
    Open(#[source] std::io::Error),
}

impl OfficialError {
    pub fn localized(&self) -> String {
        self.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialCatalog {
    pub schema_version: u32,
    pub catalog_version: String,
    pub homepage: String,
    pub submission_url: String,
    pub categories: Vec<OfficialCategory>,
    pub apps: Vec<OfficialApp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficialCategory {
    pub id: String,
    pub name: LocalizedText,
    pub description: LocalizedText,
    pub icon: String,
    pub accent: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizedText {
    #[serde(rename = "zh-CN")]
    pub zh_cn: String,
    #[serde(rename = "zh-TW")]
    pub zh_tw: String,
    pub en: String,
    pub ja: String,
    pub ko: String,
}

impl LocalizedText {
    pub fn get(&self, language: &str) -> &str {
        match language {
            "zh-TW" => &self.zh_tw,
            "en" => &self.en,
            "ja" => &self.ja,
            "ko" => &self.ko,
            _ => &self.zh_cn,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficialApp {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub category: String,
    pub summary: LocalizedText,
    pub homepage: String,
    pub mark: String,
    pub accent: String,
    pub variants: Vec<OfficialVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialVariant {
    pub id: String,
    pub os: OfficialOs,
    pub architectures: Vec<OfficialArch>,
    pub requires_admin: bool,
    pub interactive: bool,
    pub note: LocalizedText,
    pub source_url: String,
    pub method: OfficialMethod,
    #[serde(default)]
    pub detection: Option<OfficialDetection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficialOs {
    Linux,
    Windows,
    Macos,
}

impl OfficialOs {
    pub const ALL: [Self; 3] = [Self::Linux, Self::Windows, Self::Macos];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Linux => "Linux",
            Self::Windows => "Windows",
            Self::Macos => "macOS",
        }
    }
    pub fn host() -> Option<Self> {
        match std::env::consts::OS {
            "linux" => Some(Self::Linux),
            "windows" => Some(Self::Windows),
            "macos" => Some(Self::Macos),
            _ => None,
        }
    }
}

impl std::fmt::Display for OfficialOs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficialArch {
    X86_64,
    Aarch64,
}

impl OfficialArch {
    pub const ALL: [Self; 2] = [Self::X86_64, Self::Aarch64];
    pub const fn label(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "ARM64",
        }
    }
    pub fn host() -> Option<Self> {
        match std::env::consts::ARCH {
            "x86_64" => Some(Self::X86_64),
            "aarch64" => Some(Self::Aarch64),
            _ => None,
        }
    }
}

impl std::fmt::Display for OfficialArch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OfficialMethod {
    Script {
        shell: OfficialShell,
        command: String,
    },
    #[serde(rename_all = "camelCase")]
    Artifact {
        url: String,
        file_name: String,
        action: ArtifactAction,
        #[serde(default)]
        sha256: Option<String>,
        #[serde(default)]
        make_executable: Option<bool>,
    },
    OfficialPage {
        url: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficialShell {
    Sh,
    Powershell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactAction {
    RunInstaller,
    DownloadPackage,
    DownloadPortable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OfficialDetection {
    Command { program: String, args: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficialInstallState {
    Checking,
    Installed,
    NotInstalled,
    Unknown,
}

impl OfficialInstallState {
    pub fn label(self) -> String {
        match self {
            Self::Checking => tr!("official.state.checking"),
            Self::Installed => tr!("pkg.installed"),
            Self::NotInstalled => tr!("pkg.not_installed"),
            Self::Unknown => tr!("official.state.unknown"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfficialLoadState {
    Loading,
    Ready,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OfficialCatalogState {
    pub status: OfficialLoadState,
    pub catalog: Option<OfficialCatalog>,
    pub query: String,
    pub category: Option<String>,
    pub os: Option<OfficialOs>,
    pub arch: Option<OfficialArch>,
    pub page: usize,
    pub installed: std::collections::HashMap<String, OfficialInstallState>,
}

impl Default for OfficialCatalogState {
    fn default() -> Self {
        Self {
            status: OfficialLoadState::Loading,
            catalog: None,
            query: String::new(),
            category: None,
            os: OfficialOs::host(),
            arch: OfficialArch::host(),
            page: 0,
            installed: std::collections::HashMap::new(),
        }
    }
}

pub fn parse_catalog(input: &str) -> Result<OfficialCatalog, OfficialError> {
    let catalog: OfficialCatalog = serde_json::from_str(input)?;
    validate_catalog(&catalog)?;
    Ok(catalog)
}

fn validate_catalog(catalog: &OfficialCatalog) -> Result<(), OfficialError> {
    let invalid = |message: String| Err(OfficialError::Validation(message));
    if catalog.schema_version != 1 {
        return invalid(format!("不支持的 schemaVersion {}", catalog.schema_version));
    }
    if catalog.catalog_version.trim().is_empty() {
        return invalid("catalogVersion 不能为空".to_owned());
    }
    validate_https(&catalog.homepage)?;
    validate_https(&catalog.submission_url)?;
    let categories: HashSet<&str> = catalog.categories.iter().map(|c| c.id.as_str()).collect();
    if categories.len() != catalog.categories.len() {
        return invalid("重复分类 ID".to_owned());
    }
    for category in &catalog.categories {
        if !valid_id(&category.id) {
            return invalid(format!("无效分类 ID {}", category.id));
        }
        if !OFFICIAL_CATEGORY_ICONS.contains(&category.icon.as_str()) {
            return invalid(format!("分类 {} 使用了未知图标", category.id));
        }
        validate_accent(&category.accent)?;
    }
    let mut app_ids = HashSet::new();
    let mut variant_ids = HashSet::new();
    let mut coverage = HashSet::new();
    for app in &catalog.apps {
        if !valid_id(&app.id) || !app_ids.insert(&app.id) {
            return invalid(format!("无效或重复软件 ID {}", app.id));
        }
        if !categories.contains(app.category.as_str()) {
            return invalid(format!("软件 {} 的分类未定义", app.id));
        }
        if app.name.trim().is_empty() || app.publisher.trim().is_empty() {
            return invalid(format!("软件 {} 名称和开发者不能为空", app.id));
        }
        if app.mark.is_empty() || app.mark.chars().count() > 2 {
            return invalid(format!("软件 {} 的 mark 无效", app.id));
        }
        validate_https(&app.homepage)?;
        validate_accent(&app.accent)?;
        if app.variants.is_empty() {
            return invalid(format!("软件 {} 没有变体", app.id));
        }
        for variant in &app.variants {
            if !valid_id(&variant.id) || !variant_ids.insert(&variant.id) {
                return invalid(format!("无效或重复变体 ID {}", variant.id));
            }
            if variant.architectures.is_empty() {
                return invalid(format!("变体 {} 没有架构", variant.id));
            }
            for arch in &variant.architectures {
                if !coverage.insert((&app.id, variant.os, *arch)) {
                    return invalid(format!("软件 {} 的 OS/架构变体重复", app.id));
                }
            }
            validate_https(&variant.source_url)?;
            match &variant.method {
                OfficialMethod::Script { shell, command } => {
                    if command.trim().is_empty() {
                        return invalid(format!("变体 {} 的命令为空", variant.id));
                    }
                    let contains_sudo = command
                        .split(|c: char| c.is_whitespace() || "\"'();|&".contains(c))
                        .any(|part| part == "sudo");
                    if contains_sudo && (!variant.interactive || !variant.requires_admin) {
                        return invalid(format!(
                            "变体 {} 的 sudo 命令必须交互并请求权限",
                            variant.id
                        ));
                    }
                    if variant.os == OfficialOs::Windows && *shell != OfficialShell::Powershell
                        || variant.os != OfficialOs::Windows && *shell != OfficialShell::Sh
                    {
                        return invalid(format!("变体 {} 的 shell 与操作系统不匹配", variant.id));
                    }
                }
                OfficialMethod::Artifact {
                    url,
                    file_name,
                    action,
                    sha256,
                    ..
                } => {
                    validate_https(url)?;
                    validate_file_name(file_name)?;
                    if let Some(hash) = sha256
                        && (hash.len() != 64
                            || !hash
                                .bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
                    {
                        return invalid(format!("变体 {} 的 SHA-256 无效", variant.id));
                    }
                    if *action == ArtifactAction::RunInstaller {
                        let extension = Path::new(file_name)
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or_default()
                            .to_ascii_lowercase();
                        let valid = match variant.os {
                            OfficialOs::Windows => matches!(extension.as_str(), "exe" | "msi"),
                            OfficialOs::Macos => matches!(extension.as_str(), "pkg" | "dmg"),
                            OfficialOs::Linux => false,
                        };
                        if !valid {
                            return invalid(format!("变体 {} 不允许运行此安装器", variant.id));
                        }
                    }
                }
                OfficialMethod::OfficialPage { url } => validate_https(url)?,
            }
        }
    }
    Ok(())
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn validate_https(value: &str) -> Result<(), OfficialError> {
    let parsed = url::Url::parse(value)
        .map_err(|_| OfficialError::Validation(format!("无效 URL {value}")))?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err(OfficialError::Validation(format!(
            "仅接受 HTTPS URL {value}"
        )));
    }
    Ok(())
}

fn validate_file_name(name: &str) -> Result<(), OfficialError> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\'])
        || Path::new(name).file_name().and_then(|s| s.to_str()) != Some(name)
    {
        return Err(OfficialError::Validation(format!("非法文件名 {name}")));
    }
    Ok(())
}

fn validate_accent(value: &str) -> Result<(), OfficialError> {
    if value.len() != 7
        || !value.starts_with('#')
        || !value[1..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(OfficialError::Validation(format!("无效颜色 {value}")));
    }
    Ok(())
}

pub fn variant_for(
    app: &OfficialApp,
    os: OfficialOs,
    arch: OfficialArch,
) -> Option<&OfficialVariant> {
    app.variants
        .iter()
        .find(|v| v.os == os && v.architectures.contains(&arch))
}

pub fn visible_official_apps(state: &OfficialCatalogState) -> Vec<&OfficialApp> {
    visible_official_apps_for_language(state, "en")
}

/// Local filtering lives here so it never touches backend search. All
/// localized summaries participate, allowing regional aliases to work even
/// when they are not in the active UI language.
pub fn visible_official_apps_for_language<'a>(
    state: &'a OfficialCatalogState,
    _language: &str,
) -> Vec<&'a OfficialApp> {
    let Some(catalog) = &state.catalog else {
        return Vec::new();
    };
    let (Some(os), Some(arch)) = (state.os, state.arch) else {
        return Vec::new();
    };
    let query = state.query.trim().to_lowercase();
    catalog
        .apps
        .iter()
        .filter(|app| state.category.as_ref().is_none_or(|c| c == &app.category))
        .filter(|app| variant_for(app, os, arch).is_some())
        .filter(|app| {
            query.is_empty()
                || [
                    app.id.as_str(),
                    app.name.as_str(),
                    app.publisher.as_str(),
                    app.summary.zh_cn.as_str(),
                    app.summary.zh_tw.as_str(),
                    app.summary.en.as_str(),
                    app.summary.ja.as_str(),
                    app.summary.ko.as_str(),
                ]
                .iter()
                .any(|text| text.to_lowercase().contains(&query))
        })
        .collect()
}

/// Owned detection work for the currently rendered page. Any cached state,
/// including `Checking`, suppresses duplicate process launches.
pub fn pending_official_detections(
    apps: &[&OfficialApp],
    host_os: OfficialOs,
    host_arch: OfficialArch,
    installed: &std::collections::HashMap<String, OfficialInstallState>,
) -> Vec<(String, OfficialDetection)> {
    apps.iter()
        .filter_map(|app| variant_for(app, host_os, host_arch))
        .filter(|variant| !installed.contains_key(&variant.id))
        .filter_map(|variant| {
            variant
                .detection
                .clone()
                .map(|detection| (variant.id.clone(), detection))
        })
        .collect()
}

pub fn catalog_url() -> String {
    std::env::var("PAKKIT_OFFICIAL_CATALOG_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CATALOG_URL.to_owned())
}

fn agent(timeout: Duration, proxy: &str) -> Result<ureq::Agent, OfficialError> {
    let mut config = ureq::Agent::config_builder().timeout_global(Some(timeout));
    if !proxy.trim().is_empty() {
        let parsed = ureq::Proxy::new(proxy)
            .map_err(|error| OfficialError::CatalogRequest(error.to_string()))?;
        config = config.proxy(Some(parsed));
    }
    Ok(config.build().into())
}

fn valid_catalog_scheme(url: &url::Url, development: bool) -> bool {
    if url.scheme() == "https" {
        return true;
    }
    development
        && url.scheme() == "http"
        && url.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        })
}

pub fn fetch_catalog(url: &str, proxy: &str) -> Result<OfficialCatalog, OfficialError> {
    let parsed =
        url::Url::parse(url).map_err(|error| OfficialError::CatalogRequest(error.to_string()))?;
    if !valid_catalog_scheme(
        &parsed,
        std::env::var_os("PAKKIT_OFFICIAL_CATALOG_URL").is_some(),
    ) {
        return Err(OfficialError::CatalogRequest(
            "目录只接受 HTTPS；开发 HTTP 必须使用显式本机覆盖地址".to_owned(),
        ));
    }
    let agent = agent(Duration::from_secs(15), proxy)?;
    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| OfficialError::CatalogRequest(error.to_string()))?;
    if !response.status().is_success() {
        return Err(OfficialError::CatalogRequest(format!(
            "HTTP {}",
            response.status()
        )));
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(CATALOG_LIMIT)
        .read_to_string()
        .map_err(|error| OfficialError::CatalogRequest(error.to_string()))?;
    parse_catalog(&body)
}

pub fn detect_installation(detection: Option<&OfficialDetection>) -> OfficialInstallState {
    let Some(OfficialDetection::Command { program, args }) = detection else {
        return OfficialInstallState::Unknown;
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match exec::run_process(Path::new(program), &[], &args) {
        Ok(output) if output.status == 0 && !output.signal_terminated => {
            OfficialInstallState::Installed
        }
        Ok(output) if output.signal_terminated => OfficialInstallState::Unknown,
        Ok(_) | Err(_) => OfficialInstallState::NotInstalled,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfficialOutcome {
    ScriptCompleted { log: String },
    TerminalOpened,
    InstallerStarted { path: PathBuf },
    PackageDownloaded { path: PathBuf },
    PortableDownloaded { path: PathBuf },
    OfficialPageOpened,
}

impl OfficialOutcome {
    pub fn downloaded_path(&self) -> Option<&Path> {
        match self {
            Self::InstallerStarted { path }
            | Self::PackageDownloaded { path }
            | Self::PortableDownloaded { path } => Some(path),
            _ => None,
        }
    }
}

pub fn run_method(
    method: &OfficialMethod,
    interactive: bool,
    proxy: &str,
    allow_http: bool,
) -> Result<OfficialOutcome, OfficialError> {
    match method {
        OfficialMethod::Script { shell, command } => {
            if interactive {
                open_terminal(*shell, command)?;
                return Ok(OfficialOutcome::TerminalOpened);
            }
            let (program, args) = match shell {
                OfficialShell::Sh => ("sh", vec!["-c", command.as_str()]),
                OfficialShell::Powershell => (
                    "powershell",
                    vec![
                        "-NoProfile",
                        "-NonInteractive",
                        "-Command",
                        command.as_str(),
                    ],
                ),
            };
            let output = exec::run_process(Path::new(program), &[], &args)
                .map_err(|error| OfficialError::Process(error.localized()))?;
            let log = output.combined();
            if output.status != 0 || output.signal_terminated {
                return Err(OfficialError::Process(format!(
                    "退出码 {}{}",
                    output.status,
                    if log.is_empty() {
                        String::new()
                    } else {
                        format!("\n{log}")
                    }
                )));
            }
            Ok(OfficialOutcome::ScriptCompleted { log })
        }
        OfficialMethod::Artifact {
            url,
            file_name,
            action,
            sha256,
            make_executable: executable,
        } => {
            let path = download_artifact(url, file_name, sha256.as_deref(), proxy, allow_http)?;
            if *executable == Some(true) {
                make_executable(&path)?;
            }
            match action {
                ArtifactAction::RunInstaller => {
                    launch_installer(&path)?;
                    Ok(OfficialOutcome::InstallerStarted { path })
                }
                ArtifactAction::DownloadPackage => Ok(OfficialOutcome::PackageDownloaded { path }),
                ArtifactAction::DownloadPortable => {
                    Ok(OfficialOutcome::PortableDownloaded { path })
                }
            }
        }
        OfficialMethod::OfficialPage { url } => {
            open_path(Path::new(url))?;
            Ok(OfficialOutcome::OfficialPageOpened)
        }
    }
}

fn open_terminal(shell: OfficialShell, command: &str) -> Result<(), OfficialError> {
    #[cfg(target_os = "linux")]
    {
        if shell != OfficialShell::Sh {
            return Err(OfficialError::Process(
                "Linux 仅支持 sh 交互终端".to_owned(),
            ));
        }
        for (program, prefix) in [
            ("x-terminal-emulator", vec!["-e"]),
            ("gnome-terminal", vec!["--"]),
            ("konsole", vec!["-e"]),
            ("xterm", vec!["-e"]),
        ] {
            let mut args = prefix;
            args.extend(["sh", "-lc", command]);
            match Command::new(program)
                .args(args)
                .stdin(Stdio::null())
                .spawn()
            {
                Ok(_) => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(OfficialError::Open(error)),
            }
        }
        return Err(OfficialError::Process("未找到可用图形终端".to_owned()));
    }
    #[cfg(target_os = "macos")]
    {
        let script = format!("tell application \"Terminal\" to do script {:?}", command);
        Command::new("osascript")
            .args(["-e", &script])
            .spawn()
            .map_err(OfficialError::Open)?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args([
                "/c",
                "start",
                "",
                "powershell",
                "-NoExit",
                "-NoProfile",
                "-Command",
                command,
            ])
            .spawn()
            .map_err(OfficialError::Open)?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Err(OfficialError::Process("此平台不支持交互终端".to_owned()))
}

pub fn download_artifact(
    url: &str,
    file_name: &str,
    expected_sha256: Option<&str>,
    proxy: &str,
    allow_http: bool,
) -> Result<PathBuf, OfficialError> {
    if allow_http {
        let parsed =
            url::Url::parse(url).map_err(|error| OfficialError::Download(error.to_string()))?;
        if parsed.scheme() == "http" {
            let loopback = parsed.host_str().is_some_and(|host| {
                host.eq_ignore_ascii_case("localhost")
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            });
            if !loopback {
                return Err(OfficialError::Download(
                    "开发 HTTP 下载只允许本机地址".to_owned(),
                ));
            }
        } else {
            validate_https(url)?;
        }
    } else {
        validate_https(url)?;
    }
    validate_file_name(file_name)?;
    let agent = agent(Duration::from_secs(60), proxy)?;
    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| OfficialError::Download(error.to_string()))?;
    if !response.status().is_success() {
        return Err(OfficialError::Download(format!(
            "HTTP {}",
            response.status()
        )));
    }
    let dir = dirs::download_dir()
        .ok_or_else(|| OfficialError::Download("无法确定下载目录".to_owned()))?
        .join("Pakkit");
    std::fs::create_dir_all(&dir)?;
    let final_path = unique_path(&dir, file_name);
    let temp_path = final_path.with_extension(format!(
        "{}.part",
        final_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
    ));
    let result = write_download(&mut response, &temp_path, expected_sha256);
    match result {
        Ok(()) => {
            std::fs::rename(&temp_path, &final_path)?;
            Ok(final_path)
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temp_path);
            Err(error)
        }
    }
}

fn write_download(
    response: &mut ureq::http::Response<ureq::Body>,
    path: &Path,
    expected_sha256: Option<&str>,
) -> Result<(), OfficialError> {
    let mut reader = response.body_mut().as_reader();
    let mut file = std::fs::File::create(path)?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > DOWNLOAD_LIMIT {
            return Err(OfficialError::Download("文件超过 2 GiB 上限".to_owned()));
        }
        std::io::Write::write_all(&mut file, &buffer[..count])?;
        hasher.update(&buffer[..count]);
    }
    file.sync_all()?;
    if let Some(expected) = expected_sha256 {
        let actual = hasher.finalize();
        let mut actual_hex = String::with_capacity(64);
        for byte in actual {
            let _ = write!(actual_hex, "{byte:02x}");
        }
        if actual_hex != expected {
            return Err(OfficialError::Download(format!(
                "SHA-256 不匹配：期望 {expected}，实际 {actual_hex}"
            )));
        }
    }
    Ok(())
}

fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let original = dir.join(file_name);
    if !original.exists() {
        return original;
    }
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);
    let extension = Path::new(file_name).extension().and_then(|s| s.to_str());
    for index in 1_u64.. {
        let name = match extension {
            Some(extension) => format!("{stem}-{index}.{extension}"),
            None => format!("{stem}-{index}"),
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), OfficialError> {
    use std::os::unix::fs::PermissionsExt as _;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(permissions.mode() | 0o100);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), OfficialError> {
    Ok(())
}

fn launch_installer(path: &Path) -> Result<(), OfficialError> {
    #[cfg(target_os = "windows")]
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    #[cfg(target_os = "windows")]
    if extension.eq_ignore_ascii_case("msi") {
        Command::new("msiexec")
            .arg("/i")
            .arg(path)
            .spawn()
            .map_err(OfficialError::Open)?;
        return Ok(());
    }
    open_path(path)
}

pub fn open_path(path: &Path) -> Result<(), OfficialError> {
    #[cfg(target_os = "linux")]
    let result = Command::new("xdg-open").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let result = Command::new("cmd")
        .args(["/c", "start", ""])
        .arg(path)
        .spawn();
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let result: std::io::Result<std::process::Child> =
        Err(std::io::Error::other("no opener on this platform"));
    result.map(|_| ()).map_err(OfficialError::Open)
}

pub fn submission_url(
    base: &str,
    name: &str,
    link: &str,
    note: &str,
) -> Result<String, OfficialError> {
    if name.trim().is_empty() {
        return Err(OfficialError::Validation("软件名称不能为空".to_owned()));
    }
    validate_https(link)?;
    let mut url =
        url::Url::parse(base).map_err(|error| OfficialError::Validation(error.to_string()))?;
    if url.scheme() != "https" {
        return Err(OfficialError::Validation("提交地址必须为 HTTPS".to_owned()));
    }
    let body = format!("软件名称：{name}\n官方链接：{link}\n\n补充说明：\n{note}");
    url.query_pairs_mut()
        .append_pair("title", &format!("官网直装软件提交：{name}"))
        .append_pair("body", &body);
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text() -> LocalizedText {
        LocalizedText {
            zh_cn: "中文".to_owned(),
            zh_tw: "繁體".to_owned(),
            en: "English".to_owned(),
            ja: "日本語".to_owned(),
            ko: "한국어".to_owned(),
        }
    }

    fn fixture() -> String {
        serde_json::json!({
            "schemaVersion": 1,
            "catalogVersion": "test",
            "homepage": "https://example.com/catalog",
            "submissionUrl": "https://example.com/issues/new",
            "categories": [{"id":"dev","name":text(),"description":text(),"icon":"code","accent":"#4f8cff"}],
            "apps": [{
                "id":"test-app", "name":"Test App", "publisher":"Publisher", "category":"dev",
                "summary":text(), "homepage":"https://example.com", "mark":"T", "accent":"#70d6a2",
                "variants": [
                    {"id":"test-script","os":"linux","architectures":["x86_64","aarch64"],"requiresAdmin":false,"interactive":false,"note":text(),"sourceUrl":"https://example.com/install","method":{"type":"script","shell":"sh","command":"printf safe"},"detection":{"type":"command","program":"sh","args":["-c","exit 0"]}},
                    {"id":"test-artifact","os":"windows","architectures":["x86_64"],"requiresAdmin":false,"interactive":false,"note":text(),"sourceUrl":"https://example.com/install","method":{"type":"artifact","url":"https://example.com/test.exe","fileName":"test.exe","action":"run_installer","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}},
                    {"id":"test-page","os":"macos","architectures":["aarch64"],"requiresAdmin":false,"interactive":false,"note":text(),"sourceUrl":"https://example.com/install","method":{"type":"official_page","url":"https://example.com/releases"}}
                ]
            }]
        }).to_string()
    }

    #[test]
    fn valid_catalog_supports_all_methods_and_architectures() {
        let catalog = parse_catalog(&fixture()).expect("valid fixture");
        assert_eq!(catalog.apps.len(), 1);
        assert!(variant_for(&catalog.apps[0], OfficialOs::Linux, OfficialArch::Aarch64).is_some());
        assert_eq!(catalog.apps[0].summary.get("zh-CN"), "中文");
    }
    #[test]
    fn malformed_catalog_and_unmatched_filters_return_no_entries() {
        assert!(parse_catalog("{").is_err());
        let catalog = parse_catalog(&fixture()).expect("valid fixture");
        let mut state = OfficialCatalogState {
            status: OfficialLoadState::Ready,
            catalog: Some(catalog),
            query: "not-present".to_owned(),
            ..OfficialCatalogState::default()
        };
        assert!(visible_official_apps(&state).is_empty());
        state.query.clear();
        state.os = Some(OfficialOs::Windows);
        state.arch = Some(OfficialArch::Aarch64);
        assert!(visible_official_apps(&state).is_empty());
        state.catalog = None;
        assert!(visible_official_apps(&state).is_empty());
    }

    #[test]
    fn http_catalog_requires_explicit_loopback_development_mode() {
        let local = url::Url::parse("http://127.0.0.1:8765/catalog.json").expect("URL");
        let remote = url::Url::parse("http://example.com/catalog.json").expect("URL");
        let secure = url::Url::parse("https://example.com/catalog.json").expect("URL");
        assert!(!valid_catalog_scheme(&local, false));
        assert!(!valid_catalog_scheme(&remote, true));
        assert!(valid_catalog_scheme(&local, true));
        assert!(valid_catalog_scheme(&secure, false));
    }

    #[test]
    fn windows_arm64_selects_its_own_download_url() {
        let mut doc: serde_json::Value = serde_json::from_str(&fixture()).expect("fixture");
        doc["apps"][0]["variants"][1]["architectures"] = serde_json::json!(["aarch64"]);
        doc["apps"][0]["variants"][1]["method"]["url"] =
            serde_json::json!("https://example.com/arm64.exe");
        let catalog = parse_catalog(&doc.to_string()).expect("valid catalog");
        let variant = variant_for(&catalog.apps[0], OfficialOs::Windows, OfficialArch::Aarch64)
            .expect("variant");
        assert!(
            matches!(&variant.method, OfficialMethod::Artifact { url, .. } if url == "https://example.com/arm64.exe")
        );
    }

    #[test]
    fn invalid_catalog_is_rejected_as_a_whole() {
        for (pointer, value) in [
            ("/schemaVersion", serde_json::json!(2)),
            ("/apps/0/homepage", serde_json::json!("http://example.com")),
            (
                "/apps/0/variants/1/method/fileName",
                serde_json::json!("../test.exe"),
            ),
            ("/apps/0/variants/1/method/sha256", serde_json::json!("ABC")),
        ] {
            let mut value_doc: serde_json::Value =
                serde_json::from_str(&fixture()).expect("fixture");
            *value_doc.pointer_mut(pointer).expect("field") = value;
            assert!(parse_catalog(&value_doc.to_string()).is_err(), "{pointer}");
        }
    }

    #[test]
    fn duplicate_coverage_is_rejected() {
        let mut doc: serde_json::Value = serde_json::from_str(&fixture()).expect("fixture");
        let variant = doc["apps"][0]["variants"][0].clone();
        doc["apps"][0]["variants"]
            .as_array_mut()
            .expect("variants")
            .push(variant);
        assert!(parse_catalog(&doc.to_string()).is_err());
    }

    #[test]
    fn missing_detection_is_unknown_and_command_exit_is_definitive() {
        assert_eq!(detect_installation(None), OfficialInstallState::Unknown);
        let installed = OfficialDetection::Command {
            program: "sh".to_owned(),
            args: vec!["-c".to_owned(), "exit 0".to_owned()],
        };
        let missing = OfficialDetection::Command {
            program: "sh".to_owned(),
            args: vec!["-c".to_owned(), "exit 1".to_owned()],
        };
        let spawn = OfficialDetection::Command {
            program: "/definitely/not/a/program".to_owned(),
            args: Vec::new(),
        };
        assert_eq!(
            detect_installation(Some(&installed)),
            OfficialInstallState::Installed
        );
        assert_eq!(
            detect_installation(Some(&missing)),
            OfficialInstallState::NotInstalled
        );
        assert_eq!(
            detect_installation(Some(&spawn)),
            OfficialInstallState::NotInstalled
        );
    }

    #[test]
    fn submission_fields_are_url_encoded() {
        let url = submission_url(
            "https://example.com/issues/new",
            "My App",
            "https://example.com/get?a=1&b=two",
            "a note",
        )
        .expect("submission");
        let parsed = url::Url::parse(&url).expect("URL");
        let pairs: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(pairs["title"], "官网直装软件提交：My App");
        assert!(pairs["body"].contains("https://example.com/get?a=1&b=two"));
    }

    #[test]
    fn data_driven_categories_still_validate_ids_and_icons() {
        let mut doc: serde_json::Value = serde_json::from_str(&fixture()).expect("fixture");
        doc["categories"][0]["id"] = serde_json::json!("browser");
        doc["categories"][0]["icon"] = serde_json::json!("network");
        doc["apps"][0]["category"] = serde_json::json!("browser");
        assert!(parse_catalog(&doc.to_string()).is_ok());

        doc["categories"][0]["icon"] = serde_json::json!("not-an-icon");
        assert!(parse_catalog(&doc.to_string()).is_err());
        doc["categories"][0]["icon"] = serde_json::json!("network");
        doc["categories"][0]["id"] = serde_json::json!("Browser");
        assert!(parse_catalog(&doc.to_string()).is_err());
    }

    #[test]
    fn search_matches_ids_and_every_localized_summary() {
        let mut doc: serde_json::Value = serde_json::from_str(&fixture()).expect("fixture");
        doc["apps"][0]["summary"]["zh-CN"] = serde_json::json!("微信");
        let catalog = parse_catalog(&doc.to_string()).expect("valid catalog");
        let mut state = OfficialCatalogState {
            status: OfficialLoadState::Ready,
            catalog: Some(catalog),
            os: Some(OfficialOs::Windows),
            arch: Some(OfficialArch::X86_64),
            ..OfficialCatalogState::default()
        };
        for query in ["test-app", "微信", "繁體", "日本語", "한국어"] {
            state.query = query.to_owned();
            assert_eq!(
                visible_official_apps_for_language(&state, "en").len(),
                1,
                "{query}"
            );
        }
    }

    #[test]
    fn official_paging_bounds_large_catalogs_and_clamps_after_filtering() {
        let catalog = parse_catalog(&fixture()).expect("valid catalog");
        let template = catalog.apps[0].clone();
        let apps = (0..389)
            .map(|index| {
                let mut app = template.clone();
                app.id = format!("app-{index}");
                app.name = format!("App {index}");
                for variant in &mut app.variants {
                    variant.id = format!("{}-{}", app.id, variant.id);
                }
                app
            })
            .collect::<Vec<_>>();
        let catalog = OfficialCatalog { apps, ..catalog };
        let mut state = OfficialCatalogState {
            status: OfficialLoadState::Ready,
            catalog: Some(catalog),
            os: Some(OfficialOs::Windows),
            arch: Some(OfficialArch::X86_64),
            ..OfficialCatalogState::default()
        };
        let visible = visible_official_apps(&state);
        let (page, effective, pages) = crate::state::page_slice(&visible, 3);
        assert_eq!((page.len(), effective, pages), (89, 3, 4));
        let (_, effective, _) = crate::state::page_slice(&visible, 99);
        assert_eq!(effective, 3);

        state.query = "App 388".to_owned();
        let filtered = visible_official_apps(&state);
        let (_, effective, pages) = crate::state::page_slice(&filtered, 3);
        assert_eq!((effective, pages), (0, 1));
    }

    #[test]
    fn pending_detection_is_host_only_and_excludes_cached_variants() {
        let catalog = parse_catalog(&fixture()).expect("valid catalog");
        let app = &catalog.apps[0];
        let installed = std::collections::HashMap::new();
        let pending = pending_official_detections(
            &[app],
            OfficialOs::Linux,
            OfficialArch::X86_64,
            &installed,
        );
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, "test-script");
        assert!(
            pending_official_detections(
                &[app],
                OfficialOs::Windows,
                OfficialArch::Aarch64,
                &installed,
            )
            .is_empty()
        );

        for cached in [
            OfficialInstallState::Checking,
            OfficialInstallState::Installed,
            OfficialInstallState::NotInstalled,
            OfficialInstallState::Unknown,
        ] {
            let installed = [("test-script".to_owned(), cached)].into_iter().collect();
            assert!(
                pending_official_detections(
                    &[app],
                    OfficialOs::Linux,
                    OfficialArch::X86_64,
                    &installed,
                )
                .is_empty()
            );
        }

        let mut without_detection = app.clone();
        for variant in &mut without_detection.variants {
            variant.detection = None;
        }
        assert!(
            pending_official_detections(
                &[&without_detection],
                OfficialOs::Linux,
                OfficialArch::X86_64,
                &std::collections::HashMap::new(),
            )
            .is_empty()
        );
    }

    #[test]
    #[ignore = "set PAKKIT_OFFICIAL_CATALOG_PATH to a local official-install catalog"]
    fn real_catalog() {
        let path = std::env::var("PAKKIT_OFFICIAL_CATALOG_PATH")
            .expect("PAKKIT_OFFICIAL_CATALOG_PATH is required");
        let metadata = std::fs::metadata(&path).expect("catalog metadata");
        assert!(metadata.len() <= CATALOG_LIMIT, "catalog exceeds 4 MiB");
        let input = std::fs::read_to_string(&path).expect("read catalog");
        let catalog = parse_catalog(&input).expect("valid real catalog");
        let variants = catalog
            .apps
            .iter()
            .map(|app| app.variants.len())
            .sum::<usize>();
        println!(
            "validated {} categories, {} apps, {variants} variants",
            catalog.categories.len(),
            catalog.apps.len()
        );
    }
}
