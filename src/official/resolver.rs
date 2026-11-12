//! Provider-neutral release resolution.
//!
//! Providers turn an official release source into one immutable artifact. The
//! confirmation dialog, queue, downloader and platform installers only consume
//! that artifact, so adding another provider does not change their trust rules.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{OfficialArch, OfficialError, OfficialOs, OfficialVariant, validate_file_name};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OfficialReleasePackage {
    pub format: OfficialPackageFormat,
    #[serde(default)]
    pub manager: Option<LinuxPackageManager>,
    pub assets: BTreeMap<OfficialArch, String>,
    #[serde(default)]
    pub expected_signer: Option<String>,
    pub install: OfficialInstallConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OfficialInstallConfig {
    Package,
    RunInstaller,
    InstallPackage,
    #[serde(rename_all = "camelCase")]
    Archive {
        executable_by_arch: BTreeMap<OfficialArch, String>,
        #[serde(default)]
        additional_executables: Vec<String>,
        #[serde(default)]
        strip_components: u8,
        link_name: String,
        #[serde(default)]
        desktop_name: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    AppImage {
        link_name: String,
        #[serde(default)]
        desktop_name: Option<String>,
    },
}

impl<'de> Deserialize<'de> for OfficialInstallConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Input {
            Name(String),
            Object(serde_json::Map<String, serde_json::Value>),
        }
        match Input::deserialize(deserializer)? {
            Input::Name(name) => match name.as_str() {
                "package" => Ok(Self::Package),
                "run_installer" => Ok(Self::RunInstaller),
                "install_package" => Ok(Self::InstallPackage),
                "app_image" => Err(serde::de::Error::custom(
                    "app_image must declare kind and linkName",
                )),
                _ => Err(serde::de::Error::custom(format!(
                    "unknown install kind {name}"
                ))),
            },
            Input::Object(mut object) => {
                let kind = object
                    .remove("kind")
                    .and_then(|kind| kind.as_str().map(str::to_owned))
                    .ok_or_else(|| serde::de::Error::custom("install kind is required"))?;
                match kind.as_str() {
                    "package" | "run_installer" | "install_package" => {
                        if !object.is_empty() {
                            return Err(serde::de::Error::custom(
                                "simple install kinds do not accept fields",
                            ));
                        }
                        Ok(match kind.as_str() {
                            "package" => Self::Package,
                            "run_installer" => Self::RunInstaller,
                            _ => Self::InstallPackage,
                        })
                    }
                    "archive" => {
                        #[derive(Deserialize)]
                        #[serde(rename_all = "camelCase", deny_unknown_fields)]
                        struct Archive {
                            executable_by_arch: BTreeMap<OfficialArch, String>,
                            #[serde(default)]
                            additional_executables: Vec<String>,
                            #[serde(default)]
                            strip_components: u8,
                            link_name: String,
                            #[serde(default)]
                            desktop_name: Option<String>,
                        }
                        let config = Archive::deserialize(serde_json::Value::Object(object))
                            .map_err(serde::de::Error::custom)?;
                        Ok(Self::Archive {
                            executable_by_arch: config.executable_by_arch,
                            additional_executables: config.additional_executables,
                            strip_components: config.strip_components,
                            link_name: config.link_name,
                            desktop_name: config.desktop_name,
                        })
                    }
                    "app_image" => {
                        #[derive(Deserialize)]
                        #[serde(rename_all = "camelCase", deny_unknown_fields)]
                        struct AppImage {
                            link_name: String,
                            #[serde(default)]
                            desktop_name: Option<String>,
                        }
                        let config = AppImage::deserialize(serde_json::Value::Object(object))
                            .map_err(serde::de::Error::custom)?;
                        Ok(Self::AppImage {
                            link_name: config.link_name,
                            desktop_name: config.desktop_name,
                        })
                    }
                    _ => Err(serde::de::Error::custom(format!(
                        "unknown install kind {kind}"
                    ))),
                }
            }
        }
    }
}

