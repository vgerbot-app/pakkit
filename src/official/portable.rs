//! Verified, user-local portable and AppImage installation.

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::resolver::{
    OfficialInstallConfig, OfficialPackageFormat, ResolvedOfficialInstall, valid_archive_path,
    valid_link_name,
};
use super::{OfficialArch, OfficialError, OfficialOutcome};

const LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    path: String,
    directory: bool,
}

#[derive(Debug, Default)]
struct Entries {
    all: Vec<Entry>,
    paths: HashSet<String>,
    total: u64,
}

impl Entries {
    fn add(&mut self, path: String, directory: bool, size: u64) -> Result<(), OfficialError> {
        if self.all.len() >= MAX_ENTRIES || size > LIMIT {
            return Err(OfficialError::Validation(
                "归档条目数或单文件大小超过限制".to_owned(),
            ));
        }
        self.total = self
            .total
            .checked_add(size)
            .ok_or_else(|| OfficialError::Validation("归档解压总大小溢出".to_owned()))?;
        if self.total > LIMIT {
            return Err(OfficialError::Validation(
                "归档解压总大小超过 2 GiB".to_owned(),
            ));
        }
        if !self.paths.insert(path.clone()) {
            return Err(OfficialError::Validation(format!(
                "归档包含重复目标 {path}"
            )));
        }
        self.all.push(Entry { path, directory });
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct Roots {
    data: PathBuf,
    home: PathBuf,
}

impl Roots {
    fn host() -> Result<Self, OfficialError> {
        let data = dirs::data_local_dir()
            .ok_or_else(|| OfficialError::Process("无法确定用户数据目录".to_owned()))?;
        let home = dirs::home_dir()
            .ok_or_else(|| OfficialError::Process("无法确定用户主目录".to_owned()))?;
        Ok(Self { data, home })
    }

    fn apps(&self) -> PathBuf {
        self.data.join("pakkit").join("apps")
    }

    fn bin(&self) -> PathBuf {
        if cfg!(windows) {
            self.data.join("pakkit").join("bin")
        } else {
            self.home.join(".local").join("bin")
        }
    }
}

pub fn install_resolved(
    resolved: &ResolvedOfficialInstall,
    archive: &Path,
) -> Result<OfficialOutcome, OfficialError> {
    let roots = Roots::host()?;
    install_resolved_at(resolved, archive, &roots)
}

pub fn install_fixed(
    variant: &super::OfficialVariant,
    archive: &Path,
) -> Result<OfficialOutcome, OfficialError> {
    let roots = Roots::host()?;
    install_fixed_at(variant, archive, &roots)
}

fn install_fixed_at(
    variant: &super::OfficialVariant,
    archive: &Path,
    roots: &Roots,
) -> Result<OfficialOutcome, OfficialError> {
    if !matches!(variant.method, super::OfficialMethod::Portable { .. }) {
        return Err(OfficialError::Process("不是固定便携安装方法".to_owned()));
    }
    let arch = variant
        .architectures
        .first()
        .ok_or_else(|| OfficialError::Validation("便携变体缺少架构".to_owned()))?;
    let expected = format!(
        "rufus-windows-{}",
        match arch {
            super::OfficialArch::X86_64 => "x86-64",
            super::OfficialArch::Aarch64 => "aarch64",
        }
    );
    if variant.id != expected {
        return Err(OfficialError::Validation(format!(
            "固定便携变体 ID 不受支持：{}",
            variant.id
        )));
    }
    let file_name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| OfficialError::Validation("便携文件名无效".to_owned()))?;
    let version = Path::new(file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("portable")
        .to_owned();
    let target = version_target(roots, &variant.id, &version)?;
    let link_name = match arch {
        super::OfficialArch::X86_64 => "rufus",
        super::OfficialArch::Aarch64 => "rufus-arm64",
    };
    install_single_file(
        archive,
        &target,
        file_name,
        roots,
        &variant.id,
        link_name,
        None,
    )
}

fn install_resolved_at(
    resolved: &ResolvedOfficialInstall,
    archive: &Path,
    roots: &Roots,
) -> Result<OfficialOutcome, OfficialError> {
    let version = resolved
        .version
        .as_deref()
        .ok_or_else(|| OfficialError::Validation("便携安装缺少版本".to_owned()))?;
    let target = version_target(roots, &resolved.variant_id, version)?;
    match (&resolved.install, resolved.format) {
        (
            OfficialInstallConfig::Archive {
                executable_by_arch,
                strip_components,
                link_name,
                desktop_name,
            },
            OfficialPackageFormat::Zip
            | OfficialPackageFormat::TarGz
            | OfficialPackageFormat::TarZst,
        ) => {
            let arch = OfficialArch::host()
                .ok_or_else(|| OfficialError::Process("无法识别当前 CPU 架构".to_owned()))?;
            let executable = executable_by_arch
                .get(&arch)
                .ok_or_else(|| OfficialError::Validation("归档缺少当前架构入口".to_owned()))?;
            install_archive(
                archive,
                resolved.format,
                &target,
                roots,
                &resolved.variant_id,
                executable,
                *strip_components,
                link_name,
                desktop_name.as_deref(),
            )
        }
        (
            OfficialInstallConfig::AppImage {
                link_name,
                desktop_name,
            },
            OfficialPackageFormat::AppImage,
        ) => install_single_file(
            archive,
            &target,
            &resolved.file_name,
            roots,
            &resolved.variant_id,
            link_name,
            desktop_name.as_deref(),
        ),
        _ => Err(OfficialError::Process(
            "安装动作不是归档或 AppImage".to_owned(),
        )),
    }
}

fn version_target(
    roots: &Roots,
    variant_id: &str,
    version: &str,
) -> Result<PathBuf, OfficialError> {
    if variant_id.is_empty()
        || !variant_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || version.is_empty()
        || version.len() > 255
        || version.contains(['/', '\\', '?', '#', ':', '\0'])
        || version == "."
        || version == ".."
    {
        return Err(OfficialError::Validation(format!(
            "便携安装目录名无效：{variant_id}/{version}"
        )));
    }
    Ok(roots.apps().join(variant_id).join(version))
}

#[allow(clippy::too_many_arguments)]
fn install_archive(
    archive: &Path,
    format: OfficialPackageFormat,
    target: &Path,
    roots: &Roots,
    variant_id: &str,
    executable: &str,
    strip_components: u8,
    link_name: &str,
    desktop_name: Option<&str>,
) -> Result<OfficialOutcome, OfficialError> {
    if !valid_archive_path(executable) || strip_components > 1 || !valid_link_name(link_name) {
        return Err(OfficialError::Validation(
            "归档入口或启动链接无效".to_owned(),
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| OfficialError::Validation("便携安装目标没有父目录".to_owned()))?;
    let parent_existed = parent.exists();
    std::fs::create_dir_all(parent)?;
    reject_symlink_components(parent)?;
    if target.exists() {
        return finish_existing(
            target,
            roots,
            variant_id,
            executable,
            link_name,
            desktop_name,
        );
    }
    let staging = temporary_path(parent, ".installing");
    std::fs::create_dir(&staging)?;
    let result = (|| {
        match format {
            OfficialPackageFormat::Zip => extract_zip(archive, &staging, strip_components)?,
            OfficialPackageFormat::TarGz | OfficialPackageFormat::TarZst => {
                extract_tar(archive, &staging, format, strip_components)?
            }
            _ => unreachable!(),
        };
        validate_entry(&staging, executable)?;
        set_executable(&staging.join(executable))?;
        commit_staging(&staging, target)?;
        finish_existing(
            target,
            roots,
            variant_id,
            executable,
            link_name,
            desktop_name,
        )
    })();
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    if result.is_err() && !parent_existed && !target.exists() {
        let _ = std::fs::remove_dir(parent);
    }
    result
}

fn install_single_file(
    archive: &Path,
    target: &Path,
    file_name: &str,
    roots: &Roots,
    variant_id: &str,
    link_name: &str,
    desktop_name: Option<&str>,
) -> Result<OfficialOutcome, OfficialError> {
    super::validate_file_name(file_name)?;
    if !valid_link_name(link_name) {
        return Err(OfficialError::Validation("启动链接名称无效".to_owned()));
    }
    let parent = target
        .parent()
        .ok_or_else(|| OfficialError::Validation("便携安装目标没有父目录".to_owned()))?;
    let parent_existed = parent.exists();
    std::fs::create_dir_all(parent)?;
    reject_symlink_components(parent)?;
    if target.exists() {
        return finish_existing(
            target,
            roots,
            variant_id,
            file_name,
            link_name,
            desktop_name,
        );
    }
    let staging = temporary_path(parent, ".installing");
    std::fs::create_dir(&staging)?;
    let result = (|| {
        copy_file(archive, &staging.join(file_name))?;
        set_executable(&staging.join(file_name))?;
        validate_entry(&staging, file_name)?;
        commit_staging(&staging, target)?;
        finish_existing(
            target,
            roots,
            variant_id,
            file_name,
            link_name,
            desktop_name,
        )
    })();
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    if result.is_err() && !parent_existed && !target.exists() {
        let _ = std::fs::remove_dir(parent);
    }
    result
}

fn finish_existing(
    target: &Path,
    roots: &Roots,
    variant_id: &str,
    executable: &str,
    link_name: &str,
    desktop_name: Option<&str>,
) -> Result<OfficialOutcome, OfficialError> {
    reject_symlink_components(target)?;
    let executable_path = validate_entry(target, executable)?;
    let launch_path = create_launcher(roots, link_name, &executable_path)?;
    if let Some(name) = desktop_name {
        write_desktop_file(roots, variant_id, name, &executable_path, target)?;
    }
    Ok(OfficialOutcome::PortableInstalled {
        path: target.to_owned(),
        launch_path,
    })
}

fn commit_staging(staging: &Path, target: &Path) -> Result<(), OfficialError> {
    let result = if target.exists() {
        Err(OfficialError::Process(format!(
            "版本目录已存在，未覆盖：{}",
            target.display()
        )))
    } else {
        std::fs::rename(staging, target)
            .map_err(|error| OfficialError::Process(format!("无法原子提交便携安装：{error}")))
    };
    if result.is_err() && staging.exists() {
        let _ = std::fs::remove_dir_all(staging);
    }
    result
}

fn validate_entry(target: &Path, relative: &str) -> Result<PathBuf, OfficialError> {
    if !valid_archive_path(relative) {
        return Err(OfficialError::Validation(format!(
            "入口路径无效 {relative}"
        )));
    }
    let path = target.join(relative);
    reject_symlink_components(target)?;
    let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
        OfficialError::Process(format!("便携入口不存在 {}：{error}", path.display()))
    })?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(OfficialError::Process("便携入口不是普通文件".to_owned()));
    }
    if cfg!(windows) {
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("exe"))
        {
            return Err(OfficialError::Validation(
                "Windows 便携入口必须是 EXE".to_owned(),
            ));
        }
    } else {
        set_executable(&path)?;
    }
    Ok(path)
}

