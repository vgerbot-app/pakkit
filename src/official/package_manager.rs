//! Independent, fixed package-manager execution for official catalog entries.

use std::path::{Path, PathBuf};

use crate::backends::types::InstallOptions;
use crate::backends::{BackendKind, PackageBackend, detect, exec};

use super::{OfficialError, OfficialOs, OfficialOutcome, OfficialPackageManager};

pub fn run(
    manager: OfficialPackageManager,
    package_id: &str,
    cask: bool,
) -> Result<OfficialOutcome, OfficialError> {
    let host =
        OfficialOs::host().ok_or_else(|| OfficialError::Process("无法识别当前系统".to_owned()))?;
    if !manager.supports_os(host) {
        return Err(OfficialError::Process(format!(
            "{} 不支持 {}",
            manager.label(),
            host.label()
        )));
    }
    if manager == OfficialPackageManager::Npm {
        let program = find_on_path(if cfg!(windows) { "npm.cmd" } else { "npm" })
            .ok_or_else(|| missing_manager(manager, package_id))?;
        return run_npm_at(&program, package_id);
    }
    let kind = match manager {
        OfficialPackageManager::Winget => BackendKind::Winget,
        OfficialPackageManager::Brew => BackendKind::Brew,
        OfficialPackageManager::Flatpak => BackendKind::Flatpak,
        OfficialPackageManager::Npm => unreachable!(),
    };
    let backend = detect(kind, "").map_err(|_| missing_manager(manager, package_id))?;
    let args = backend_args(&*backend, package_id, manager, cask);
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = backend.run(&refs).map_err(|error| {
        OfficialError::Process(format!(
            "{} {package_id}：{}",
            manager.label(),
            error.localized()
        ))
    })?;
    checked_output(manager, package_id, output)
}

fn run_npm_at(program: &Path, package_id: &str) -> Result<OfficialOutcome, OfficialError> {
    let args = npm_args(package_id);
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = exec::run_process(program, &[], &refs).map_err(|error| {
        OfficialError::Process(format!("npm {package_id}：{}", error.localized()))
    })?;
    checked_output(OfficialPackageManager::Npm, package_id, output)
}

fn missing_manager(manager: OfficialPackageManager, package_id: &str) -> OfficialError {
    OfficialError::Process(tr!(
        "official.error.manager_missing",
        manager = manager.label(),
        package_id = package_id
    ))
}

fn backend_args(
    backend: &dyn PackageBackend,
    package_id: &str,
    manager: OfficialPackageManager,
    cask: bool,
) -> Vec<String> {
    let id = if manager == OfficialPackageManager::Brew && cask {
        format!("cask/{package_id}")
    } else {
        package_id.to_owned()
    };
    let source = match manager {
        OfficialPackageManager::Winget => Some("winget".to_owned()),
        OfficialPackageManager::Flatpak => Some("flathub".to_owned()),
        _ => None,
    };
    backend.install_args(
        &id,
        &InstallOptions {
            source,
            location: None,
            silent: false,
            accept: true,
        },
    )
}

fn npm_args(package_id: &str) -> Vec<String> {
    vec![
        "install".to_owned(),
        "--global".to_owned(),
        format!("{package_id}@latest"),
    ]
}

fn checked_output(
    manager: OfficialPackageManager,
    package_id: &str,
    output: exec::Output,
) -> Result<OfficialOutcome, OfficialError> {
    let log = output.combined();
    if output.status != 0 || output.signal_terminated {
        return Err(OfficialError::Process(format!(
            "{} {package_id} 退出码 {}{}",
            manager.label(),
            output.status,
            if log.is_empty() {
                String::new()
            } else {
                format!("\n{log}")
            }
        )));
    }
    Ok(OfficialOutcome::PackageManagerInstalled {
        manager: manager.label().to_owned(),
        package_id: package_id.to_owned(),
        log,
    })
}

