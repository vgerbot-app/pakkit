//! PackageKit D-Bus client: the privileged execution path for apt and
//! pacman mutations.
//!
//! Instead of wrapping every mutation in `pkexec` (one polkit prompt per
//! spawned process), mutations go to the `org.freedesktop.PackageKit` system
//! service. Its polkit actions `package-install`/`system-update` are
//! `auth_admin_keep`, so one authorization covers a burst of queue items for
//! ~5 minutes, and a single transaction carries any number of packages.
//! `package-remove` is plain `auth_admin` — each removal prompts once.
//!
//! The client is blocking (zbus `blocking` feature): callers already offload
//! `PackageBackend::run` onto background threads, so a synchronous D-Bus
//! round-trip per transaction fits the existing executor model. A fresh
//! system-bus connection is opened per call — about a millisecond on a local
//! unix socket — which sidesteps sharing a connection across the `Arc`-shared
//! backend's threads.
//!
//! If the daemon disappears mid-transaction the drain loop blocks forever,
//! same as a hung `pkexec` child today; the daemon is D-Bus-activatable and
//! supervised by systemd, so this is accepted rather than adding a timeout.

use std::fmt;

use zbus::{
    MatchRule,
    blocking::{self, MessageIterator},
    message::Type,
    zvariant::OwnedObjectPath,
};

use super::exec::Output;

/// Well-known bus name of the PackageKit daemon.
const DESTINATION: &str = "org.freedesktop.PackageKit";
/// Root object: transaction factory and daemon probes.
const ROOT_PATH: &str = "/org/freedesktop/PackageKit";
/// Transaction interface (also the factory interface on the root object).
const TRANSACTION_IFACE: &str = "org.freedesktop.PackageKit.Transaction";

// PkExitEnum values relevant to the mapping below.
const PK_EXIT_SUCCESS: u32 = 1;
const PK_EXIT_CANCELLED: u32 = 3;

// PkInfoEnum values carried by `Package` signals from Resolve.
const PK_INFO_INSTALLED: u32 = 1;
const PK_INFO_AVAILABLE: u32 = 2;

/// Exit code for a dismissed/cancelled authorization, mirroring pkexec's 126.
const STATUS_AUTH_DISMISSED: u32 = 126;
/// Exit code for a denied authorization, mirroring pkexec's 127.
const STATUS_AUTH_DENIED: u32 = 127;

/// A mutation expressed as a PackageKit operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PkOp {
    /// `install -y <name>` (apt) / `-S --noconfirm <name>` (pacman; pacman's
    /// single-package upgrade is also `-S`, so it maps here too).
    Install { name: String },
    /// `install --only-upgrade -y <name>` — apt's single-package upgrade.
    UpgradeOne { name: String },
    /// `remove -y <name>` (apt) / `-R --noconfirm <name>` (pacman).
    Remove { name: String },
    /// `upgrade -y` (apt) / `-Syu --noconfirm` (pacman).
    UpgradeAll,
    /// `update` (apt) / `-Sy` (pacman) — refresh the package lists.
    RefreshCache,
}

/// Map backend mutation arguments to a PackageKit operation.
///
/// The apt and pacman shapes are disjoint, so one planner serves both
/// backends. Returns `None` for shapes the planner does not recognize; the
/// caller falls back to the pkexec pipeline for those.
pub fn plan(args: &[&str]) -> Option<PkOp> {
    match args {
        ["install", "-y", name] => Some(PkOp::Install {
            name: (*name).to_owned(),
        }),
        ["install", "--only-upgrade", "-y", name] => Some(PkOp::UpgradeOne {
            name: (*name).to_owned(),
        }),
        ["remove", "-y", name] => Some(PkOp::Remove {
            name: (*name).to_owned(),
        }),
        ["upgrade", "-y"] => Some(PkOp::UpgradeAll),
        ["update"] => Some(PkOp::RefreshCache),
        ["-S", "--noconfirm", name] => Some(PkOp::Install {
            name: (*name).to_owned(),
        }),
        ["-R", "--noconfirm", name] => Some(PkOp::Remove {
            name: (*name).to_owned(),
        }),
        ["-Syu", "--noconfirm"] => Some(PkOp::UpgradeAll),
        ["-Sy"] => Some(PkOp::RefreshCache),
        _ => None,
    }
}

