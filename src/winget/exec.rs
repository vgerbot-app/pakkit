//! Process execution for the winget CLI.
//!
//! winget is a console-subsystem binary, so every spawn must suppress its
//! console window (`CREATE_NO_WINDOW`) or a black window flashes on screen.

use std::{
    path::{Path, PathBuf},
    process::{Command, Output as ProcOutput, Stdio},
};

use super::exit::{WingetError, is_benign};

/// Flags every command accepts except `source`, which takes no agreement
/// flags at all.
const COMMON_FLAGS: &[&str] = &[
    "--disable-interactivity",
    "--accept-source-agreements",
    "--nowarn",
];

/// `winget source …` is the one family with no agreement flags.
const SOURCE_FLAGS: &[&str] = &["--disable-interactivity", "--nowarn"];

/// The flags `command` accepts.
///
/// winget validates options per command and rejects the whole invocation with
/// "Argument name was not recognized" when handed one it does not know, so
/// this is derived from the subcommand rather than applied blindly.
///
/// `--accept-package-agreements` is deliberately absent: only the install
/// family accepts it, and it must stay tied to the user's consent toggle, so
/// [`super::install`] emits it. Notably absent on purpose: `--output-locale`,
/// `--no-progress` and `--no-vt` are *not* winget flags — passing them is a
/// hard argument error — so locale-stable parsing reads the header row instead
/// (see [`super::parse`]).
fn base_flags_for(command: &str) -> &'static [&'static str] {
    match command {
        "source" => SOURCE_FLAGS,
        _ => COMMON_FLAGS,
    }
}

/// `CREATE_NO_WINDOW` — run the child without allocating a console.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Captured result of one winget invocation.
#[derive(Debug, Clone)]
pub struct Output {
    /// Raw process exit code, widened from the i32 `ExitStatus::code`.
    pub status: u32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    /// Combined output, stdout first — winget splits notices across both.
    pub fn combined(&self) -> String {
        let mut s = self.stdout.clone();
        if !self.stderr.is_empty() {
            if !s.is_empty() && !s.ends_with('\n') {
                s.push('\n');
            }
            s.push_str(&self.stderr);
        }
        s
    }

    pub fn success(&self) -> bool {
        self.status == 0 || is_benign(self.status)
    }
}

/// A resolved winget executable.
#[derive(Debug, Clone)]
pub struct Winget {
    exe: PathBuf,
}

impl Winget {
    /// Wrap an explicit executable path.
    pub fn new(exe: impl Into<PathBuf>) -> Self {
        Self { exe: exe.into() }
    }

    /// Locate `winget.exe`: `PATH` first, then the per-user WindowsApps alias.
    pub fn discover() -> Result<Self, WingetError> {
        if let Some(path) = Self::on_path() {
            return Ok(Self::new(path));
        }
        if let Some(path) = Self::windows_apps_alias()
            && path.is_file()
        {
            return Ok(Self::new(path));
        }
        Err(WingetError::NotFound)
    }

    /// Honor an operator-supplied path, falling back to discovery.
    ///
    /// An empty configured path means "auto-detect".
    pub fn resolve(configured: &str) -> Result<Self, WingetError> {
        let trimmed = configured.trim();
        if trimmed.is_empty() {
            return Self::discover();
        }
        let path = PathBuf::from(trimmed);
        if path.is_file() {
            Ok(Self::new(path))
        } else {
            Err(WingetError::InvalidPath(path))
        }
    }

    pub fn exe(&self) -> &Path {
        &self.exe
    }

