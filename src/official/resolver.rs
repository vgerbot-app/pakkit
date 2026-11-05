//! Provider-neutral release resolution.
//!
//! Providers turn an official release source into one immutable artifact. The
//! confirmation dialog, queue, downloader and platform installers only consume
//! that artifact, so adding another provider does not change their trust rules.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{
    OfficialArch, OfficialError, OfficialOs, OfficialVariant, validate_file_name, validate_https,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OfficialReleaseSource {
    GithubRelease {
        repository: String,
        packages: Vec<OfficialReleasePackage>,
    },
    /// Forward compatibility: a newer provider only removes dynamic install
    /// support for this entry; the official-page fallback remains usable.
    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialReleasePackage {
    pub format: OfficialPackageFormat,
    pub assets: BTreeMap<OfficialArch, String>,
    #[serde(default)]
    pub expected_signer: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfficialPackageFormat {
    Deb,
    Rpm,
    Exe,
    Msi,
    Pkg,
    Dmg,
    #[serde(rename = "appimage")]
    AppImage,
}

impl OfficialPackageFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Deb => "deb",
            Self::Rpm => "rpm",
            Self::Exe => "exe",
            Self::Msi => "msi",
            Self::Pkg => "pkg",
            Self::Dmg => "dmg",
            Self::AppImage => "appimage",
        }
    }

    pub const fn supports_os(self, os: OfficialOs) -> bool {
        match os {
            OfficialOs::Linux => matches!(self, Self::Deb | Self::Rpm | Self::AppImage),
            OfficialOs::Windows => matches!(self, Self::Exe | Self::Msi),
            OfficialOs::Macos => matches!(self, Self::Pkg | Self::Dmg),
        }
    }

    pub const fn action(self) -> OfficialInstallAction {
        match self {
            Self::Deb | Self::Rpm | Self::Msi => OfficialInstallAction::InstallPackage,
            Self::Exe => OfficialInstallAction::RunInstaller,
            Self::Pkg | Self::Dmg => OfficialInstallAction::OpenInstaller,
            Self::AppImage => OfficialInstallAction::DownloadPortable,
        }
    }

    pub fn matches_file(self, name: &str) -> bool {
        Path::new(name)
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case(self.extension()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficialInstallAction {
    InstallPackage,
    RunInstaller,
    OpenInstaller,
    DownloadPortable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedOfficialInstall {
    pub variant_id: String,
    pub version: Option<String>,
    pub file_name: String,
    pub resolved_url: String,
    pub size: Option<u64>,
    pub sha256: String,
    pub format: OfficialPackageFormat,
    pub install_action: OfficialInstallAction,
    pub fallback_url: String,
    pub expected_signer: Option<String>,
    pub requires_admin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfficialResolutionState {
    Resolving,
    Resolved(ResolvedOfficialInstall),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxPackageManager {
    Apt,
    Dnf,
    Zypper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostContext {
    pub os: OfficialOs,
    pub arch: OfficialArch,
    pub linux_package_manager: Option<LinuxPackageManager>,
}

impl HostContext {
    pub const fn new(
        os: OfficialOs,
        arch: OfficialArch,
        linux_package_manager: Option<LinuxPackageManager>,
    ) -> Self {
        Self {
            os,
            arch,
            linux_package_manager,
        }
    }

    pub fn host() -> Option<Self> {
        let os = OfficialOs::host()?;
        let arch = OfficialArch::host()?;
        let linux_package_manager = (os == OfficialOs::Linux)
            .then(linux_package_manager)
            .flatten();
        Some(Self::new(os, arch, linux_package_manager))
    }

    pub fn supports(self, package: &OfficialReleasePackage) -> bool {
        if !package.format.supports_os(self.os) {
            return false;
        }
        match package.format {
            OfficialPackageFormat::Deb => {
                self.linux_package_manager == Some(LinuxPackageManager::Apt)
            }
            OfficialPackageFormat::Rpm => matches!(
                self.linux_package_manager,
                Some(LinuxPackageManager::Dnf | LinuxPackageManager::Zypper)
            ),
            OfficialPackageFormat::AppImage => true,
            _ => true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NetworkOptions<'a> {
    pub proxy: &'a str,
    /// Only intended for explicitly enabled loopback test/development servers.
    pub allow_http: bool,
}

/// A synchronous resolver runs inside the app's existing background executor.
pub trait ReleaseResolver {
    fn resolve(
        &self,
        variant: &OfficialVariant,
        host: HostContext,
        network: NetworkOptions<'_>,
    ) -> Result<ResolvedOfficialInstall, OfficialError>;
}

pub fn resolve_release(
    source: &OfficialReleaseSource,
    variant: &OfficialVariant,
    host: HostContext,
    network: NetworkOptions<'_>,
) -> Result<ResolvedOfficialInstall, OfficialError> {
    match source {
        OfficialReleaseSource::GithubRelease { .. } => {
            super::github::resolve(source, variant, host, network)
        }
        OfficialReleaseSource::Unsupported => Err(OfficialError::Resolve(
            "此客户端不支持该下载来源，请打开官网".to_owned(),
        )),
    }
}

pub fn validate_release(
    source: &OfficialReleaseSource,
    variant: &OfficialVariant,
) -> Result<(), OfficialError> {
    let OfficialReleaseSource::GithubRelease {
        repository,
        packages,
    } = source
    else {
        return Ok(());
    };
    if !valid_repository(repository) {
        return Err(OfficialError::Validation(format!(
            "变体 {} 的 GitHub repository 无效",
            variant.id
        )));
    }
    if packages.is_empty() {
        return Err(OfficialError::Validation(format!(
            "变体 {} 没有 release 包候选",
            variant.id
        )));
    }
    let mut formats = std::collections::HashSet::new();
    for package in packages {
        if !package.format.supports_os(variant.os) {
            return Err(OfficialError::Validation(format!(
                "变体 {} 的包格式与 OS 不匹配",
                variant.id
            )));
        }
        if !formats.insert(package.format) {
            return Err(OfficialError::Validation(format!(
                "变体 {} 包含重复包格式",
                variant.id
            )));
        }
        let signer = package.expected_signer.as_deref().unwrap_or_default();
        if variant.os == OfficialOs::Windows && signer.trim().is_empty() {
            return Err(OfficialError::Validation(format!(
                "变体 {} 的 Windows 包必须声明 expectedSigner",
                variant.id
            )));
        }
        if variant.os != OfficialOs::Windows && package.expected_signer.is_some() {
            return Err(OfficialError::Validation(format!(
                "变体 {} 的非 Windows 包不能声明签名者",
                variant.id
            )));
        }
        if package.assets.is_empty() {
            return Err(OfficialError::Validation(format!(
                "变体 {} 的包没有资产选择器",
                variant.id
            )));
        }
        for (arch, pattern) in &package.assets {
            if !variant.architectures.contains(arch) {
                return Err(OfficialError::Validation(format!(
                    "变体 {} 的资产架构超出变体声明",
                    variant.id
                )));
            }
            validate_asset_pattern(pattern)?;
            let sample = pattern.replace('*', "asset");
            if !package.format.matches_file(&sample) {
                return Err(OfficialError::Validation(format!(
                    "变体 {} 的资产扩展名与包格式不匹配",
                    variant.id
                )));
            }
        }
    }
    for arch in &variant.architectures {
        if !packages
            .iter()
            .any(|package| package.assets.contains_key(arch))
        {
            return Err(OfficialError::Validation(format!(
                "变体 {} 的 {arch} 没有 release 资产",
                variant.id
            )));
        }
    }
    Ok(())
}

pub fn valid_repository(repository: &str) -> bool {
    let mut segments = repository.split('/');
    let valid_segment = |segment: &str| {
        !segment.is_empty()
            && segment.len() <= 100
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            && segment != "."
            && segment != ".."
    };
    segments.next().is_some_and(valid_segment)
        && segments.next().is_some_and(valid_segment)
        && segments.next().is_none()
}

pub fn validate_asset_pattern(pattern: &str) -> Result<(), OfficialError> {
    if pattern.is_empty()
        || pattern.len() > 255
        || pattern.bytes().filter(|byte| *byte == b'*').count() > 8
        || pattern.contains(['/', '\\', '?'])
    {
        return Err(OfficialError::Validation(format!(
            "非法 release 资产选择器 {pattern}"
        )));
    }
    validate_file_name(pattern)
}

/// Match the complete filename, with `*` as the only wildcard.
pub fn asset_pattern_matches(pattern: &str, name: &str) -> bool {
    let pattern = pattern.as_bytes();
    let name = name.as_bytes();
    let (mut pattern_ix, mut name_ix) = (0, 0);
    let (mut star, mut retry_name) = (None, 0);
    while name_ix < name.len() {
        if pattern_ix < pattern.len() && pattern[pattern_ix] == name[name_ix] {
            pattern_ix += 1;
            name_ix += 1;
        } else if pattern_ix < pattern.len() && pattern[pattern_ix] == b'*' {
            star = Some(pattern_ix);
            pattern_ix += 1;
            retry_name = name_ix;
        } else if let Some(star_ix) = star {
            pattern_ix = star_ix + 1;
            retry_name += 1;
            name_ix = retry_name;
        } else {
            return false;
        }
    }
    while pattern_ix < pattern.len() && pattern[pattern_ix] == b'*' {
        pattern_ix += 1;
    }
    pattern_ix == pattern.len()
}

pub fn validate_fallback_page(url: &str, repository: &str) -> Result<(), OfficialError> {
    validate_https(url)?;
    let parsed =
        url::Url::parse(url).map_err(|error| OfficialError::Validation(error.to_string()))?;
    let (owner, repo) = repository.split_once('/').ok_or_else(|| {
        OfficialError::Validation(format!("GitHub repository 无效：{repository}"))
    })?;
    let mut segments = parsed.path_segments().into_iter().flatten();
    if parsed.host_str() != Some("github.com")
        || segments.next() != Some(owner)
        || segments.next() != Some(repo)
        || segments.next() != Some("releases")
    {
        return Err(OfficialError::Validation(format!(
            "GitHub release 回退页必须属于 {repository}"
        )));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn linux_package_manager() -> Option<LinuxPackageManager> {
    let release = std::fs::read_to_string("/etc/os-release").ok()?;
    let ids = release
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key == "ID" || key == "ID_LIKE").then(|| value.trim_matches('"'))
        })
        .flat_map(|value| value.split_whitespace())
        .collect::<Vec<_>>();
    let manager = if ids.iter().any(|id| {
        matches!(
            *id,
            "debian" | "ubuntu" | "linuxmint" | "pop" | "elementary" | "kali"
        )
    }) {
        LinuxPackageManager::Apt
    } else if ids.iter().any(|id| {
        matches!(
            *id,
            "fedora" | "rhel" | "centos" | "rocky" | "almalinux" | "amzn"
        )
    }) {
        LinuxPackageManager::Dnf
    } else if ids.iter().any(|id| {
        matches!(
            *id,
            "suse" | "opensuse" | "opensuse-leap" | "opensuse-tumbleweed"
        )
    }) {
        LinuxPackageManager::Zypper
    } else {
        return None;
    };
    let executable = match manager {
        LinuxPackageManager::Apt => "apt-get",
        LinuxPackageManager::Dnf => "dnf",
        LinuxPackageManager::Zypper => "zypper",
    };
    executable_exists(executable).then_some(manager)
}

#[cfg(not(target_os = "linux"))]
fn linux_package_manager() -> Option<LinuxPackageManager> {
    None
}

#[cfg(target_os = "linux")]
fn executable_exists(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|directory| directory.join(program).is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_must_match_the_complete_name() {
        assert!(asset_pattern_matches(
            "OpenIDEA-*-amd64.deb",
            "OpenIDEA-v1-amd64.deb"
        ));
        assert!(asset_pattern_matches("*.AppImage", "OpenIDEA.AppImage"));
        assert!(!asset_pattern_matches("*.deb", "x.deb.sig"));
        assert!(!asset_pattern_matches("OpenIDEA", "NotOpenIDEA"));
    }

    #[test]
    fn fallback_page_must_be_the_repositories_release_path() {
        assert!(
            validate_fallback_page(
                "https://github.com/owner/repo/releases/latest",
                "owner/repo"
            )
            .is_ok()
        );
        assert!(
            validate_fallback_page("https://github.com/owner/repo/releases-evil", "owner/repo")
                .is_err()
        );
        assert!(
            validate_fallback_page(
                "https://github.com/owner/other/releases/latest",
                "owner/repo"
            )
            .is_err()
        );
    }

    #[test]
    fn msi_uses_waitable_package_install_semantics() {
        assert_eq!(
            OfficialPackageFormat::Msi.action(),
            OfficialInstallAction::InstallPackage
        );
        assert_eq!(
            OfficialPackageFormat::Exe.action(),
            OfficialInstallAction::RunInstaller
        );
    }

    #[test]
    fn repository_shape_is_strict() {
        assert!(valid_repository("l0ng-ai/openidea"));
        assert!(!valid_repository("github.com/l0ng-ai/openidea"));
        assert!(!valid_repository("owner/repo/evil"));
        assert!(!valid_repository("owner/../repo"));
    }
}
