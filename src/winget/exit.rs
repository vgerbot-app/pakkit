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

/// Human-readable description for the exit codes a GUI must surface.
///
/// Codes are the `APPINSTALLER_CLI_ERROR_*` values winget returns in the low
/// 32 bits of its process exit status.
pub const fn describe(code: u32) -> &'static str {
    match code {
        0x8A15_0001 => "内部错误",
        0x8A15_0002 => "参数无效",
        0x8A15_0014 => "未找到匹配的软件包",
        0x8A15_0016 => "匹配到多个软件包",
        0x8A15_0019 => "需要管理员权限",
        0x8A15_002B => "没有可用的更新",
        0x8A15_002C => "升级完成，但部分软件包失败",
        0x8A15_003A => "被策略阻止",
        0x8A15_004F => "当前版本不比已安装版本新",
        0x8A15_0050 => "无法确定版本",
        0x8A15_0061 => "该软件包已安装",
        0x8A15_0065 => "一个或多个安装失败",
        0x8A15_0066 => "一个或多个卸载失败",
        0x8A15_0068 => "该软件包已被固定（pin）",
        0x8A15_0101 => "软件包正在使用中",
        0x8A15_0102 => "已有安装正在进行",
        0x8A15_0105 => "磁盘空间不足",
        0x8A15_0107 => "网络不可用",
        0x8A15_0109 => "需要重启以完成操作",
        0x8A15_010C => "操作已取消",
        0x8A15_010D => "已安装其他版本",
        0x8A15_010E => "不允许降级",
        0x8A15_010F => "被策略阻止",
        0x8A15_0114 => "该软件包不支持升级",
        _ => "未知错误",
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
        assert!(!is_benign(0x8A15_0019));
        assert!(needs_elevation(0x8A15_0019));
        assert_eq!(describe(0x8A15_0019), "需要管理员权限");
    }

    #[test]
    fn unknown_codes_have_a_fallback_description() {
        assert_eq!(describe(0x1234_5678), "未知错误");
    }

    #[test]
    fn format_matches_winget_notation() {
        assert_eq!(format_code(0x8A15_0011), "0x8A150011");
    }
}