/// Split a PackageKit package id (`name;version;arch;data`) into its parts.
///
/// `data` may itself contain `;`, so the split stops after the fourth field.
pub fn parse_package_id(id: &str) -> Option<(&str, &str, &str, &str)> {
    let mut parts = id.splitn(4, ';');
    let name = parts.next()?;
    let version = parts.next()?;
    let arch = parts.next()?;
    let data = parts.next()?;
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name, version, arch, data))
}

/// Map a transaction's `Finished` exit enum to a synthetic process status.
///
/// Success becomes 0 (the only code `queue::complete` treats as done);
/// cancellation becomes 126 to match a dismissed pkexec prompt; everything
/// else is a generic failure.
pub fn exit_to_status(exit: u32) -> u32 {
    match exit {
        PK_EXIT_SUCCESS => 0,
        PK_EXIT_CANCELLED => STATUS_AUTH_DISMISSED,
        _ => 1,
    }
}

/// Why a [`run`] call did not produce a successful transaction.
#[derive(Debug)]
pub enum RunError {
    /// The D-Bus connection or daemon was unreachable; the caller should
    /// fall back to the pkexec pipeline.
    Transport(zbus::Error),
    /// The transaction ran and failed (or was denied); carries the synthetic
    /// process output so the queue can report it like any CLI failure.
    Transaction(Output),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(error) => write!(f, "PackageKit transport: {error}"),
            Self::Transaction(output) => write!(f, "transaction failed: {}", output.combined()),
        }
    }
}

/// Probe whether the PackageKit daemon answers on the system bus.
pub fn available() -> bool {
    let probe = || -> zbus::Result<()> {
        let conn = blocking::Connection::system()?;
        let root = blocking::Proxy::new(&conn, DESTINATION, ROOT_PATH, "org.freedesktop.PackageKit")?;
        let _: String = root.call("GetDaemonState", &())?;
        Ok(())
    };
    probe().is_ok()
}

/// One `Package` signal: info enum, package id, summary.
type PackageSignal = (u32, String, String);

/// Collected results of one finished transaction.
#[derive(Debug, Default)]
struct Outcome {
    exit: u32,
    packages: Vec<PackageSignal>,
    error: Option<String>,
}

/// A live transaction: its proxy plus the signal stream subscribed before
/// any method was invoked (Package signals race the method reply otherwise).
struct Transaction<'c> {
    proxy: blocking::Proxy<'c>,
    signals: MessageIterator,
}

/// Open a transaction on `conn` with interactive hints set.
fn begin(conn: &blocking::Connection) -> zbus::Result<Transaction<'_>> {
    let root = blocking::Proxy::new(conn, DESTINATION, ROOT_PATH, "org.freedesktop.PackageKit")?;
    let path: OwnedObjectPath = root.call("CreateTransaction", &())?;
    // Owned copies keep the proxy and match rule independent of `path`.
    let path_string = path.as_str().to_owned();
    let proxy = blocking::Proxy::new(conn, DESTINATION, path_string.clone(), TRANSACTION_IFACE)?;

    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(TRANSACTION_IFACE)?
        .path(path_string)?
        .build();
    let signals = MessageIterator::for_match_rule(rule, conn, None)?;

    let locale = sys_locale::get_locale().unwrap_or_else(|| "C".to_owned());
    let hints = vec![
        "interactive=true".to_owned(),
        "background=false".to_owned(),
        format!("locale={locale}"),
    ];
    let _: () = proxy.call("SetHints", &hints)?;

    Ok(Transaction { proxy, signals })
}

