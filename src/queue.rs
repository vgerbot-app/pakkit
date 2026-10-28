//! The install queue.
//!
//! winget is not reentrant: two installs at once deadlock on the installer
//! lock, so tasks run strictly serially unless the operator raises 并发任务数.
//! The queue is therefore a small state machine — enqueue, start, finish,
//! remove — kept free of gpui so it can be unit-tested directly. The shell
//! owns the running `Task` handles and does the spawning.

use std::time::Instant;

use crate::backends::{PackageBackend, exec, exec::Output, types::BackendError};
use crate::model::Package;
use crate::state::AppState;

/// Where a queue entry is in its life cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueState {
    Pending,
    Running,
    Done,
    Error,
}

impl QueueState {
    /// The pill text the drawer shows.
    pub fn label(self) -> String {
        match self {
            QueueState::Pending => tr!("queue.state.pending"),
            QueueState::Running => tr!("queue.state.running"),
            QueueState::Done => tr!("queue.state.done"),
            QueueState::Error => tr!("queue.state.error"),
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            QueueState::Pending => "pending",
            QueueState::Running => "running",
            QueueState::Done => "done",
            QueueState::Error => "error",
        }
    }

    /// Still occupying a slot: either waiting or in flight.
    pub const fn is_active(self) -> bool {
        matches!(self, QueueState::Pending | QueueState::Running)
    }
}

/// What winget is asked to do for an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueAction {
    /// `winget install` — the 安装 / 重新安装 buttons.
    Install,
    /// `winget upgrade --id` — the 更新 view's button.
    Upgrade,
    /// `upgrade -y` — one backend-wide transaction for every pending update.
    /// Backends with `Capabilities::batch_upgrade` (apt, flatpak) route their
    /// 全部更新 button here: a single dependency resolution and a single
    /// polkit authorization instead of one per package.
    UpgradeAll,
}

impl QueueAction {
    /// The verb shown in the drawer's command line.
    pub fn label(self) -> String {
        match self {
            QueueAction::Install => tr!("queue.action.install"),
            QueueAction::Upgrade => tr!("queue.action.upgrade"),
            QueueAction::UpgradeAll => tr!("queue.action.upgrade_all"),
        }
    }
}

/// One row of the drawer.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueItem {
    /// Stable handle for removal; package ids are not unique in the queue
    /// across a remove-then-re-add of the same package.
    pub uid: u64,
    pub id: String,
    pub name: String,
    /// The source the package was found in. Package ids are only unique
    /// within a source, so the install must name it rather than the
    /// configured default.
    pub source: String,
    pub action: QueueAction,
    pub state: QueueState,
    /// 0–100. winget exposes no progress, so this is indicative and only the
    /// process exit is authoritative.
    pub progress: f32,
    /// The command line while running, or the outcome once finished.
    pub log: String,
    /// The full failure report — command line, exit code, complete output —
    /// behind the drawer's 详情 button. Only set on error.
    pub detail: Option<String>,
    /// The raw winget exit code, once the process has exited.
    pub exit: Option<u32>,
    /// Set when the process exited, so the drawer can show the elapsed time.
    started: Option<Instant>,
}

impl QueueItem {
    /// Build the entry a package would create.
    fn new(uid: u64, package: &Package, action: QueueAction) -> Self {
        Self {
            uid,
            id: package.id.clone(),
            name: package.name.clone(),
            source: package.source.clone(),
            action,
            state: QueueState::Pending,
            progress: 0.0,
            log: tr!("queue.waiting"),
            detail: None,
            exit: None,
            started: None,
        }
    }

    /// Elapsed seconds since the run started, for the completion log.
    pub fn elapsed_secs(&self) -> f32 {
        self.started.map_or(0.0, |t| t.elapsed().as_secs_f32())
    }
}

