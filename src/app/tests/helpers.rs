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
    let mut git = GitViewManager::from_test_parts(".".to_string(), snapshot, tree_watch);
    git.agent_indicator.auto_follow = true;
    git.view.status.files = files
        .into_iter()
        .map(|path| ChangedFile::unstaged_only(path.to_string(), StatusKind::Modified))
        .collect();
    git.view.status.recompute_filter();
    App::from_test_parts(
        git,
        TerminalState::new(None, false),
        KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
    )
}

impl App {
    pub(crate) fn from_test_parts(
        git: GitViewManager,
        terminal: TerminalState,
        leader: KeyEvent,
    ) -> Self {
        Self {
            git,
            focus: Focus::FileList,
            notice: None,
            terminal,
            pending_terminal: Some(crate::workspace::persistence::SessionState {
                focus: Some(Focus::Terminal),
                ..Default::default()
            }),
            list_fullscreen: false,
            interaction: InteractionState::new(leader),
        }
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