fn find_on_path(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|directory| directory.join(program))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::{brew::Brew, flatpak::Flatpak, winget::Winget};

    #[test]
    fn fixed_backend_arguments_never_accept_arbitrary_options() {
        let winget = Winget::new("winget");
        assert_eq!(
            backend_args(
                &winget,
                "ZedIndustries.Zed",
                OfficialPackageManager::Winget,
                false,
            ),
            [
                "install",
                "--id",
                "ZedIndustries.Zed",
                "--exact",
                "--source",
                "winget",
                "--accept-package-agreements"
            ]
        );
        let brew = Brew::new("brew");
        assert_eq!(
            backend_args(&brew, "ollama", OfficialPackageManager::Brew, true),
            ["install", "--cask", "ollama"]
        );
        assert_eq!(
            backend_args(&brew, "btop", OfficialPackageManager::Brew, false),
            ["install", "btop"]
        );
        let flatpak = Flatpak::new("flatpak");
        assert_eq!(
            backend_args(
                &flatpak,
                "org.mozilla.firefox",
                OfficialPackageManager::Flatpak,
                false,
            ),
            ["install", "--assumeyes", "flathub", "org.mozilla.firefox"]
        );
        assert_eq!(
            npm_args("@google/gemini-cli"),
            ["install", "--global", "@google/gemini-cli@latest"]
        );
    }

    #[test]
    fn nonzero_and_signalled_processes_include_full_output() {
        let output = exec::Output {
            status: 17,
            stdout: "stdout detail".to_owned(),
            stderr: "stderr detail".to_owned(),
            signal_terminated: false,
        };
        let error = checked_output(OfficialPackageManager::Npm, "pkg", output)
            .expect_err("nonzero must fail")
            .to_string();
        assert!(error.contains("17"));
        assert!(error.contains("stdout detail"));
        assert!(error.contains("stderr detail"));
        assert!(
            checked_output(
                OfficialPackageManager::Npm,
                "pkg",
                exec::Output {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                    signal_terminated: true,
                },
            )
            .is_err()
        );
    }

    #[test]
    fn zero_exit_returns_the_package_manager_log() {
        let outcome = checked_output(
            OfficialPackageManager::Flatpak,
            "org.example.App",
            exec::Output {
                status: 0,
                stdout: "installed".to_owned(),
                stderr: "notice".to_owned(),
                signal_terminated: false,
            },
        )
        .expect("successful install");
        assert!(outcome.should_detect());
        let OfficialOutcome::PackageManagerInstalled { log, .. } = outcome else {
            panic!("wrong outcome");
        };
        assert_eq!(log, "installed\nnotice");
    }

    #[test]
    #[cfg(unix)]
    fn npm_execution_preserves_arguments_and_output() {
        use std::os::unix::fs::PermissionsExt as _;
        let temp = std::env::temp_dir().join(format!(
            "pakkit-npm-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir(&temp).expect("temporary executable directory");
        let program = temp.join("npm");
        let log = temp.join("args");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nprintf installed\nprintf notice >&2\n",
                log.display()
            ),
        )
        .expect("fake npm");
        let mut permissions = std::fs::metadata(&program).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&program, permissions).expect("executable");
        let outcome = run_npm_at(&program, "@google/gemini-cli").expect("fake npm execution");
        assert_eq!(
            std::fs::read_to_string(log).expect("arguments"),
            "install\n--global\n@google/gemini-cli@latest\n"
        );
        let OfficialOutcome::PackageManagerInstalled { log, .. } = outcome else {
            panic!("wrong result");
        };
        assert!(log.contains("installed"));
        assert!(log.contains("notice"));
        std::fs::remove_dir_all(temp).expect("remove temporary directory");
    }

    #[test]
    fn missing_npm_has_an_actionable_error() {
        let error = missing_manager(OfficialPackageManager::Npm, "@google/gemini-cli");
        assert!(error.to_string().contains("npm"));
        assert!(error.to_string().contains("@google/gemini-cli"));
    }
}