/// The result of asking to enqueue a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enqueued {
    /// Added, with the handle the drawer and the task map key on.
    Added(u64),
    /// The same package is already waiting or running.
    Duplicate(String),
    /// No such package in the catalog.
    Unknown,
}

/// The arguments a queue entry runs, derived from the current state.
///
/// This is the mapping the prototype's `buildArgs` performs; every setting on
/// the settings screen feeds it, and the backend maps them onto its own flags.
pub fn args_for(item: &QueueItem, state: &AppState, backend: &dyn PackageBackend) -> Vec<String> {
    match item.action {
        QueueAction::Install => {
            backend.install_args(&item.id, &state.install_options_for(&item.source))
        }
        QueueAction::Upgrade => backend.upgrade_args(&item.id),
        QueueAction::UpgradeAll => backend.upgrade_all_args(),
    }
}

/// The full command line a queue entry shows, prefixed with the CLI's name.
pub fn command_line(item: &QueueItem, state: &AppState, backend: &dyn PackageBackend) -> String {
    let args = args_for(item, state, backend);
    format!("{} {}", backend.cli_name(), args.join(" "))
}

/// Add a package to the queue.
///
/// A package already pending or running is rejected — the prototype refuses
/// the duplicate rather than running it twice.
pub fn enqueue(state: &mut AppState, id: &str, action: QueueAction) -> Enqueued {
    let Some(package) = state
        .packages
        .iter()
        .find(|p| p.key() == id.to_lowercase())
        .cloned()
    else {
        return Enqueued::Unknown;
    };

    if state
        .queue
        .iter()
        .any(|item| item.id.eq_ignore_ascii_case(&package.id) && item.state.is_active())
    {
        return Enqueued::Duplicate(package.name);
    }

    let uid = state.next_queue_uid();
    state.queue.push(QueueItem::new(uid, &package, action));
    Enqueued::Added(uid)
}

/// The id the batch-upgrade entry carries; no package owns it.
pub const UPGRADE_ALL_ID: &str = "*";

/// Enqueue the single batch-upgrade task. Unlike [`enqueue`] there is no
/// catalog lookup — the entry is synthetic — and a second click while one
/// is active is a duplicate.
pub fn enqueue_upgrade_all(state: &mut AppState) -> Enqueued {
    if state
        .queue
        .iter()
        .any(|item| item.action == QueueAction::UpgradeAll && item.state.is_active())
    {
        return Enqueued::Duplicate(tr!("queue.action.upgrade_all"));
    }
    let package = Package {
        id: UPGRADE_ALL_ID.to_owned(),
        name: tr!("queue.action.upgrade_all"),
        ..Package::default()
    };
    let uid = state.next_queue_uid();
    state
        .queue
        .push(QueueItem::new(uid, &package, QueueAction::UpgradeAll));
    Enqueued::Added(uid)
}

/// Indices of entries that may start now, given how many slots are free.
pub fn startable(state: &AppState) -> Vec<usize> {
    let running = state
        .queue
        .iter()
        .filter(|item| item.state == QueueState::Running)
        .count();
    let slots = state
        .settings
        .effective_concurrency()
        .saturating_sub(running);
    state
        .queue
        .iter()
        .enumerate()
        .filter(|(_, item)| item.state == QueueState::Pending)
        .map(|(ix, _)| ix)
        .take(slots)
        .collect()
}

/// Move entry `ix` into the running state and return the command line it shows.
pub fn begin(state: &mut AppState, ix: usize, backend: &dyn PackageBackend) -> Option<String> {
    if state.queue.get(ix)?.state != QueueState::Pending {
        return None;
    }
    let command = command_line(&state.queue[ix], state, backend);
    let item = &mut state.queue[ix];
    item.state = QueueState::Running;
    item.progress = 0.0;
    item.started = Some(Instant::now());
    item.log = command.clone();
    Some(command)
}

