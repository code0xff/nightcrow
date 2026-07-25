use super::*;
use crate::git::diff::{ChangedFile, DiffHunk, DiffLine, LineKind, StatusKind};
use crate::runtime::snapshot::SnapshotMsg;
use crossterm::event::{KeyCode, KeyModifiers};
use std::sync::mpsc;

/// Build an inert SnapshotChannel for tests: real receiver, real stop
/// sender, but no worker thread driving the receiver.
///
/// Drops `_stop_rx` immediately on purpose: the only contract of `_stop_tx`
/// is "dropped → worker observes disconnect". Since there is no worker
/// here, nothing waits on either side, and dropping `_stop_rx` upfront
/// keeps the helper's tuple shape minimal. If a future test ever spawns
/// a real worker against this channel, it must keep `_stop_rx` alive.
pub(crate) fn dummy_snapshot_channel() -> (SnapshotChannel, mpsc::Sender<SnapshotMsg>) {
    let (tx, rx) = mpsc::channel::<SnapshotMsg>();
    let (stop_tx, _stop_rx) = mpsc::sync_channel::<()>(0);
    (SnapshotChannel::from_endpoints(rx, stop_tx), tx)
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
        repo_path: ".".to_string(),
        log_view: LogView::default(),
        tree_view: TreeView::default(),
        terminal: TerminalState::new(None, false),
        accent_idx: 0,
        tracking: None,
        snapshot,
        pending_snapshot: None,
        tree_watch,
        tree_dirty: Default::default(),
        tree_dirty_all: false,
        pending_selection: None,
        repo_cache: None,
        cfg_agent_indicator: crate::config::AgentIndicatorConfig {
            auto_follow: true,
            ..crate::config::AgentIndicatorConfig::default()
        },
        cfg_tree: crate::config::TreeConfig::default(),
        pagination: CommitLogPagination::with_config(
            crate::config::LogConfig::default().commit_log_page_size,
            crate::config::LogConfig::default().commit_log_prefetch_threshold,
        ),
        auto_follow: AutoFollow::default(),
        list_fullscreen: false,
        branch_name: None,
        leader: KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
        prefix_armed: false,
        awaiting_swap_target: false,
        pending_mouse_press: None,
        mouse_enabled: true,
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