/// Run `setup` (the transaction's method call) and drain signals until
/// `Finished`. Auth failures raised as D-Bus error replies from the method
/// call itself are converted into synthetic outputs by the caller via
/// [`method_error_output`].
fn transact<F>(conn: &blocking::Connection, setup: F) -> zbus::Result<Outcome>
where
    F: FnOnce(&blocking::Proxy) -> zbus::Result<()>,
{
    let mut txn = begin(conn)?;
    setup(&txn.proxy)?;

    let mut outcome = Outcome::default();
    for message in txn.signals.by_ref() {
        let message = match message {
            Ok(message) => message,
            Err(_) => break,
        };
        let header = message.header();
        match header.member().map(|m| m.as_str()) {
            Some("Package") => {
                if let Ok(package) = message.body().deserialize::<PackageSignal>() {
                    outcome.packages.push(package);
                }
            }
            Some("ErrorCode") => {
                if let Ok((_code, details)) = message.body().deserialize::<(u32, String)>() {
                    outcome.error = Some(details);
                }
            }
            Some("Finished") => {
                if let Ok((exit, _runtime_ms)) = message.body().deserialize::<(u32, u32)>() {
                    outcome.exit = exit;
                }
                return Ok(outcome);
            }
            _ => {}
        }
    }
    // The stream ended without Finished: the daemon went away.
    outcome.exit = PK_EXIT_CANCELLED + 1; // any non-success, non-cancelled code
    if outcome.error.is_none() {
        outcome.error = Some("PackageKit daemon disconnected mid-transaction".to_owned());
    }
    Ok(outcome)
}

/// Convert a D-Bus error reply from a transaction method call into the
/// synthetic output for auth failures; `None` for genuine transport errors.
fn method_error_output(error: &zbus::Error) -> Option<Output> {
    let zbus::Error::MethodError(name, description, _) = error else {
        return None;
    };
    let name = name.as_str();
    let detail = description.clone().unwrap_or_else(|| name.to_owned());
    if name.contains("AccessDenied") {
        Some(Output {
            status: STATUS_AUTH_DISMISSED,
            stdout: String::new(),
            stderr: detail,
        })
    } else if name.contains("NotAuthorized") {
        Some(Output {
            status: STATUS_AUTH_DENIED,
            stdout: String::new(),
            stderr: detail,
        })
    } else {
        None
    }
}

/// Turn a finished transaction's outcome into synthetic process output.
fn outcome_to_output(outcome: Outcome) -> Output {
    let mut stdout = String::new();
    for (_info, id, summary) in &outcome.packages {
        let line = match parse_package_id(id) {
            Some((name, version, arch, _)) => format!("{name} {version} [{arch}]"),
            None => id.clone(),
        };
        stdout.push_str(&line);
        if !summary.is_empty() {
            stdout.push_str(" — ");
            stdout.push_str(summary);
        }
        stdout.push('\n');
    }
    Output {
        status: exit_to_status(outcome.exit),
        stdout,
        stderr: outcome.error.unwrap_or_default(),
    }
}

/// Keep the update ids whose package name matches; `None` keeps them all.
///
/// Upgrades install the *candidate* version id that `GetUpdates` emits —
/// the id of the installed version does not select an update. And they go
/// through `InstallPackages`, not `UpdatePackages`: PackageKit 1.3.x's apt
/// backend rejects its own GetUpdates ids from UpdatePackages ("not
/// available as an update candidate"), while installing the candidate id
/// is exactly apt's `install --only-upgrade` semantics.
fn matching_update_ids(packages: &[PackageSignal], name: Option<&str>) -> Vec<String> {
    packages
        .iter()
        .filter(|(_, id, _)| {
            name.is_none_or(|name| {
                parse_package_id(id).is_some_and(|(pkg_name, _, _, _)| pkg_name == name)
            })
        })
        .map(|(_, id, _)| id.clone())
        .collect()
}