/// Advance the indicative progress bar.
///
/// winget reports nothing machine-readable, so the bar eases toward a
/// ceiling and only the exit code completes it. The step shrinks as it
/// approaches the ceiling, so the bar never stalls outright.
pub fn advance_progress(item: &mut QueueItem) {
    const CEILING: f32 = 90.0;
    if item.state != QueueState::Running {
        return;
    }
    let remaining = CEILING - item.progress;
    if remaining <= 0.0 {
        return;
    }
    item.progress += (remaining * 0.18).max(1.0);
    item.progress = item.progress.min(CEILING);
}

/// Record a finished run.
///
/// `Err` is a spawn failure; a non-zero exit code arrives as `Ok` with the
/// code in [`Output::status`], matching how [`PackageBackend::run`] reports it.
/// The backend interprets its own exit codes (benign, needs elevation).
pub fn complete(
    state: &mut AppState,
    uid: u64,
    result: Result<Output, BackendError>,
    backend: &dyn PackageBackend,
) {
    let Some(ix) = state.queue.iter().position(|item| item.uid == uid) else {
        return;
    };
    let action = state.queue[ix].action;
    let id = state.queue[ix].id.clone();
    let elapsed = state.queue[ix].elapsed_secs();
    // While running, `log` still holds the command line `begin` wrote — it
    // heads the failure report so the dialog shows exactly what was run.
    let command = state.queue[ix].log.clone();

    match result {
        Ok(output) if output.status == 0 || backend.exit_is_benign(output.status) => {
            let item = &mut state.queue[ix];
            item.state = QueueState::Done;
            item.progress = 100.0;
            item.exit = Some(output.status);
            item.log = if output.status == 0 {
                tr!(
                    "queue.success_elapsed",
                    action = action.label(),
                    elapsed = format!("{elapsed:.1}")
                )
            } else {
                // A benign code: the desired end state already held.
                tr!("queue.success_exists", action = action.label())
            };
        }
        Ok(output) => {
            let description = backend.describe_exit(output.status);
            let note = if backend.exit_needs_elevation(output.status) {
                tr!("queue.elevation_note")
            } else {
                String::new()
            };
            // Surface the run's own first message: the bare exit code alone
            // ("失败：1 未知错误") leaves backend-specific errors — e.g.
            // PackageKit's ErrorCode details — undiagnosable from the drawer.
            let detail = exec::first_message(&output);
            let detail = if detail.is_empty() {
                detail
            } else {
                format!(" — {detail}")
            };
            let full_output = output.combined();
            let report = format!(
                "$ {command}\n{}: {}\n{}{}",
                tr!("queue.detail_exit_code"),
                backend.format_exit_code(output.status),
                description,
                if full_output.is_empty() {
                    String::new()
                } else {
                    format!("\n{full_output}")
                }
            );
            let item = &mut state.queue[ix];
            item.state = QueueState::Error;
            item.progress = 100.0;
            item.exit = Some(output.status);
            item.log = format!(
                "{}{detail}",
                tr!(
                    "queue.failed_code",
                    action = action.label(),
                    code = backend.format_exit_code(output.status),
                    desc = description,
                    note = note
                )
            );
            item.detail = Some(report);
        }
        Err(error) => {
            let item = &mut state.queue[ix];
            item.state = QueueState::Error;
            item.progress = 100.0;
            item.log = tr!(
                "queue.failed",
                action = action.label(),
                error = error.localized()
            );
            item.detail = Some(format!("$ {command}\n{}", error.localized()));
        }
    }

    // The package's own state moves with the outcome.
    if state.queue[ix].state == QueueState::Done {
        match action {
            QueueAction::Install => state.mark_installed(&id),
            QueueAction::Upgrade => state.mark_update_applied(&id),
            QueueAction::UpgradeAll => state.mark_all_updates_applied(),
        }
    }
}

/// Drop every finished entry, as the drawer's 清除 button does.
///
/// Failures stay: they carry the error message the operator has to read.
pub fn clear_finished(state: &mut AppState) {
    state.queue.retain(|item| item.state != QueueState::Done);
}

