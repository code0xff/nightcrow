use super::*;
use crate::git::diff::{ChangedFile, DiffHunk, DiffLine, LineKind, StatusKind};
use crate::runtime::snapshot::SnapshotMsg;
use crossterm::event::{KeyCode, KeyModifiers};
use std::sync::mpsc;

/// Build an inert SnapshotChannel for tests: a real receiver, but no worker
/// thread and no filesystem watcher. The returned sender is how a test puts a
/// snapshot in front of the app.
pub(crate) fn dummy_snapshot_channel() -> (SnapshotChannel, mpsc::Sender<SnapshotMsg>) {
    let (tx, rx) = mpsc::channel::<SnapshotMsg>();
    (SnapshotChannel::from_endpoints(rx), tx)
}

/// Inert tree watcher plus its event sender. Tests that drive the
/// watcher-triggered refresh keep the `Sender` to inject synthetic events;
/// most tests drop it (a closed channel simply never reports a change).
pub(crate) fn dummy_tree_watcher() -> (
    crate::runtime::tree_watch::TreeWatcher,
    mpsc::Sender<notify_debouncer_mini::DebounceEventResult>,
) {
    let (tx, rx) = mpsc::channel();
    (
        crate::runtime::tree_watch::TreeWatcher::from_receiver(rx),
        tx,
    )
}

pub(crate) fn app_with_files(files: Vec<&str>) -> App {
    let (snapshot, _tx) = dummy_snapshot_channel();
    let (tree_watch, _tw_tx) = dummy_tree_watcher();
    let mut status_view = StatusView {
        files: files
            .into_iter()
            .map(|path| ChangedFile::unstaged_only(path.to_string(), StatusKind::Modified))
            .collect(),
        ..Default::default()
    };
    status_view.recompute_filter();
    App {
        mode: ViewMode::Status,
        status_view,
        diff: DiffPane::default(),
        focus: Focus::FileList,
        notice: None,
        repo_id: None,
        repo_path: ".".to_string(),
        log_view: LogView::default(),
        tree_view: TreeView::default(),
        terminal: TerminalState::new(None, false),
        tracking: None,
        snapshot,
        pending_snapshot: None,
        tree_watch,
        tree_dirty: Default::default(),
        tree_dirty_all: false,
        pending_selection: None,
        // The fresh-launch rule `App::new` starts with: focus the terminals
        // when they arrive.
        pending_terminal: Some(crate::workspace::persistence::SessionState {
            focus: Some(Focus::Terminal),
            ..Default::default()
        }),
        repo_cache: None,
        cfg_agent_indicator: crate::config::AgentIndicatorConfig {
            auto_follow: true,
            ..crate::config::AgentIndicatorConfig::default()
        },
        cfg_tree: crate::config::TreeConfig::default(),
        commit_log_controller: CommitLogController::with_config(
            crate::config::LogConfig::default().commit_log_page_size,
            crate::config::LogConfig::default().commit_log_prefetch_threshold,
        ),
        auto_follow: AutoFollow::default(),
        list_fullscreen: false,
        branch_name: None,
        log_decorations: Default::default(),
        last_refs_fingerprint: None,
        interaction: InteractionState::new(KeyEvent::new(
            KeyCode::Char('f'),
            KeyModifiers::CONTROL,
        )),
    }
}

pub(crate) fn context_hunk(lines: &[&str]) -> DiffHunk {
    DiffHunk {
        header: "@@ -1 +1 @@".to_string(),
        lines: lines
            .iter()
            .map(|content| DiffLine {
                kind: LineKind::Context,
                content: (*content).to_string(),
                old_lineno: None,
                new_lineno: None,
            })
            .collect(),
        file_path: None,
    }
}

pub(crate) fn app_with_fake_backend() -> App {
    let mut app = app_with_files(vec!["a.rs"]);
    let backend = Box::new(crate::test_util::FakeBackend::default());
    app.terminal = TerminalState::new(Some(backend), false);
    app
}

/// Helper: build a populated FileViewState so tests can assert that
/// downstream operations either preserve or invalidate it without
/// going through the disk-reading `load_file_view` path.
pub(crate) fn seeded_file_view(path: &str) -> FileViewState {
    FileViewState {
        key: Some(FileViewKey::Status(path.to_string())),
        content: "one\ntwo\nthree\n".to_string(),
        scroll: 1,
        scroll_x: 4,
        total_lines: 3,
        ..Default::default()
    }
}