fn extract_zip(archive: &Path, staging: &Path, strip: u8) -> Result<Entries, OfficialError> {
    let mut zip = zip::ZipArchive::new(File::open(archive)?)
        .map_err(|error| OfficialError::Validation(format!("ZIP 无法读取：{error}")))?;
    if zip.len() > MAX_ENTRIES {
        return Err(OfficialError::Validation("ZIP 条目超过 10000".to_owned()));
    }
    let mut entries = Entries::default();
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| OfficialError::Validation(format!("ZIP 条目无法读取：{error}")))?;
        let raw_name = entry.name().to_owned();
        let name = safe_archive_path(&raw_name, strip)?;
        let directory = entry.is_dir() || raw_name.ends_with('/');
        let unix_mode = entry.unix_mode().unwrap_or(0);
        let file_type = unix_mode & 0o170000;
        if !directory && file_type != 0 && file_type != 0o100000 {
            return Err(OfficialError::Validation(format!(
                "ZIP 包含非正常文件 {raw_name}"
            )));
        }
        let Some(name) = name else {
            if !directory {
                return Err(OfficialError::Validation(format!(
                    "剥离路径后为空 {raw_name}"
                )));
            }
            continue;
        };
        let size = entry.size();
        entries.add(name.clone(), directory, size)?;
        if directory {
            create_safe_directory(staging, &name)?;
            continue;
        }
        let path = create_safe_file(staging, &name)?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        copy_bounded(&mut entry, &mut file, size)?;
        file.sync_all()?;
    }
    Ok(entries)
}