/// Collect the update ids from `GetUpdates`, optionally narrowed to one
/// package name. An empty result with `Some(name)` is an error — the caller
/// asked to update a package that has no update pending.
fn update_ids(conn: &blocking::Connection, name: Option<&str>) -> Result<Vec<String>, RunError> {
    let outcome = transact(conn, |txn| txn.call("GetUpdates", &0_u64)).map_err(|error| {
        match method_error_output(&error) {
            Some(output) => RunError::Transaction(output),
            None => RunError::Transport(error),
        }
    })?;
    let ids = matching_update_ids(&outcome.packages, name);
    if ids.is_empty() && let Some(name) = name {
        return Err(RunError::Transaction(Output {
            status: 100, // apt's "unable to locate package"
            stdout: String::new(),
            stderr: format!("E: Package {name} has no available update"),
        }));
    }
    Ok(ids)
}

/// Resolve a plain package name to full PackageKit ids, keeping the first
/// signal whose info enum matches `wanted_info` (installed or available).
fn resolve_ids(
    conn: &blocking::Connection,
    name: &str,
    wanted_info: u32,
) -> Result<Vec<String>, RunError> {
    let outcome = transact(conn, |txn| {
        let names = vec![name.to_owned()];
        txn.call("Resolve", &(0_u64, names))
    })
    .map_err(|error| match method_error_output(&error) {
        Some(output) => RunError::Transaction(output),
        None => RunError::Transport(error),
    })?;
    let ids: Vec<String> = outcome
        .packages
        .iter()
        .filter(|(info, _, _)| *info == wanted_info)
        .map(|(_, id, _)| id.clone())
        .collect();
    if ids.is_empty() {
        return Err(RunError::Transaction(Output {
            status: 100, // apt's "unable to locate package"
            stdout: String::new(),
            stderr: format!("E: Unable to locate package {name}"),
        }));
    }
    Ok(ids)
}

/// Run an action transaction over `ids`, mapping the outcome to output.
fn act<F>(conn: &blocking::Connection, setup: F) -> Result<Output, RunError>
where
    F: FnOnce(&blocking::Proxy) -> zbus::Result<()>,
{
    let outcome = transact(conn, setup).map_err(|error| match method_error_output(&error) {
        Some(output) => RunError::Transaction(output),
        None => RunError::Transport(error),
    })?;
    let output = outcome_to_output(outcome);
    if output.status == 0 {
        Ok(output)
    } else {
        Err(RunError::Transaction(output))
    }
}

