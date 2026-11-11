//! Verified platform actions for an already-confirmed release snapshot.

use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
use crate::backends::exec;

use super::resolver::{HostContext, OfficialPackageFormat, ResolvedOfficialInstall};
use super::{OfficialError, OfficialOutcome};

/// Download and execute exactly the snapshot shown in the confirmation dialog.
/// No release lookup or asset selection is allowed on this path.
pub fn run_resolved(
    resolved: &ResolvedOfficialInstall,
    proxy: &str,
    allow_http: bool,
) -> Result<OfficialOutcome, OfficialError> {
    let path = super::download_artifact(
        &resolved.resolved_url,
        &resolved.file_name,
        Some(&resolved.sha256),
        resolved.size,
        proxy,
        allow_http,
    )?;
    let path = std::fs::canonicalize(path)?;
    match resolved.format {
        OfficialPackageFormat::Deb | OfficialPackageFormat::Rpm => {
            install_linux_package(resolved.format, &path)
        }
        OfficialPackageFormat::AppImage
        | OfficialPackageFormat::Zip
        | OfficialPackageFormat::TarGz
        | OfficialPackageFormat::TarZst => super::portable::install_resolved(resolved, &path),
        OfficialPackageFormat::Exe | OfficialPackageFormat::Msi => install_windows(resolved, &path),
        OfficialPackageFormat::Pkg | OfficialPackageFormat::Dmg => {
            #[cfg(target_os = "macos")]
            {
                super::open_path(&path)?;
                Ok(OfficialOutcome::InstallerStarted { path })
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = path;
                Err(OfficialError::Process(
                    "PKG/DMG 只能在 macOS 上打开".to_owned(),
                ))
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn install_linux_package(
    format: OfficialPackageFormat,
    path: &Path,
) -> Result<OfficialOutcome, OfficialError> {
    use crate::backends::{PackageBackend, apt::Apt, dnf::Dnf, zypper::Zypper};

    let host =
        HostContext::host().ok_or_else(|| OfficialError::Process("无法识别当前系统".to_owned()))?;
    let path = path
        .to_str()
        .ok_or_else(|| OfficialError::Process("安装包路径不是有效 UTF-8".to_owned()))?;
    let args = package_args(host, format, path)?;
    let output = match host.linux_package_manager {
        Some(super::resolver::LinuxPackageManager::Apt) => {
            Apt::discover().and_then(|backend| backend.run(&args))
        }
        Some(super::resolver::LinuxPackageManager::Dnf) => {
            Dnf::discover().and_then(|backend| backend.run(&args))
        }
        Some(super::resolver::LinuxPackageManager::Zypper) => {
            Zypper::discover().and_then(|backend| backend.run(&args))
        }
        None => return Err(OfficialError::Process("没有可用的系统包管理器".to_owned())),
    }
    .map_err(|error| OfficialError::Process(error.localized()))?;
    let log = output.combined();
    if output.status != 0 || output.signal_terminated {
        return Err(OfficialError::Process(format!(
            "包管理器退出码 {}{}",
            output.status,
            if log.is_empty() {
                String::new()
            } else {
                format!("\n{log}")
            }
        )));
    }
    Ok(OfficialOutcome::PackageInstalled {
        path: PathBuf::from(path),
        log,
    })
}

#[cfg(not(target_os = "linux"))]
fn install_linux_package(
    _format: OfficialPackageFormat,
    _path: &Path,
) -> Result<OfficialOutcome, OfficialError> {
    Err(OfficialError::Process(
        "DEB/RPM 只能在 Linux 上安装".to_owned(),
    ))
}

/// Construct the same argument shapes used by the normal system backends.
/// Their runners handle PackageKit/polkit and fall back to waitable pkexec.
fn package_args(
    host: HostContext,
    format: OfficialPackageFormat,
    path: &str,
) -> Result<Vec<&str>, OfficialError> {
    match (format, host.linux_package_manager) {
        (OfficialPackageFormat::Deb, Some(super::resolver::LinuxPackageManager::Apt)) => {
            Ok(vec!["install", "-y", path])
        }
        (OfficialPackageFormat::Rpm, Some(super::resolver::LinuxPackageManager::Dnf)) => {
            Ok(vec!["install", "-y", path])
        }
        (OfficialPackageFormat::Rpm, Some(super::resolver::LinuxPackageManager::Zypper)) => {
            Ok(vec!["--non-interactive", "install", path])
        }
        _ => Err(OfficialError::Process(
            "安装包格式与当前发行版包管理器不匹配".to_owned(),
        )),
    }
}

#[cfg(target_os = "windows")]
fn install_windows(
    resolved: &ResolvedOfficialInstall,
    path: &Path,
) -> Result<OfficialOutcome, OfficialError> {
    verify_windows_package(path, resolved.expected_signer.as_deref())?;
    if resolved.format == OfficialPackageFormat::Msi {
        let path_arg = path
            .to_str()
            .ok_or_else(|| OfficialError::Process("安装包路径不是有效 UTF-8".to_owned()))?;
        let output = exec::run_process(Path::new("msiexec"), &[], &["/i", path_arg])
            .map_err(|error| OfficialError::Process(error.localized()))?;
        let log = output.combined();
        if msi_requires_reboot(output.status, output.signal_terminated, &log)? {
            Ok(OfficialOutcome::InstallerRebootRequired {
                path: path.to_owned(),
            })
        } else {
            Ok(OfficialOutcome::PackageInstalled {
                path: path.to_owned(),
                log,
            })
        }
    } else {
        // Shell execution lets Windows present UAC when the installer asks for
        // it. No undocumented silent switches are added.
        super::open_path(path)?;
        Ok(OfficialOutcome::InstallerStarted {
            path: path.to_owned(),
        })
    }
}

#[cfg(not(target_os = "windows"))]
fn install_windows(
    _resolved: &ResolvedOfficialInstall,
    _path: &Path,
) -> Result<OfficialOutcome, OfficialError> {
    Err(OfficialError::Process(
        "EXE/MSI 只能在 Windows 上运行".to_owned(),
    ))
}

#[cfg(any(target_os = "windows", test))]
fn msi_requires_reboot(status: u32, signaled: bool, log: &str) -> Result<bool, OfficialError> {
    match (status, signaled) {
        (0, false) => Ok(false),
        (3010, false) => Ok(true),
        (1602, false) => Err(OfficialError::Process("用户取消了 MSI 安装".to_owned())),
        (status, _) => Err(OfficialError::Process(format!(
            "MSI 安装失败（退出码 {status}）\n{log}"
        ))),
    }
}

#[cfg(target_os = "windows")]
fn verify_windows_package(path: &Path, expected_signer: Option<&str>) -> Result<(), OfficialError> {
    let expected_signer = expected_signer
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| OfficialError::Process("Windows 包缺少预期签名者".to_owned()))?;
    let expected_arch = super::OfficialArch::host()
        .ok_or_else(|| OfficialError::Process("无法识别 Windows CPU 架构".to_owned()))?;
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "exe" => {
            let mut file = std::fs::File::open(path)?;
            let machine = pe_machine_reader(&mut file)?
                .ok_or_else(|| OfficialError::Process("无法验证 EXE 的 PE 架构".to_owned()))?;
            if !machine_matches(machine, expected_arch) {
                return Err(OfficialError::Process("EXE 架构与当前系统不符".to_owned()));
            }
        }
        "msi" => {}
        _ => return Err(OfficialError::Process("不支持的 Windows 安装包".to_owned())),
    }
    let path_arg = path
        .to_str()
        .ok_or_else(|| OfficialError::Process("安装包路径不是有效 UTF-8".to_owned()))?;
    let output = exec::run_process(
        Path::new("powershell"),
        &[],
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            WINDOWS_VERIFY_SCRIPT,
            path_arg,
        ],
    )
    .map_err(|error| OfficialError::Process(error.localized()))?;
    if output.status != 0 || output.signal_terminated {
        return Err(OfficialError::Process(format!(
            "Windows 签名/架构验证失败: {}",
            output.combined()
        )));
    }
    let verification: WindowsVerification = serde_json::from_str(output.stdout.trim())
        .map_err(|error| OfficialError::Process(format!("Windows 验证响应无效: {error}")))?;
    if verification.status != "Valid" || verification.subject != expected_signer {
        return Err(OfficialError::Process(format!(
            "Authenticode 签名无效或签名者变化：{} / {}",
            verification.status, verification.subject
        )));
    }
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("msi"))
        && !msi_template_matches(&verification.template, expected_arch)
    {
        return Err(OfficialError::Process("MSI 架构与当前系统不符".to_owned()));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WindowsVerification {
    status: String,
    subject: String,
    template: String,
}

#[cfg(target_os = "windows")]
const WINDOWS_VERIFY_SCRIPT: &str = r#"
$signature = Get-AuthenticodeSignature -LiteralPath $args[0]
$template = ''
if ([IO.Path]::GetExtension($args[0]) -ieq '.msi') {
    $installer = New-Object -ComObject WindowsInstaller.Installer
    $database = $installer.GetType().InvokeMember('OpenDatabase', 'InvokeMethod', $null, $installer, @($args[0], 0))
    $summary = $database.GetType().InvokeMember('SummaryInformation', 'GetProperty', $null, $database, $null)
    $template = [string]$summary.GetType().InvokeMember('Property', 'GetProperty', $null, $summary, 7)
}
[pscustomobject]@{
    Status = [string]$signature.Status
    Subject = [string]$signature.SignerCertificate.Subject
    Template = $template
} | ConvertTo-Json -Compress
"#;

#[cfg(any(target_os = "windows", test))]
fn pe_machine_reader<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
) -> std::io::Result<Option<u16>> {
    let mut dos = [0_u8; 64];
    reader.read_exact(&mut dos[..2])?;
    if dos[..2] != *b"MZ" {
        return Ok(None);
    }
    reader.read_exact(&mut dos[2..])?;
    let offset = u64::from(u32::from_le_bytes(dos[0x3c..0x40].try_into().map_err(
        |_| std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid PE offset"),
    )?));
    reader.seek(std::io::SeekFrom::Start(offset))?;
    let mut header = [0_u8; 6];
    reader.read_exact(&mut header)?;
    if header[..4] != *b"PE\0\0" {
        return Ok(None);
    }
    Ok(Some(u16::from_le_bytes([header[4], header[5]])))
}

#[cfg(any(target_os = "windows", test))]
fn machine_matches(machine: u16, arch: super::OfficialArch) -> bool {
    match arch {
        super::OfficialArch::X86_64 => machine == 0x8664,
        super::OfficialArch::Aarch64 => machine == 0xaa64,
    }
}

#[cfg(any(target_os = "windows", test))]
fn msi_template_matches(template: &str, arch: super::OfficialArch) -> bool {
    let platform = template
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match arch {
        super::OfficialArch::X86_64 => matches!(platform.as_str(), "x64" | "amd64" | "intel64"),
        super::OfficialArch::Aarch64 => platform == "arm64",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::official::OfficialArch;
    use crate::official::resolver::{
        LinuxPackageManager, OfficialInstallAction, OfficialInstallConfig,
    };
    #[test]
    fn package_arguments_never_use_a_shell_or_bare_dpkg() {
        let apt = HostContext::new(
            super::super::OfficialOs::Linux,
            OfficialArch::X86_64,
            Some(LinuxPackageManager::Apt),
        );
        assert_eq!(
            package_args(apt, OfficialPackageFormat::Deb, "/tmp/a file.deb")
                .expect("supported local package arguments"),
            ["install", "-y", "/tmp/a file.deb"]
        );
        let dnf = HostContext::new(
            super::super::OfficialOs::Linux,
            OfficialArch::Aarch64,
            Some(LinuxPackageManager::Dnf),
        );
        assert_eq!(
            package_args(dnf, OfficialPackageFormat::Rpm, "/tmp/a.rpm")
                .expect("supported local package arguments"),
            ["install", "-y", "/tmp/a.rpm"]
        );
        let zypper = HostContext::new(
            super::super::OfficialOs::Linux,
            OfficialArch::X86_64,
            Some(LinuxPackageManager::Zypper),
        );
        assert_eq!(
            package_args(zypper, OfficialPackageFormat::Rpm, "/tmp/a.rpm")
                .expect("supported local package arguments"),
            ["--non-interactive", "install", "/tmp/a.rpm"]
        );
        assert!(package_args(zypper, OfficialPackageFormat::Deb, "/tmp/a.deb").is_err());
    }

    #[test]
    fn pe_and_msi_architectures_fail_closed() {
        let mut pe = vec![0_u8; 128];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3c..0x40].copy_from_slice(&64_u32.to_le_bytes());
        pe[64..68].copy_from_slice(b"PE\0\0");
        pe[68..70].copy_from_slice(&0x8664_u16.to_le_bytes());
        assert_eq!(
            pe_machine_reader(&mut std::io::Cursor::new(&pe)).expect("PE header"),
            Some(0x8664)
        );
        assert!(machine_matches(0x8664, OfficialArch::X86_64));
        assert!(!machine_matches(0x8664, OfficialArch::Aarch64));
        assert!(msi_template_matches("x64;1033", OfficialArch::X86_64));
        assert!(msi_template_matches("Arm64;1033", OfficialArch::Aarch64));
        assert!(!msi_template_matches("Intel;1033", OfficialArch::X86_64));
        assert_eq!(
            pe_machine_reader(&mut std::io::Cursor::new(b"not a PE")).expect("non-PE"),
            None
        );
    }

    #[test]
    fn msi_exit_codes_distinguish_success_reboot_and_cancellation() {
        assert!(!msi_requires_reboot(0, false, "").expect("installed"));
        assert!(msi_requires_reboot(3010, false, "").expect("reboot required"));
        assert!(msi_requires_reboot(1602, false, "").is_err());
        assert!(msi_requires_reboot(1, false, "failure").is_err());
        assert!(msi_requires_reboot(0, true, "terminated").is_err());
    }

    #[test]
    fn installer_actions_remain_distinct_from_portable_installation() {
        assert_eq!(
            OfficialInstallConfig::RunInstaller.action(),
            OfficialInstallAction::RunInstaller
        );
        assert_eq!(
            OfficialInstallConfig::AppImage {
                link_name: "app".to_owned(),
                desktop_name: None,
            }
            .action(),
            OfficialInstallAction::InstallPortable
        );
    }
}