fn extract_tar(
    archive: &Path,
    staging: &Path,
    format: OfficialPackageFormat,
    strip: u8,
) -> Result<Entries, OfficialError> {
    let file = File::open(archive)?;
    let reader: Box<dyn Read> = match format {
        OfficialPackageFormat::TarGz => Box::new(flate2::read::GzDecoder::new(file)),
        OfficialPackageFormat::TarZst => {
            Box::new(zstd::stream::read::Decoder::new(file).map_err(|error| {
                OfficialError::Validation(format!("Zstandard 无法读取：{error}"))
            })?)
        }
        _ => return Err(OfficialError::Validation("不是 TAR 归档".to_owned())),
    };
    let mut tar = tar::Archive::new(reader);
    let mut entries = Entries::default();
    for entry in tar.entries()? {
        let mut entry = entry?;
        if entries.all.len() >= MAX_ENTRIES {
            return Err(OfficialError::Validation("TAR 条目超过 10000".to_owned()));
        }
        let raw_path = entry.path()?.into_owned();
        let raw_name = raw_path
            .to_str()
            .ok_or_else(|| OfficialError::Validation("TAR 路径不是 UTF-8".to_owned()))?;
        let name = safe_archive_path(raw_name, strip)?;
        let kind = entry.header().entry_type();
        let directory = kind.is_dir();
        if !directory && !kind.is_file() {
            return Err(OfficialError::Validation(format!(
                "TAR 包含链接、设备或其他非正常文件 {raw_name}"
            )));
        }
        entry.header().mode()?;
        let Some(name) = name else {
            if !directory {
                return Err(OfficialError::Validation(format!(
                    "剥离路径后为空 {raw_name}"
                )));
            }
            continue;
        };
        let size = entry.header().size()?;
        entries.add(name.clone(), directory, size)?;
        if directory {
            create_safe_directory(staging, &name)?;
            continue;
        }
        let path = create_safe_file(staging, &name)?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        copy_bounded(&mut entry, &mut file, size)?;
        file.sync_all()?;
    }
    Ok(entries)
}

