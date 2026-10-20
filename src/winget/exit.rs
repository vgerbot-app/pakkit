//! Typed errors and winget exit-code decoding.

use std::path::PathBuf;

use thiserror::Error;

/// Everything that can go wrong talking to the winget CLI.
#[derive(Debug, Error)]
pub enum WingetError {
    /// No `winget.exe` could be located on this machine.
    #[error("未找到 winget.exe（请确认已安装“应用安装程序”）")]
    NotFound,

    /// The process could not be started at all.
    #[error("无法启动 winget：{0}")]
    Spawn(#[source] std::io::Error),

    /// A table could not be understood.
    #[error("无法解析 winget 输出：{0}")]
    Parse(String),

    /// winget ran and returned a non-benign non-zero exit code.
    #[error("winget 退出码 {code:#010X}：{message}")]
    Exit { code: u32, message: String },

    /// `winget export`/`source export` produced invalid JSON.
    #[error("winget JSON 输出无效：{0}")]
    Json(#[from] serde_json::Error),

    /// A path could not be prepared (e.g. the export temp file).
    #[error("文件操作失败：{0}")]
    Io(#[from] std::io::Error),

    /// An explicit winget path was configured but does not exist.
    #[error("配置的 winget 路径不存在：{}", .0.display())]
    InvalidPath(PathBuf),
}

impl WingetError {
    /// The message in the current UI language. `Display` keeps the base-locale
    /// text so logs stay greppable regardless of the interface language.
    pub fn localized(&self) -> String {
        match self {
            WingetError::NotFound => tr!("error.not_found"),
            WingetError::Spawn(error) => tr!("error.spawn", error = error.to_string()),
            WingetError::Parse(message) => tr!("error.parse", error = message.clone()),
            WingetError::Exit { code, message } => {
                tr!("error.exit", code = format_code(*code), message = message.clone())
            }
            WingetError::Json(error) => tr!("error.json", error = error.to_string()),
            WingetError::Io(error) => tr!("error.io", error = error.to_string()),
            WingetError::InvalidPath(path) => {
                tr!("error.invalid_path", path = path.display().to_string())
            }
        }
    }
}

/// Human-readable description for the exit codes a GUI must surface.
///
/// Codes are the `APPINSTALLER_CLI_ERROR_*` values winget returns in the low
/// 32 bits of its process exit status.
pub fn describe(code: u32) -> String {
    match code {
        0x8A15_0001 => tr!("exitcode.internal"),
        0x8A15_0002 => tr!("exitcode.invalid_argument"),
        0x8A15_0014 => tr!("exitcode.package_not_found"),
        0x8A15_0016 => tr!("exitcode.multiple_packages"),
        0x8A15_0019 => tr!("exitcode.needs_elevation"),
        0x8A15_002B => tr!("exitcode.no_update"),
        0x8A15_002C => tr!("exitcode.upgrade_partial"),
        0x8A15_003A => tr!("exitcode.blocked_by_policy"),
        0x8A15_004F => tr!("exitcode.not_newer"),
        0x8A15_0050 => tr!("exitcode.version_unknown"),
        0x8A15_0061 => tr!("exitcode.already_installed"),
        0x8A15_0065 => tr!("exitcode.install_failed_some"),
        0x8A15_0066 => tr!("exitcode.uninstall_failed_some"),
        0x8A15_0068 => tr!("exitcode.pinned"),
        0x8A15_0101 => tr!("exitcode.in_use"),
        0x8A15_0102 => tr!("exitcode.install_in_progress"),
        0x8A15_0105 => tr!("exitcode.disk_full"),
        0x8A15_0107 => tr!("exitcode.network_unavailable"),
        0x8A15_0109 => tr!("exitcode.reboot_required"),
        0x8A15_010C => tr!("exitcode.cancelled"),
        0x8A15_010D => tr!("exitcode.other_version_installed"),
        0x8A15_010E => tr!("exitcode.no_downgrade"),
        0x8A15_010F => tr!("exitcode.blocked_by_policy"),
        0x8A15_0114 => tr!("exitcode.no_upgrade_support"),
        _ => tr!("exitcode.unknown"),
    }
}

/// Codes that mean "the desired end state already holds" and must not be
/// reported to the user as failures.
pub const fn is_benign(code: u32) -> bool {
    matches!(
        code,
        0x8A15_0061 // 已安装
            | 0x8A15_010D // 已安装其他版本
            | 0x8A15_002B // 没有可用更新
            | 0x8A15_0109 // 需要重启（视为成功）
    )
}

/// Codes that specifically mean the operation needs an elevated shell.
pub const fn needs_elevation(code: u32) -> bool {
    code == 0x8A15_0019
}

/// Format an exit code the way winget prints it in its own messages.
pub fn format_code(code: u32) -> String {
    format!("{code:#010X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benign_codes_are_the_end_state_codes() {
        for code in [0x8A15_0061, 0x8A15_010D, 0x8A15_002B, 0x8A15_0109] {
            assert!(is_benign(code), "{code:#010X} must be benign");
        }
        for code in [0x8A15_0019, 0x8A15_0065, 0x8A15_0001, 0x8A15_0102] {
            assert!(!is_benign(code), "{code:#010X} must not be benign");
        }
    }

    #[test]
    fn elevation_is_not_benign_and_describes_as_such() {
        crate::ensure_zh_locale();
        assert!(!is_benign(0x8A15_0019));
        assert!(needs_elevation(0x8A15_0019));
        assert_eq!(describe(0x8A15_0019), "需要管理员权限");
    }

    #[test]
    fn unknown_codes_have_a_fallback_description() {
        crate::ensure_zh_locale();
        assert_eq!(describe(0x1234_5678), "未知错误");
    }

    #[test]
    fn format_matches_winget_notation() {
        assert_eq!(format_code(0x8A15_0011), "0x8A150011");
    }
}