/// Execute `op` through PackageKit, blocking until the transaction finishes.
pub fn run(op: &PkOp) -> Result<Output, RunError> {
    let conn = blocking::Connection::system().map_err(RunError::Transport)?;
    match op {
        PkOp::Install { name } => {
            let ids = resolve_ids(&conn, name, PK_INFO_AVAILABLE)?;
            act(&conn, |txn| txn.call("InstallPackages", &(0_u64, ids)))
        }
        PkOp::UpgradeOne { name } => {
            let ids = update_ids(&conn, Some(name))?;
            // InstallPackages, not UpdatePackages: the 1.3.x apt backend
            // rejects its own GetUpdates ids from UpdatePackages ("not
            // available as an update candidate"), while installing the
            // candidate version id is exactly `install --only-upgrade`.
            act(&conn, |txn| txn.call("InstallPackages", &(0_u64, ids)))
        }
        PkOp::Remove { name } => {
            let ids = resolve_ids(&conn, name, PK_INFO_INSTALLED)?;
            act(&conn, |txn| {
                // allow_deps mirrors `apt-get remove` (reverse dependencies go
                // too); autoremove stays off, matching the CLI's non-purge,
                // non-autoremove semantics.
                txn.call("RemovePackages", &(0_u64, ids, true, false))
            })
        }
        PkOp::UpgradeAll => {
            let ids = update_ids(&conn, None)?;
            if ids.is_empty() {
                return Ok(Output {
                    status: 0,
                    stdout: "All packages are up to date.\n".to_owned(),
                    stderr: String::new(),
                });
            }
            // InstallPackages here too — see UpgradeOne.
            act(&conn, |txn| txn.call("InstallPackages", &(0_u64, ids)))
        }
        PkOp::RefreshCache => act(&conn, |txn| txn.call("RefreshCache", &false)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_covers_every_mutation_shape() {
        assert_eq!(
            plan(&["install", "-y", "curl"]),
            Some(PkOp::Install {
                name: "curl".to_owned()
            })
        );
        assert_eq!(
            plan(&["install", "--only-upgrade", "-y", "curl"]),
            Some(PkOp::UpgradeOne {
                name: "curl".to_owned()
            })
        );
        assert_eq!(
            plan(&["remove", "-y", "curl"]),
            Some(PkOp::Remove {
                name: "curl".to_owned()
            })
        );
        assert_eq!(plan(&["upgrade", "-y"]), Some(PkOp::UpgradeAll));
        assert_eq!(plan(&["update"]), Some(PkOp::RefreshCache));
    }

    #[test]
    fn plan_covers_the_pacman_mutation_shapes() {
        assert_eq!(
            plan(&["-S", "--noconfirm", "curl"]),
            Some(PkOp::Install {
                name: "curl".to_owned()
            })
        );
        assert_eq!(
            plan(&["-R", "--noconfirm", "curl"]),
            Some(PkOp::Remove {
                name: "curl".to_owned()
            })
        );
        assert_eq!(plan(&["-Syu", "--noconfirm"]), Some(PkOp::UpgradeAll));
        assert_eq!(plan(&["-Sy"]), Some(PkOp::RefreshCache));
    }

    #[test]
    fn plan_rejects_unknown_shapes_for_the_pkexec_fallback() {
        assert_eq!(plan(&["list", "--installed"]), None);
        assert_eq!(plan(&["install", "curl"]), None);
        assert_eq!(plan(&["purge", "-y", "curl"]), None);
        // pacman's read-only queries and its AUR marker shape never plan.
        assert_eq!(plan(&["-Ss", "firefox"]), None);
        assert_eq!(plan(&["--from-aur", "-S", "--noconfirm", "foo"]), None);
        assert_eq!(plan(&[]), None);
    }

    #[test]
    fn package_ids_split_into_four_fields() {
        let id = "curl;8.5.0-2ubuntu10;amd64;noble-updates";
        assert_eq!(
            parse_package_id(id),
            Some(("curl", "8.5.0-2ubuntu10", "amd64", "noble-updates"))
        );
        // `data` keeps its own separators.
        let id = "vim;2:9.1;amd64;repo;extra";
        assert_eq!(
            parse_package_id(id),
            Some(("vim", "2:9.1", "amd64", "repo;extra"))
        );
        assert_eq!(parse_package_id("curl"), None);
        assert_eq!(parse_package_id(";8.0;amd64;x"), None);
    }

    #[test]
    fn update_ids_filter_by_package_name() {
        let packages: Vec<PackageSignal> = vec![
            (9, "curl;8.5.0-2;amd64;noble-updates".to_owned(), String::new()),
            (9, "wget;1.21.4-1;amd64;noble-updates".to_owned(), String::new()),
        ];
        // A name filter keeps only that package's update id — the shape
        // aptcc's UpdatePackages requires (the installed id fails).
        assert_eq!(
            matching_update_ids(&packages, Some("curl")),
            vec!["curl;8.5.0-2;amd64;noble-updates".to_owned()]
        );
        assert!(matching_update_ids(&packages, Some("git")).is_empty());
        // No filter keeps every id, for the batch upgrade.
        assert_eq!(matching_update_ids(&packages, None).len(), 2);
    }

    #[test]
    fn exit_enums_map_to_process_status() {
        assert_eq!(exit_to_status(PK_EXIT_SUCCESS), 0);
        assert_eq!(exit_to_status(PK_EXIT_CANCELLED), STATUS_AUTH_DISMISSED);
        assert_eq!(exit_to_status(2), 1);
        assert_eq!(exit_to_status(0), 1);
    }
}