/// Remove one entry. Returns it, so the caller can cancel a running task.
pub fn remove(state: &mut AppState, uid: u64) -> Option<QueueItem> {
    let ix = state.queue.iter().position(|item| item.uid == uid)?;
    Some(state.queue.remove(ix))
}

/// Entries waiting or running — the count the sidebar badge shows.
pub fn active_count(state: &AppState) -> usize {
    state
        .queue
        .iter()
        .filter(|item| item.state.is_active())
        .count()
}

/// Whether the package is already waiting or running, for the card button.
/// An active batch upgrade covers every package.
pub fn is_busy(state: &AppState, id: &str) -> bool {
    state.queue.iter().any(|item| {
        item.state.is_active()
            && (item.action == QueueAction::UpgradeAll || item.id.eq_ignore_ascii_case(id))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CategoryId;

    fn output(status: u32) -> Output {
        Output {
            status,
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    /// A winget handle for building command lines; nothing is ever spawned.
    fn backend() -> crate::backends::winget::Winget {
        crate::backends::winget::Winget::new("winget.exe")
    }

    fn state_with(ids: &[&str]) -> AppState {
        AppState {
            packages: ids
                .iter()
                .map(|id| Package {
                    id: (*id).to_owned(),
                    name: (*id).to_owned(),
                    category: CategoryId::Dev,
                    ..Package::default()
                })
                .collect(),
            ..AppState::default()
        }
    }

    #[test]
    fn enqueue_adds_a_pending_entry_and_reports_its_handle() {
        crate::ensure_zh_locale();
        let mut state = state_with(&["A"]);
        let Enqueued::Added(uid) = enqueue(&mut state, "A", QueueAction::Install) else {
            panic!("expected the package to be queued");
        };
        assert_eq!(state.queue.len(), 1);
        assert_eq!(state.queue[0].uid, uid);
        assert_eq!(state.queue[0].state, QueueState::Pending);
        assert_eq!(state.queue[0].log, "等待中…");
        assert_eq!(active_count(&state), 1);
    }

    #[test]
    fn a_duplicate_is_refused_while_pending_or_running() {
        let mut state = state_with(&["A"]);
        enqueue(&mut state, "A", QueueAction::Install);
        assert_eq!(
            enqueue(&mut state, "A", QueueAction::Install),
            Enqueued::Duplicate("A".to_owned())
        );
        assert_eq!(state.queue.len(), 1, "the duplicate was not added");

        // Ids are case-insensitive, as winget's are.
        assert!(matches!(
            enqueue(&mut state, "a", QueueAction::Install),
            Enqueued::Duplicate(_)
        ));
    }

    #[test]
    fn a_finished_package_may_be_enqueued_again() {
        let mut state = state_with(&["A"]);
        let Enqueued::Added(uid) = enqueue(&mut state, "A", QueueAction::Install) else {
            panic!("first enqueue");
        };
        begin(&mut state, 0, &backend());
        complete(&mut state, uid, Ok(output(0)), &backend());
        assert_eq!(state.queue[0].state, QueueState::Done);

        assert!(matches!(
            enqueue(&mut state, "A", QueueAction::Install),
            Enqueued::Added(_)
        ));
        assert_eq!(state.queue.len(), 2);
    }

    #[test]
    fn an_unknown_package_is_not_queued() {
        let mut state = state_with(&["A"]);
        assert_eq!(
            enqueue(&mut state, "Nope", QueueAction::Install),
            Enqueued::Unknown
        );
        assert!(state.queue.is_empty());
    }

    #[test]
    fn concurrency_caps_how_many_start_at_once() {
        let mut state = state_with(&["A", "B", "C"]);
        for id in ["A", "B", "C"] {
            enqueue(&mut state, id, QueueAction::Install);
        }

        state.settings.concurrency = 1;
        assert_eq!(startable(&state).len(), 1);

        state.settings.concurrency = 2;
        assert_eq!(startable(&state).len(), 2);

        // Two running leaves no slot for a third at concurrency 2.
        begin(&mut state, 0, &backend());
        begin(&mut state, 1, &backend());
        assert!(startable(&state).is_empty());
    }

    #[test]
    fn begin_writes_the_command_line_from_the_settings() {
        let mut state = state_with(&["A"]);
        enqueue(&mut state, "A", QueueAction::Install);

        let command = begin(&mut state, 0, &backend()).expect("the entry starts");
        assert!(command.starts_with("winget install --id A"));
        assert!(
            !command.contains("--architecture") && !command.contains("--scope"),
            "installs must not force an architecture or scope"
        );
        assert_eq!(state.queue[0].state, QueueState::Running);
        assert_eq!(state.queue[0].log, command);
    }

    #[test]
    fn begin_ignores_an_entry_that_is_not_pending() {
        let mut state = state_with(&["A"]);
        enqueue(&mut state, "A", QueueAction::Install);
        assert!(begin(&mut state, 0, &backend()).is_some());
        assert!(
            begin(&mut state, 0, &backend()).is_none(),
            "already running"
        );
    }

    #[test]
    fn the_settings_change_the_command_the_queue_runs() {
        let mut state = state_with(&["A"]);
        enqueue(&mut state, "A", QueueAction::Install);
        let silent = command_line(&state.queue[0], &state, &backend());

        state.settings.silent = false;
        state.settings.accept_agreements = false;
        let verbose = command_line(&state.queue[0], &state, &backend());

        assert!(silent.contains("--silent"));
        assert!(!verbose.contains("--silent"));
        assert!(!verbose.contains("--accept-package-agreements"));
    }

    #[test]
    fn an_install_runs_against_the_packages_own_source() {
        // Regression: the install used to name the configured default source
        // unconditionally, so a `winget` hit installed with `--source msstore`
        // and failed the id lookup.
        let mut state = AppState {
            packages: vec![Package {
                id: "A".into(),
                name: "A".into(),
                source: "winget".into(),
                category: CategoryId::Dev,
                ..Package::default()
            }],
            ..AppState::default()
        };
        state.sources = vec![
            crate::model::Source {
                name: "winget".into(),
                ..crate::model::Source::default()
            },
            crate::model::Source {
                name: "msstore".into(),
                ..crate::model::Source::default()
            },
        ];
        enqueue(&mut state, "A", QueueAction::Install);

        let command = command_line(&state.queue[0], &state, &backend());
        assert!(
            command.contains("--source winget"),
            "the package's own source is named: {command}"
        );
        assert!(!command.contains("--source msstore"));
    }

    #[test]
    fn an_install_without_a_recorded_source_names_no_source() {
        let mut state = state_with(&["A"]);
        enqueue(&mut state, "A", QueueAction::Install);

        let command = command_line(&state.queue[0], &state, &backend());
        assert!(
            !command.contains("--source"),
            "no recorded source means no --source flag: {command}"
        );
    }

    #[test]
    fn an_upgrade_entry_runs_upgrade_not_install() {
        let mut state = state_with(&["A"]);
        enqueue(&mut state, "A", QueueAction::Upgrade);
        let command = command_line(&state.queue[0], &state, &backend());
        assert!(command.starts_with("winget upgrade --id A"));
    }

    #[test]
    fn the_batch_upgrade_enqueues_once_and_runs_upgrade_all_args() {
        let mut state = AppState {
            packages: ["A", "B"]
                .iter()
                .map(|id| Package {
                    id: (*id).to_owned(),
                    name: (*id).to_owned(),
                    version: "1.0".into(),
                    available: Some("2.0".into()),
                    installed: true,
                    category: CategoryId::Dev,
                    ..Package::default()
                })
                .collect(),
            ..AppState::default()
        };
        let Enqueued::Added(uid) = enqueue_upgrade_all(&mut state) else {
            panic!("expected the batch task to be queued");
        };
        assert_eq!(state.queue.len(), 1);
        assert_eq!(state.queue[0].action, QueueAction::UpgradeAll);
        assert_eq!(state.queue[0].id, UPGRADE_ALL_ID);

        // A second click while the first is active is a duplicate.
        assert!(matches!(
            enqueue_upgrade_all(&mut state),
            Enqueued::Duplicate(_)
        ));
        assert_eq!(state.queue.len(), 1, "no second batch task");

        // The entry runs the backend-wide command, not a per-package one.
        let command = begin(&mut state, 0, &backend()).expect("the entry starts");
        assert_eq!(command, "winget upgrade --all");

        // While it runs, every package reads busy.
        assert!(is_busy(&state, "A"));
        assert!(is_busy(&state, "B"));

        // A finished batch frees the next one.
        complete(&mut state, uid, Ok(output(0)), &backend());
        assert!(matches!(
            enqueue_upgrade_all(&mut state),
            Enqueued::Added(_)
        ));

        // Success applied every pending update at once.
        assert!(state.updates().is_empty());
        assert!(state.packages.iter().all(|pkg| pkg.version == "2.0"));
    }

    #[test]
    fn a_failed_run_shows_the_processes_own_first_message() {
        crate::ensure_zh_locale();
        let mut state = state_with(&["A"]);
        let Enqueued::Added(uid) = enqueue(&mut state, "A", QueueAction::Upgrade) else {
            panic!("enqueue");
        };
        begin(&mut state, 0, &backend());
        let failed = Output {
            status: 1,
            stdout: String::new(),
            stderr: "E: Package A has no available update".to_owned(),
        };
        complete(&mut state, uid, Ok(failed), &backend());
        assert_eq!(state.queue[0].state, QueueState::Error);
        assert!(
            state.queue[0]
                .log
                .contains("E: Package A has no available update"),
            "the stderr detail reached the drawer: {}",
            state.queue[0].log
        );
    }

    #[test]
    fn a_failed_run_keeps_the_full_report_behind_the_detail_button() {
        crate::ensure_zh_locale();
        let mut state = state_with(&["A"]);
        let Enqueued::Added(uid) = enqueue(&mut state, "A", QueueAction::Install) else {
            panic!("enqueue");
        };
        begin(&mut state, 0, &backend());
        let failed = Output {
            status: 1,
            stdout: "Resolving dependencies…".to_owned(),
            stderr: "E: Unable to locate package A".to_owned(),
        };
        complete(&mut state, uid, Ok(failed), &backend());

        let detail = state.queue[0].detail.as_ref().expect("a failure reports");
        assert!(
            detail.starts_with("$ winget install --id A"),
            "the report opens with the command line: {detail}"
        );
        assert!(detail.contains("0x00000001"), "the exit code: {detail}");
        assert!(
            detail.contains("Resolving dependencies…"),
            "stdout: {detail}"
        );
        assert!(
            detail.contains("E: Unable to locate package A"),
            "stderr: {detail}"
        );
    }

    #[test]
    fn a_successful_run_carries_no_error_detail() {
        crate::ensure_zh_locale();
        let mut state = state_with(&["A"]);
        let Enqueued::Added(uid) = enqueue(&mut state, "A", QueueAction::Install) else {
            panic!("enqueue");
        };
        begin(&mut state, 0, &backend());
        complete(&mut state, uid, Ok(output(0)), &backend());
        assert_eq!(state.queue[0].detail, None);
    }

    #[test]
    fn a_successful_install_marks_the_package_installed() {
        crate::ensure_zh_locale();
        let mut state = state_with(&["A"]);
        let Enqueued::Added(uid) = enqueue(&mut state, "A", QueueAction::Install) else {
            panic!("enqueue");
        };
        begin(&mut state, 0, &backend());
        complete(&mut state, uid, Ok(output(0)), &backend());

        assert_eq!(state.queue[0].state, QueueState::Done);
        assert_eq!(state.queue[0].progress, 100.0);
        assert!(state.packages[0].installed);
        assert!(state.queue[0].log.contains("成功"));
        assert_eq!(active_count(&state), 0);
    }

    #[test]
    fn a_benign_exit_code_reads_as_already_satisfied() {
        crate::ensure_zh_locale();
        let mut state = state_with(&["A"]);
        let Enqueued::Added(uid) = enqueue(&mut state, "A", QueueAction::Install) else {
            panic!("enqueue");
        };
        begin(&mut state, 0, &backend());
        // 0x8A150061 — the package is already installed.
        complete(&mut state, uid, Ok(output(0x8A15_0061)), &backend());

        assert_eq!(state.queue[0].state, QueueState::Done);
        assert!(state.queue[0].log.contains("已存在同版本"));
        assert!(state.packages[0].installed);
    }

    #[test]
    fn a_failure_keeps_the_code_and_its_description() {
        crate::ensure_zh_locale();
        let mut state = state_with(&["A"]);
        let Enqueued::Added(uid) = enqueue(&mut state, "A", QueueAction::Install) else {
            panic!("enqueue");
        };
        begin(&mut state, 0, &backend());
        complete(&mut state, uid, Ok(output(0x8A15_0019)), &backend());

        assert_eq!(state.queue[0].state, QueueState::Error);
        assert_eq!(state.queue[0].exit, Some(0x8A15_0019));
        assert!(state.queue[0].log.contains("0x8A150019"));
        assert!(state.queue[0].log.contains("需要管理员权限"));
        assert!(!state.packages[0].installed, "a failure installs nothing");
    }

    #[test]
    fn a_spawn_failure_is_a_queue_error_not_a_panic() {
        crate::ensure_zh_locale();
        let mut state = state_with(&["A"]);
        let Enqueued::Added(uid) = enqueue(&mut state, "A", QueueAction::Install) else {
            panic!("enqueue");
        };
        begin(&mut state, 0, &backend());
        complete(
            &mut state,
            uid,
            Err(BackendError::NotFound("winget")),
            &backend(),
        );
        assert_eq!(state.queue[0].state, QueueState::Error);
        assert!(state.queue[0].log.contains("未找到 winget"));
    }

    #[test]
    fn an_upgrade_advances_the_version_and_drops_the_row_from_updates() {
        let mut state = AppState {
            packages: vec![Package {
                id: "A".into(),
                name: "A".into(),
                version: "1.0".into(),
                available: Some("2.0".into()),
                installed: true,
                category: CategoryId::Dev,
                ..Package::default()
            }],
            ..AppState::default()
        };
        let Enqueued::Added(uid) = enqueue(&mut state, "A", QueueAction::Upgrade) else {
            panic!("enqueue");
        };
        begin(&mut state, 0, &backend());
        complete(&mut state, uid, Ok(output(0)), &backend());

        assert_eq!(state.packages[0].version, "2.0");
        assert!(state.updates().is_empty());
    }

    #[test]
    fn completing_an_unknown_handle_changes_nothing() {
        let mut state = state_with(&["A"]);
        enqueue(&mut state, "A", QueueAction::Install);
        complete(&mut state, 999, Ok(output(0)), &backend());
        assert_eq!(state.queue[0].state, QueueState::Pending);
    }

    #[test]
    fn progress_eases_toward_the_ceiling_without_passing_it() {
        let mut state = state_with(&["A"]);
        enqueue(&mut state, "A", QueueAction::Install);
        begin(&mut state, 0, &backend());

        let mut previous = state.queue[0].progress;
        for _ in 0..200 {
            advance_progress(&mut state.queue[0]);
            let now = state.queue[0].progress;
            assert!(now >= previous, "progress never goes backwards");
            assert!(now <= 90.0, "the bar never completes on its own");
            previous = now;
        }
        assert!(previous > 80.0, "the bar makes real progress");
    }

    #[test]
    fn a_pending_entry_does_not_advance() {
        let mut state = state_with(&["A"]);
        enqueue(&mut state, "A", QueueAction::Install);
        advance_progress(&mut state.queue[0]);
        assert_eq!(state.queue[0].progress, 0.0);
    }

    #[test]
    fn clear_finished_keeps_failures_for_reading() {
        let mut state = state_with(&["A", "B", "C"]);
        for id in ["A", "B", "C"] {
            enqueue(&mut state, id, QueueAction::Install);
        }
        let uid = |state: &AppState, id: &str| {
            state
                .queue
                .iter()
                .find(|item| item.id == id)
                .map(|item| item.uid)
                .expect("queued")
        };
        let (a, b, c) = (uid(&state, "A"), uid(&state, "B"), uid(&state, "C"));
        complete(&mut state, a, Ok(output(0)), &backend());
        complete(&mut state, b, Ok(output(0x8A15_0019)), &backend());
        complete(&mut state, c, Ok(output(0)), &backend());

        clear_finished(&mut state);
        assert_eq!(state.queue.len(), 1);
        assert_eq!(state.queue[0].id, "B", "the failure stays");
    }

    #[test]
    fn remove_takes_one_entry_out_and_never_reuses_its_handle() {
        let mut state = state_with(&["A", "B"]);
        let Enqueued::Added(first) = enqueue(&mut state, "A", QueueAction::Install) else {
            panic!("enqueue");
        };
        let Enqueued::Added(second) = enqueue(&mut state, "B", QueueAction::Install) else {
            panic!("enqueue");
        };
        assert_ne!(first, second);

        let removed = remove(&mut state, first).expect("the entry was there");
        assert_eq!(removed.id, "A");
        assert_eq!(state.queue.len(), 1);
        assert!(remove(&mut state, first).is_none(), "gone for good");

        let Enqueued::Added(third) = enqueue(&mut state, "A", QueueAction::Install) else {
            panic!("re-enqueue");
        };
        assert_ne!(third, first, "handles are not recycled");
    }

    #[test]
    fn is_busy_tracks_only_active_entries() {
        let mut state = state_with(&["A"]);
        assert!(!is_busy(&state, "A"));
        let Enqueued::Added(uid) = enqueue(&mut state, "A", QueueAction::Install) else {
            panic!("enqueue");
        };
        assert!(is_busy(&state, "a"), "case-insensitive");

        begin(&mut state, 0, &backend());
        assert!(is_busy(&state, "A"), "running counts as busy");

        complete(&mut state, uid, Ok(output(0)), &backend());
        assert!(!is_busy(&state, "A"), "a finished entry frees the button");
    }

    #[test]
    fn every_state_has_a_distinct_label_and_key() {
        let states = [
            QueueState::Pending,
            QueueState::Running,
            QueueState::Done,
            QueueState::Error,
        ];
        let mut labels = Vec::new();
        let mut keys = Vec::new();
        for state in states {
            assert!(!labels.contains(&state.label()));
            assert!(!keys.contains(&state.key()));
            labels.push(state.label());
            keys.push(state.key());
        }
    }

    /// The drawer shows `winget <args>`; the prefix appears once, not twice.
    #[test]
    fn command_line_prefixes_winget_exactly_once() {
        let mut state = state_with(&["A"]);
        enqueue(&mut state, "A", QueueAction::Install);
        let line = command_line(&state.queue[0], &state, &backend());
        assert!(line.starts_with("winget install --id A --exact"));
        // `--source winget` legitimately repeats the word, but never the
        // leading prefix.
        assert_eq!(
            line.split(' ').take(2).collect::<Vec<_>>(),
            ["winget", "install"]
        );
    }
}