impl OfficialInstallConfig {
    pub const fn action(&self) -> OfficialInstallAction {
        match self {
            Self::Package | Self::InstallPackage => OfficialInstallAction::InstallPackage,
            Self::RunInstaller => OfficialInstallAction::RunInstaller,
            Self::Archive { .. } | Self::AppImage { .. } => OfficialInstallAction::InstallPortable,
        }
    }
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
    Zip,
    TarGz,
    TarZst,
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
            Self::Zip => "zip",
            Self::TarGz => "tar.gz",
            Self::TarZst => "tar.zst",
            Self::AppImage => "appimage",
        }
    }

    pub const fn supports_os(self, os: OfficialOs) -> bool {
        match self {
            Self::Zip | Self::TarGz | Self::TarZst => true,
            Self::AppImage | Self::Deb | Self::Rpm => matches!(os, OfficialOs::Linux),
            Self::Exe | Self::Msi => matches!(os, OfficialOs::Windows),
            Self::Pkg | Self::Dmg => matches!(os, OfficialOs::Macos),
        }
    }

    pub fn matches_file(self, name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        match self {
            Self::TarGz => name.ends_with(".tar.gz"),
            Self::TarZst => name.ends_with(".tar.zst"),
            Self::AppImage => name.ends_with(".appimage"),
            _ => name.ends_with(&format!(".{}", self.extension())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficialInstallAction {
    InstallPackage,
    RunInstaller,
    OpenInstaller,
    InstallPortable,
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
    pub install: OfficialInstallConfig,
    pub install_action: OfficialInstallAction,
    pub expected_signer: Option<String>,
    pub requires_admin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfficialResolutionState {
    Resolving,
    Resolved(Box<ResolvedOfficialInstall>),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
                package.manager == Some(LinuxPackageManager::Apt)
                    && self.linux_package_manager == Some(LinuxPackageManager::Apt)
            }
            OfficialPackageFormat::Rpm => {
                package.manager == self.linux_package_manager
                    && matches!(
                        package.manager,
                        Some(LinuxPackageManager::Dnf | LinuxPackageManager::Zypper)
                    )
            }
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
    repository: &str,
    packages: &[OfficialReleasePackage],
    variant: &OfficialVariant,
    host: HostContext,
    network: NetworkOptions<'_>,
) -> Result<ResolvedOfficialInstall, OfficialError> {
    super::github::resolve(repository, packages, variant, host, network)
}

pub fn validate_release(
    repository: &str,
    packages: &[OfficialReleasePackage],
    variant: &OfficialVariant,
) -> Result<(), OfficialError> {
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
        if !formats.insert((package.format, package.manager)) {
            return Err(OfficialError::Validation(format!(
                "变体 {} 包含重复包格式和后端",
                variant.id
            )));
        }
        let signer = package.expected_signer.as_deref().unwrap_or_default();
        let windows_installer = matches!(
            package.format,
            OfficialPackageFormat::Exe | OfficialPackageFormat::Msi
        );
        if windows_installer && signer.trim().is_empty() {
            return Err(OfficialError::Validation(format!(
                "变体 {} 的 Windows 安装包必须声明 expectedSigner",
                variant.id
            )));
        }
        if !windows_installer && package.expected_signer.is_some() {
            return Err(OfficialError::Validation(format!(
                "变体 {} 的非 Windows 安装包不能声明签名者",
                variant.id
            )));
        }
        let valid_action = match (&package.install, package.format) {
            (OfficialInstallConfig::Package, OfficialPackageFormat::Deb) => {
                package.manager == Some(LinuxPackageManager::Apt)
            }
            (OfficialInstallConfig::Package, OfficialPackageFormat::Rpm) => matches!(
                package.manager,
                Some(LinuxPackageManager::Dnf | LinuxPackageManager::Zypper)
            ),
            (OfficialInstallConfig::RunInstaller, OfficialPackageFormat::Exe)
            | (
                OfficialInstallConfig::RunInstaller,
                OfficialPackageFormat::Pkg | OfficialPackageFormat::Dmg,
            ) => true,
            (OfficialInstallConfig::InstallPackage, OfficialPackageFormat::Msi) => true,
            (
                OfficialInstallConfig::Archive {
                    executable_by_arch,
                    additional_executables,
                    strip_components,
                    link_name,
                    desktop_name,
                },
                OfficialPackageFormat::Zip
                | OfficialPackageFormat::TarGz
                | OfficialPackageFormat::TarZst,
            ) => {
                *strip_components <= 1
                    && executable_by_arch.len() == package.assets.len()
                    && package.assets.keys().all(|arch| {
                        executable_by_arch
                            .get(arch)
                            .is_some_and(|path| valid_archive_path(path))
                    })
                    && additional_executables
                        .iter()
                        .all(|path| valid_archive_path(path))
                    && valid_link_name(link_name)
                    && valid_desktop_name(desktop_name.as_deref())
            }
            (
                OfficialInstallConfig::AppImage {
                    link_name,
                    desktop_name,
                },
                OfficialPackageFormat::AppImage,
            ) => valid_link_name(link_name) && valid_desktop_name(desktop_name.as_deref()),
            _ => false,
        };
        if !valid_action
            || (package.manager.is_some()
                && !matches!(
                    package.format,
                    OfficialPackageFormat::Deb | OfficialPackageFormat::Rpm
                ))
        {
            return Err(OfficialError::Validation(format!(
                "变体 {} 的安装动作、包格式或后端不匹配",
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

pub fn valid_archive_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

pub fn valid_link_name(name: &str) -> bool {
    validate_file_name(name).is_ok()
        && !name.ends_with(".cmd")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_desktop_name(name: Option<&str>) -> bool {
    name.is_none_or(|name| {
        !name.trim().is_empty() && name.len() <= 128 && !name.contains(['\n', '\r', '=', '/', '\\'])
    })
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
    Some(manager)
}
#[cfg(not(target_os = "linux"))]
fn linux_package_manager() -> Option<LinuxPackageManager> {
    None
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
    fn archive_formats_and_distro_backends_are_explicit() {
        for (format, file, matches) in [
            (OfficialPackageFormat::Zip, "App.zip", true),
            (OfficialPackageFormat::TarGz, "App.tar.gz", true),
            (OfficialPackageFormat::TarGz, "App.gz", false),
            (OfficialPackageFormat::TarZst, "App.tar.zst", true),
            (OfficialPackageFormat::TarZst, "App.zst", false),
        ] {
            assert_eq!(format.matches_file(file), matches);
        }
        assert_eq!(
            OfficialInstallConfig::InstallPackage.action(),
            OfficialInstallAction::InstallPackage
        );
        assert_eq!(
            OfficialInstallConfig::RunInstaller.action(),
            OfficialInstallAction::RunInstaller
        );
        let apt = HostContext::new(
            OfficialOs::Linux,
            OfficialArch::X86_64,
            Some(LinuxPackageManager::Apt),
        );
        let dnf = HostContext::new(
            OfficialOs::Linux,
            OfficialArch::X86_64,
            Some(LinuxPackageManager::Dnf),
        );
        let package = |format, manager| OfficialReleasePackage {
            format,
            manager,
            assets: BTreeMap::from([(OfficialArch::X86_64, "app".to_owned())]),
            expected_signer: None,
            install: OfficialInstallConfig::Package,
        };
        assert!(apt.supports(&package(
            OfficialPackageFormat::Deb,
            Some(LinuxPackageManager::Apt)
        )));
        assert!(!dnf.supports(&package(
            OfficialPackageFormat::Deb,
            Some(LinuxPackageManager::Apt)
        )));
        assert!(dnf.supports(&package(
            OfficialPackageFormat::Rpm,
            Some(LinuxPackageManager::Dnf)
        )));
        assert!(!dnf.supports(&package(
            OfficialPackageFormat::Rpm,
            Some(LinuxPackageManager::Zypper)
        )));
    }

    #[test]
    fn repository_shape_is_strict() {
        assert!(valid_repository("l0ng-ai/openidea"));
        assert!(!valid_repository("github.com/l0ng-ai/openidea"));
        assert!(!valid_repository("owner/repo/evil"));
        assert!(!valid_repository("owner/../repo"));
    }
}