fn safe_archive_path(raw: &str, strip: u8) -> Result<Option<String>, OfficialError> {
    if raw.contains('\\') || raw.starts_with('/') || raw.contains('\0') {
        return Err(OfficialError::Validation(format!("非法归档路径 {raw}")));
    }
    let path = Path::new(raw);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::ParentDir
            )
        })
    {
        return Err(OfficialError::Validation(format!("归档路径逃逸 {raw}")));
    }
    let parts = raw
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.contains(&"..") {
        return Err(OfficialError::Validation(format!("归档路径逃逸 {raw}")));
    }
    if strip > 1 || parts.len() <= usize::from(strip) {
        return Ok(None);
    }
    Ok(Some(parts[usize::from(strip)..].join("/")))
}

fn create_safe_directory(root: &Path, relative: &str) -> Result<PathBuf, OfficialError> {
    if !valid_archive_path(relative) {
        return Err(OfficialError::Validation(format!(
            "非法提取路径 {relative}"
        )));
    }
    let mut path = root.to_owned();
    for part in relative.split('/') {
        path.push(part);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(OfficialError::Validation(format!(
                    "提取父目录不是安全目录 {}",
                    path.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&path)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}

fn create_safe_file(root: &Path, relative: &str) -> Result<PathBuf, OfficialError> {
    let (parent, name) = relative.rsplit_once('/').unwrap_or(("", relative));
    let directory = if parent.is_empty() {
        root.to_owned()
    } else {
        create_safe_directory(root, parent)?
    };
    let path = directory.join(name);
    if std::fs::symlink_metadata(&path).is_ok() {
        return Err(OfficialError::Validation(format!(
            "提取目标已存在 {}",
            path.display()
        )));
    }
    Ok(path)
}

fn copy_bounded(
    reader: &mut dyn Read,
    writer: &mut File,
    declared: u64,
) -> Result<(), OfficialError> {
    let mut limited = reader.take(LIMIT.saturating_add(1));
    let written = std::io::copy(&mut limited, writer)?;
    if written != declared || written > LIMIT {
        return Err(OfficialError::Validation(
            "归档条目大小与声明不符或超过限制".to_owned(),
        ));
    }
    Ok(())
}

fn copy_file(source: &Path, target: &Path) -> Result<(), OfficialError> {
    let mut input = File::open(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > LIMIT {
            return Err(OfficialError::Validation("便携文件超过 2 GiB".to_owned()));
        }
        output.write_all(&buffer[..count])?;
    }
    output.sync_all()?;
    Ok(())
}

fn reject_symlink_components(path: &Path) -> Result<(), OfficialError> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(OfficialError::Validation(format!(
            "安装路径包含符号链接 {}",
            path.display()
        )));
    }
    if let Some(parent) = path.parent()
        && parent != path
        && parent.exists()
    {
        reject_symlink_components(parent)?;
    }
    Ok(())
}

fn temporary_path(parent: &Path, label: &str) -> PathBuf {
    parent.join(format!(
        "{label}-{}-{}",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

fn create_launcher(
    roots: &Roots,
    link_name: &str,
    executable: &Path,
) -> Result<PathBuf, OfficialError> {
    let bin = roots.bin();
    std::fs::create_dir_all(&bin)?;
    reject_symlink_components(&bin)?;
    if cfg!(windows) {
        let link = bin.join(format!("{link_name}.cmd"));
        let content = format!("@echo off\r\n\"{}\" %*\r\n", executable.display());
        atomic_write(&link, content.as_bytes())?;
        Ok(link)
    } else {
        let link = bin.join(link_name);
        let temporary = temporary_path(&bin, ".link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(executable, &temporary)?;
        #[cfg(not(unix))]
        return Err(OfficialError::Process(
            "此平台不支持启动符号链接".to_owned(),
        ));
        if let Err(error) = std::fs::rename(&temporary, &link) {
            let _ = std::fs::remove_file(&temporary);
            return Err(OfficialError::Process(format!(
                "无法原子替换启动链接 {}：{error}",
                link.display()
            )));
        }
        Ok(link)
    }
}

fn write_desktop_file(
    roots: &Roots,
    variant_id: &str,
    name: &str,
    executable: &Path,
    working_directory: &Path,
) -> Result<(), OfficialError> {
    if !cfg!(target_os = "linux") {
        return Ok(());
    }
    let directory = roots.data.join("applications");
    std::fs::create_dir_all(&directory)?;
    reject_symlink_components(&directory)?;
    let path = directory.join(format!("pakkit-{variant_id}.desktop"));
    let content = format!(
        "[Desktop Entry]\nType=Application\nName={name}\nExec=\"{}\"\nPath=\"{}\"\nTerminal=false\n",
        desktop_quote(executable),
        desktop_quote(working_directory),
    );
    atomic_write(&path, content.as_bytes())
}

fn desktop_quote(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<(), OfficialError> {
    let parent = path
        .parent()
        .ok_or_else(|| OfficialError::Validation("原子写入目标没有父目录".to_owned()))?;
    let temporary = temporary_path(parent, ".write");
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(content)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if temporary.exists() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), OfficialError> {
    use std::os::unix::fs::PermissionsExt as _;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), OfficialError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::official::resolver::OfficialInstallAction;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = temporary_path(&std::env::temp_dir(), "portable-test");
            std::fs::create_dir(&path).expect("create test root");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn roots(test: &TempDir) -> Roots {
        Roots {
            data: test.0.join("data"),
            home: test.0.join("home"),
        }
    }

    fn resolved() -> ResolvedOfficialInstall {
        ResolvedOfficialInstall {
            variant_id: "test-app".to_owned(),
            version: Some("v1.2.3".to_owned()),
            file_name: "app.tar.gz".to_owned(),
            resolved_url: "https://example.com/app.tar.gz".to_owned(),
            size: None,
            sha256: "a".repeat(64),
            format: OfficialPackageFormat::TarGz,
            install: OfficialInstallConfig::Archive {
                executable_by_arch: [
                    (OfficialArch::X86_64, "bin/app".to_owned()),
                    (OfficialArch::Aarch64, "bin/app".to_owned()),
                ]
                .into_iter()
                .collect(),
                strip_components: 1,
                link_name: "test-app".to_owned(),
                desktop_name: Some("Test App".to_owned()),
            },
            install_action: OfficialInstallAction::InstallPortable,
            expected_signer: None,
            requires_admin: false,
        }
    }

    fn tar_archive(path: &Path, name: &str, content: &[u8], mode: u32) {
        let file = File::create(path).expect("archive");
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_path(name).expect("path");
        header.set_size(content.len() as u64);
        header.set_mode(mode);
        header.set_cksum();
        builder.append(&header, content).expect("entry");
        let encoder = builder.into_inner().expect("finish archive");
        encoder.finish().expect("finish gzip");
    }

    #[test]
    fn archive_install_is_atomic_executable_and_reusable() {
        let temp = TempDir::new();
        let roots = roots(&temp);
        let archive = temp.0.join("app.tar.gz");
        tar_archive(&archive, "top/bin/app", b"#!/bin/sh\nexit 0\n", 0o644);
        let outcome = install_resolved_at(&resolved(), &archive, &roots).expect("install archive");
        let OfficialOutcome::PortableInstalled { path, launch_path } = outcome else {
            panic!("wrong result");
        };
        assert_eq!(
            std::fs::read(path.join("bin/app")).expect("entry"),
            b"#!/bin/sh\nexit 0\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_ne!(
                std::fs::metadata(path.join("bin/app"))
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o111,
                0
            );
            assert_eq!(
                std::fs::read_link(&launch_path).expect("link"),
                path.join("bin/app")
            );
        }
        let before = std::fs::metadata(path.join("bin/app"))
            .expect("entry metadata")
            .modified()
            .expect("modified");
        install_resolved_at(&resolved(), &archive, &roots).expect("reuse installation");
        assert_eq!(
            std::fs::metadata(path.join("bin/app"))
                .expect("entry metadata")
                .modified()
                .expect("modified"),
            before
        );
        assert!(!path.parent().expect("parent").join(".installing").exists());
        assert!(
            !std::fs::read_dir(path.parent().expect("parent"))
                .expect("list")
                .any(|entry| entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".installing"))
        );
    }

    #[test]
    fn dangerous_archive_paths_and_entry_failures_leave_no_version() {
        for name in ["../escape", "top\\escape.exe"] {
            let temp = TempDir::new();
            let roots = roots(&temp);
            let archive = temp.0.join("bad.tar.gz");
            let file = File::create(&archive).expect("archive");
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_size(3);
            header.set_mode(0o644);
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            builder.append(&header, &b"bad"[..]).expect("entry");
            builder
                .into_inner()
                .expect("finish")
                .finish()
                .expect("gzip");
            assert!(
                install_resolved_at(&resolved(), &archive, &roots).is_err(),
                "{name}"
            );
            assert_no_install(&roots);
        }
        let temp = TempDir::new();
        let roots = roots(&temp);
        let archive = temp.0.join("absolute.zip");
        {
            let file = File::create(&archive).expect("zip");
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("/absolute", zip::write::SimpleFileOptions::default())
                .expect("entry");
            std::io::Write::write_all(&mut writer, b"bad").expect("write");
            writer.finish().expect("zip finish");
        }
        let mut spec = resolved();
        spec.format = OfficialPackageFormat::Zip;
        assert!(install_resolved_at(&spec, &archive, &roots).is_err());
        assert_no_install(&roots);

        let temp = TempDir::new();
        let missing_roots = self::roots(&temp);
        let archive = temp.0.join("missing.tar.gz");
        tar_archive(&archive, "top/other", b"bad", 0o644);
        assert!(install_resolved_at(&resolved(), &archive, &missing_roots).is_err());
        assert_no_install(&missing_roots);
    }

    fn assert_no_install(roots: &Roots) {
        assert!(!roots.apps().join("test-app").exists());
    }

    #[test]
    fn zip_installs_and_ignores_unrelated_executable_bits() {
        let temp = TempDir::new();
        let roots = roots(&temp);
        let archive = temp.0.join("app.zip");
        {
            let file = File::create(&archive).expect("zip");
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer.add_directory("top/bin", options).expect("directory");
            writer.start_file("top/bin/app", options).expect("entry");
            std::io::Write::write_all(&mut writer, b"#!/bin/sh\nexit 0\n").expect("write");
            writer
                .start_file(
                    "top/README",
                    zip::write::SimpleFileOptions::default().unix_permissions(0o755),
                )
                .expect("README");
            std::io::Write::write_all(&mut writer, b"not executable").expect("write");
            writer.finish().expect("zip finish");
        }
        let mut spec = resolved();
        spec.format = OfficialPackageFormat::Zip;
        let outcome = install_resolved_at(&spec, &archive, &roots).expect("install ZIP");
        let OfficialOutcome::PortableInstalled { path, .. } = outcome else {
            panic!("wrong result");
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                std::fs::metadata(path.join("README"))
                    .expect("README")
                    .permissions()
                    .mode()
                    & 0o111,
                0
            );
        }
    }

    #[test]
    fn links_duplicates_and_entry_limits_fail_without_artifacts() {
        for kind in [tar::EntryType::Symlink, tar::EntryType::Link] {
            let temp = TempDir::new();
            let roots = roots(&temp);
            let archive = temp.0.join("link.tar.gz");
            let file = File::create(&archive).expect("archive");
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_path("top/bin/app").expect("path");
            header.set_entry_type(kind);
            header.set_size(0);
            header.set_mode(0o777);
            header.set_link_name("/etc/passwd").expect("link");
            header.set_cksum();
            builder.append(&header, std::io::empty()).expect("entry");
            builder
                .into_inner()
                .expect("finish")
                .finish()
                .expect("gzip");
            assert!(install_resolved_at(&resolved(), &archive, &roots).is_err());
            assert_no_install(&roots);
        }

        let temp = TempDir::new();
        let roots = roots(&temp);
        let archive = temp.0.join("duplicate.tar.gz");
        let file = File::create(&archive).expect("archive");
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_path("top/bin/app").expect("path");
        header.set_size(3);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, &b"one"[..]).expect("first");
        builder.append(&header, &b"two"[..]).expect("duplicate");
        builder
            .into_inner()
            .expect("finish")
            .finish()
            .expect("gzip");
        assert!(install_resolved_at(&resolved(), &archive, &roots).is_err());
        assert_no_install(&roots);

        let mut entries = Entries::default();
        entries
            .add("one".to_owned(), false, LIMIT)
            .expect("limit entry");
        assert!(entries.add("two".to_owned(), false, 1).is_err());
        assert!(entries.add("one".to_owned(), false, 0).is_err());
        let mut entries = Entries::default();
        for index in 0..MAX_ENTRIES {
            entries
                .add(index.to_string(), false, 0)
                .expect("bounded entry");
        }
        assert!(entries.add("overflow".to_owned(), false, 0).is_err());
    }

    #[test]
    fn archive_path_rules_reject_traversal_backslash_and_bad_links() {
        for path in [
            "/etc/passwd",
            "../passwd",
            "a/../../passwd",
            "a\\b",
            "a/./../b",
        ] {
            assert!(safe_archive_path(path, 0).is_err(), "{path}");
        }
        assert_eq!(
            safe_archive_path("top/bin/app", 1).expect("safe path"),
            Some("bin/app".to_owned())
        );
        assert!(!valid_link_name("../app"));
    }

    #[test]
    fn zip_symlinks_are_rejected_without_leaving_staging() {
        let temp = TempDir::new();
        let roots = roots(&temp);
        let archive = temp.0.join("link.zip");
        {
            let file = File::create(&archive).expect("zip");
            let mut writer = zip::ZipWriter::new(file);
            writer
                .add_symlink(
                    "top/bin/app",
                    "/etc/passwd",
                    zip::write::SimpleFileOptions::default(),
                )
                .expect("symlink entry");
            writer.finish().expect("zip finish");
        }
        let mut spec = resolved();
        spec.format = OfficialPackageFormat::Zip;
        assert!(install_resolved_at(&spec, &archive, &roots).is_err());
        assert_no_install(&roots);
    }

    #[test]
    fn failed_rename_cleans_staging_without_overwriting_target() {
        let temp = TempDir::new();
        let roots = roots(&temp);
        let target = roots.apps().join("test-app/v1.2.3");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("parent");
        let staging = temporary_path(target.parent().expect("parent"), ".installing");
        std::fs::create_dir(&staging).expect("staging");
        std::fs::create_dir(&target).expect("competing target");
        std::fs::create_dir_all(target.join("bin")).expect("existing entry directory");
        std::fs::write(target.join("bin/app"), b"old").expect("existing entry");
        assert!(commit_staging(&staging, &target).is_err());
        assert!(!staging.exists());
        assert_eq!(
            std::fs::read(target.join("bin/app")).expect("preserved entry"),
            b"old"
        );
        assert!(!staging.exists());
    }

    #[test]
    fn app_image_is_installed_not_executed() {
        let temp = TempDir::new();
        let roots = roots(&temp);
        let source = temp.0.join("Test.AppImage");
        std::fs::write(&source, b"fixture app image").expect("fixture");
        let mut resolved = resolved();
        resolved.format = OfficialPackageFormat::AppImage;
        resolved.file_name = "Test.AppImage".to_owned();
        resolved.install = OfficialInstallConfig::AppImage {
            link_name: "test-app".to_owned(),
            desktop_name: Some("Test App".to_owned()),
        };
        let outcome =
            install_resolved_at(&resolved, &source, &roots).expect("AppImage installation");
        let OfficialOutcome::PortableInstalled { path, .. } = outcome else {
            panic!("wrong outcome")
        };
        assert_eq!(
            std::fs::read(path.join("Test.AppImage")).expect("installed file"),
            b"fixture app image"
        );
        if cfg!(target_os = "linux") {
            let desktop =
                std::fs::read_to_string(roots.data.join("applications/pakkit-test-app.desktop"))
                    .expect("desktop");
            assert!(desktop.contains("Type=Application\nName=Test App\n"));
        }
    }
}