    fn on_path() -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("winget.exe"))
            .find(|candidate| candidate.is_file())
    }

    fn windows_apps_alias() -> Option<PathBuf> {
        let local = std::env::var_os("LOCALAPPDATA")?;
        Some(
            PathBuf::from(local)
                .join("Microsoft")
                .join("WindowsApps")
                .join("winget.exe"),
        )
    }

    /// Spawn winget with the command's agreement flags plus `args`.
    pub fn run(&self, args: &[&str]) -> Result<Output, WingetError> {
        self.run_with(args, true)
    }

    /// Spawn winget with exactly `args`, no base flags.
    ///
    /// Used by `--version`, which rejects the interleaved base flags.
    pub fn run_bare(&self, args: &[&str]) -> Result<Output, WingetError> {
        self.run_with(args, false)
    }

    fn run_with(&self, args: &[&str], base_flags: bool) -> Result<Output, WingetError> {
        let mut command = Command::new(&self.exe);
        if base_flags && let Some((verb_or_command, rest)) = args.split_first() {
            command.arg(*verb_or_command);
            // `source` carries a sub-verb; its flags follow the sub-verb.
            let (command_name, rest) = match (*verb_or_command, rest.split_first()) {
                ("source", Some((verb, rest))) => {
                    command.arg(*verb);
                    ("source", rest)
                }
                _ => (*verb_or_command, rest),
            };
            command.args(base_flags_for(command_name));
            command.args(rest);
        } else {
            command.args(args);
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // `creation_flags` is a safe API on the Windows `CommandExt` trait, so
        // this needs no `unsafe` block (the crate denies `unsafe_code`).
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let ProcOutput {
            status,
            stdout,
            stderr,
        } = command.output().map_err(WingetError::Spawn)?;

        Ok(Output {
            status: status.code().unwrap_or(0) as u32,
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    }

    /// Run and fail on a non-benign non-zero exit code.
    ///
    /// Benign codes (`0x8A150061`, `0x8A15010D`, `0x8A15002B`, `0x8A150109`)
    /// describe an already-satisfied end state and are returned as success.
    pub fn run_checked(&self, args: &[&str]) -> Result<Output, WingetError> {
        let output = self.run(args)?;
        if output.success() {
            Ok(output)
        } else {
            Err(WingetError::Exit {
                code: output.status,
                message: first_message(&output),
            })
        }
    }

    /// `winget --version`, e.g. `v1.29.380`.
    pub fn version(&self) -> Result<String, WingetError> {
        let output = self.run_bare(&["--version"])?;
        if !output.success() {
            return Err(WingetError::Exit {
                code: output.status,
                message: first_message(&output),
            });
        }
        Ok(output
            .stdout
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default()
            .to_owned())
    }
}

/// First non-empty, non-banner line of a failed winget run.
fn first_message(output: &Output) -> String {
    output
        .combined()
        .lines()
        .map(str::trim)
        .find(|line| {
            !line.is_empty()
                && !line.starts_with("Windows Package Manager v")
                && !line.starts_with('©')
        })
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winget::exit::format_code;

    #[test]
    fn flags_never_contain_options_winget_rejects() {
        // These are not winget options; including any of them makes every
        // invocation fail with "Argument name was not recognized".
        for flags in [COMMON_FLAGS, SOURCE_FLAGS] {
            for bad in [
                "--output-locale",
                "--no-progress",
                "--no-vt",
                "--accept-package-agreements",
            ] {
                assert!(!flags.contains(&bad), "{bad} must not be in {flags:?}");
            }
        }
    }

    #[test]
    fn source_takes_no_agreement_flags() {
        // `winget source …` rejects every agreement flag, so the source family
        // would otherwise fail with "Argument name was not recognized".
        assert_eq!(base_flags_for("source"), SOURCE_FLAGS);
        assert!(!SOURCE_FLAGS.contains(&"--accept-source-agreements"));
        // Everything else takes the source agreement.
        for command in ["list", "search", "show", "export", "install", "upgrade"] {
            assert_eq!(base_flags_for(command), COMMON_FLAGS, "{command}");
            assert!(COMMON_FLAGS.contains(&"--accept-source-agreements"));
        }
    }

    #[test]
    fn first_message_skips_the_version_banner() {
        let output = Output {
            status: 0x8A15_0019,
            stdout: String::new(),
            stderr: "Windows Package Manager v1.29.380\n© 2026 Microsoft.\n\n需要管理员权限\n"
                .into(),
        };
        assert_eq!(first_message(&output), "需要管理员权限");
    }

    #[test]
    fn format_code_round_trips_a_real_error() {
        assert_eq!(format_code(0x8A15_0011), "0x8A150011");
    }
}
