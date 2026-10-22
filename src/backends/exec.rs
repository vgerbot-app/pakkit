//! Process execution shared by every backend.
//!
//! Each backend owns its executable discovery and argument interleaving; this
//! module only spawns a process and captures its output. On Windows the child
//! must not allocate a console window (`CREATE_NO_WINDOW`) or a black window
//! flashes on screen for console-subsystem binaries like winget.

use std::{
    path::Path,
    process::{Command, Stdio},
};

use super::types::BackendError;

/// `CREATE_NO_WINDOW` — run the child without allocating a console.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Captured result of one CLI invocation.
#[derive(Debug, Clone)]
pub struct Output {
    /// Raw process exit code, widened from the i32 `ExitStatus::code`.
    pub status: u32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    /// Combined output, stdout first — some CLIs split notices across both.
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
}

/// Spawn `exe` with `env` extra variables and `args`, capturing output.
///
/// Stdin is null so a child that unexpectedly prompts gets EOF instead of
/// blocking the UI thread's background task forever.
pub fn run_process(
    exe: &Path,
    env: &[(&str, &str)],
    args: &[&str],
) -> Result<Output, BackendError> {
    let mut command = Command::new(exe);
    command.args(args);
    for (key, value) in env {
        command.env(key, value);
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

    let proc_output = command.output().map_err(BackendError::Spawn)?;

    Ok(Output {
        status: proc_output.status.code().unwrap_or(0) as u32,
        stdout: String::from_utf8_lossy(&proc_output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&proc_output.stderr).into_owned(),
    })
}

/// First non-empty, non-banner line of a failed run.
///
/// The skipped banners are winget's; other backends simply never print them.
pub fn first_message(output: &Output) -> String {
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
