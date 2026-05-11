use crate::backend::{PtyBackend, TerminalBackend};
use crate::git::diff::{ChangedFile, RepoSnapshot, TrackingStatus};
mod auto_follow;
mod diff_load;
mod focus;
mod navigation;
mod repo_input;
mod session_io;
mod snapshot_io;
mod terminal_ctrl;

pub use crate::runtime::snapshot::{SnapshotChannel, SnapshotMsg};
#[cfg(test)]
pub(crate) use crate::runtime::terminal::strip_escape_sequences;
pub use crate::runtime::terminal::{PaneInfo, TerminalState};
pub use crate::ui::diff_pane::{DiffPane, DiffPaneView};
pub use crate::ui::file_view::{FileViewKey, FileViewState};
pub use crate::ui::log_view::LogView;
pub use crate::ui::status_view::{RepoInput, StatusView};
#[cfg(test)]
pub(crate) use diff_load::DiffApply;
use std::time::Instant;

pub(crate) const SCROLLBACK_LINES: usize = 1000;
pub(crate) const LIST_PAGE_SIZE: usize = 10;
pub(crate) const DIFF_PAGE_SIZE: usize = 20;
pub(crate) const COMMIT_LOG_LIMIT: usize = 500;

/// Move a list index up by `n`, saturating at 0. Returns `true` when the index
/// actually changed so callers can decide whether to refresh associated state.
pub(crate) fn cursor_up(idx: &mut usize, n: usize) -> bool {
    let next = idx.saturating_sub(n);
    if next != *idx {
        *idx = next;
        true
    } else {
        false
    }
}

/// Move a list index down by `n`, clamped to `len - 1`. Returns `true` when the
/// index actually changed. A zero-length list is a no-op.
pub(crate) fn cursor_down(idx: &mut usize, len: usize, n: usize) -> bool {
    if len == 0 {
        return false;
    }
    let next = idx.saturating_add(n).min(len - 1);
    if next != *idx {
        *idx = next;
        true
    } else {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ViewMode {
    #[default]
    Status,
    Log,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Focus {
    FileList,
    DiffViewer,
    Terminal,
}

pub struct App {
    pub mode: ViewMode,
    pub status_view: StatusView,
    pub diff: DiffPane,
    pub focus: Focus,
    pub status: Option<String>,
    pub repo_path: String,
    pub log_view: LogView,
    pub terminal: TerminalState,
    pub repo_input: RepoInput,
    pub accent_idx: usize,
    pub tracking: Option<TrackingStatus>,
    pub(crate) snapshot: SnapshotChannel,
    pub(crate) pending_session: Option<crate::session::SessionState>,
    /// Cached `git2::Repository` for synchronous loads (file diff, commit
    /// diff, file blob, commit log). Opened lazily on first use; invalidated
    /// in `change_repo`. The snapshot worker thread keeps its own handle —
    /// `git2::Repository` is `!Send` and cannot be shared.
    pub(crate) repo_cache: Option<git2::Repository>,
    pub cfg_agent_indicator: crate::config::AgentIndicatorConfig,
    /// Wall-clock instant of the most recent user-driven selection change in
    /// the file list. `None` means "idle since boot". Used to gate
    /// auto-follow so an active user is never hijacked.
    pub last_manual_nav_at: Option<Instant>,
    /// Path the auto-follow last steered selection to. Prevents repeatedly
    /// re-asserting selection on the same already-hot-and-selected file.
    pub auto_followed_path: Option<String>,
}

impl App {
    pub fn new(repo_path: String, prompt_log: bool) -> Self {
        let snapshot = SnapshotChannel::spawn(&repo_path);

        let backend: Box<dyn TerminalBackend> = Box::new(PtyBackend::new(&repo_path));

        let mut app = App {
            mode: ViewMode::Status,
            status_view: StatusView::default(),
            diff: DiffPane::default(),
            focus: Focus::FileList,
            status: None,
            repo_path,
            log_view: LogView::default(),
            terminal: TerminalState::new(Some(backend), prompt_log),
            repo_input: RepoInput::default(),
            accent_idx: 0,
            tracking: None,
            snapshot,
            pending_session: None,
            repo_cache: None,
            cfg_agent_indicator: crate::config::AgentIndicatorConfig::default(),
            last_manual_nav_at: None,
            auto_followed_path: None,
        };

        app.ensure_initial_terminal();
        tracing::info!(repo = %app.repo_path, "nightcrow started");
        app
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::diff::{
        ChangeStatus, CommitEntry, DiffHunk, DiffLine, LineKind, load_commit_log,
    };
    use crate::test_util::{make_repo, open_repo, run_git};
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::{Duration, SystemTime};

    /// Build an inert SnapshotChannel for tests: real receiver, real stop
    /// sender, but no worker thread driving the receiver.
    ///
    /// Drops `_stop_rx` immediately on purpose: the only contract of `_stop_tx`
    /// is "dropped → worker observes disconnect". Since there is no worker
    /// here, nothing waits on either side, and dropping `_stop_rx` upfront
    /// keeps the helper's tuple shape minimal. If a future test ever spawns
    /// a real worker against this channel, it must keep `_stop_rx` alive.
    fn dummy_snapshot_channel() -> (SnapshotChannel, std::sync::mpsc::Sender<SnapshotMsg>) {
        let (tx, rx) = mpsc::channel::<SnapshotMsg>();
        let (stop_tx, _stop_rx) = mpsc::sync_channel::<()>(0);
        (
            SnapshotChannel {
                rx,
                _stop_tx: stop_tx,
            },
            tx,
        )
    }

    fn app_with_files(files: Vec<&str>) -> App {
        let (snapshot, _tx) = dummy_snapshot_channel();
        let mut status_view = StatusView {
            files: files
                .into_iter()
                .map(|path| ChangedFile::new(path.to_string(), ChangeStatus::Modified))
                .collect(),
            ..Default::default()
        };
        status_view.recompute_filter();
        App {
            mode: ViewMode::Status,
            status_view,
            diff: DiffPane::default(),
            focus: Focus::FileList,
            status: None,
            repo_path: ".".to_string(),
            log_view: LogView::default(),
            terminal: TerminalState::new(None, false),
            repo_input: RepoInput::default(),
            accent_idx: 0,
            tracking: None,
            snapshot,
            pending_session: None,
            repo_cache: None,
            cfg_agent_indicator: crate::config::AgentIndicatorConfig {
                auto_follow: true,
                ..crate::config::AgentIndicatorConfig::default()
            },
            last_manual_nav_at: None,
            auto_followed_path: None,
        }
    }

    fn context_hunk(lines: &[&str]) -> DiffHunk {
        DiffHunk {
            header: "@@ -1 +1 @@".to_string(),
            lines: lines
                .iter()
                .map(|content| DiffLine {
                    kind: LineKind::Context,
                    content: (*content).to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn selection_clamps_when_file_list_shrinks() {
        let mut app = app_with_files(vec!["a.rs", "b.rs", "c.rs"]);
        app.status_view.selected = 2;
        app.status_view.files = vec![ChangedFile::new("a.rs".to_string(), ChangeStatus::Modified)];

        let selected_path = app.restore_selection(Some("c.rs"));

        assert_eq!(selected_path.as_deref(), Some("a.rs"));
        assert_eq!(app.status_view.selected, 0);
    }

    #[test]
    fn selection_prefers_same_path_after_refresh() {
        let mut app = app_with_files(vec!["a.rs", "b.rs", "c.rs"]);
        app.status_view.selected = 1;
        app.status_view.files = vec![
            ChangedFile::new("a.rs".to_string(), ChangeStatus::Modified),
            ChangedFile::new("c.rs".to_string(), ChangeStatus::Modified),
            ChangedFile::new("b.rs".to_string(), ChangeStatus::Modified),
        ];

        let selected_path = app.restore_selection(Some("b.rs"));

        assert_eq!(selected_path.as_deref(), Some("b.rs"));
        assert_eq!(app.status_view.selected, 2);
    }

    #[test]
    fn diff_scroll_saturates_on_page_up() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.scroll = 3;

        app.page_up();

        assert_eq!(app.diff.scroll, 0);
    }

    #[test]
    fn diff_scroll_clamps_at_last_line_on_select_down() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        // 1 hunk = header + 1 content line = 2 total lines, max_scroll = 1
        app.diff.hunks = vec![context_hunk(&["x"])];
        app.diff.scroll = 1; // already at max

        app.select_down();

        assert_eq!(app.diff.scroll, 1, "scroll must not exceed last line index");
    }

    #[test]
    fn diff_scroll_clamps_at_last_line_on_page_down() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.hunks = vec![context_hunk(&["x"])];
        app.diff.scroll = 0;

        app.page_down(); // +20, but max is 1

        assert_eq!(app.diff.scroll, 1);
    }

    #[test]
    fn diff_scroll_handles_large_restored_offset() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.hunks = vec![context_hunk(&["x"])];
        app.diff.scroll = usize::MAX;

        app.select_down();

        assert_eq!(app.diff.scroll, 1);
    }

    #[test]
    fn diff_match_refresh_can_preserve_manual_scroll() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.diff.hunks = vec![context_hunk(&["needle"])];
        app.diff.search.query = "needle".to_string();
        app.diff.search.query_lower = "needle".to_string();
        app.diff.scroll = 7;

        app.diff.recompute_matches(false);

        assert_eq!(app.diff.search.matches, vec![1]);
        assert_eq!(app.diff.scroll, 7);
    }

    #[test]
    fn diff_search_input_scrolls_to_first_match() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.diff.hunks = vec![context_hunk(&["alpha", "needle"])];

        app.diff.search_push('n');

        assert_eq!(app.diff.search.matches, vec![2]);
        assert_eq!(app.diff.scroll, 2);
    }

    #[test]
    fn status_search_with_no_matches_clears_stale_diff() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.diff.hunks = vec![context_hunk(&["stale"])];

        app.search_push('z');

        assert!(app.filtered_indices().is_empty());
        assert!(app.diff.hunks.is_empty());
    }

    #[test]
    fn terminal_scrollback_uses_full_buffer() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.terminal.active = 0;
        app.terminal.size = (3, 10);

        let mut parser = vt100::Parser::new(3, 10, SCROLLBACK_LINES);
        parser.process(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n9\r\n");
        app.terminal.parsers.insert(1, parser);
        // Request scrolling well past screen height; vt100 supports
        // arbitrary offsets up to the buffered line count.
        app.terminal.scroll.insert(1, 6);

        app.terminal.sync_scroll();

        let actual = app.terminal.parsers.get(&1).unwrap().screen().scrollback();
        assert_eq!(actual, 6);
        assert_eq!(app.terminal.scroll.get(&1).copied(), Some(6));
    }

    #[test]
    fn terminal_scrollback_clamps_to_buffered_rows() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.terminal.active = 0;
        app.terminal.size = (3, 10);

        let mut parser = vt100::Parser::new(3, 10, SCROLLBACK_LINES);
        // Only a handful of buffered rows exist; an outsized request must
        // clamp to whatever vt100 actually has, never panic.
        parser.process(b"1\r\n2\r\n3\r\n4\r\n5\r\n");
        app.terminal.parsers.insert(1, parser);
        app.terminal.scroll.insert(1, 999);

        app.terminal.sync_scroll();

        let stored = app.terminal.scroll.get(&1).copied().unwrap_or(0);
        let actual = app.terminal.parsers.get(&1).unwrap().screen().scrollback();
        assert_eq!(stored, actual);
        assert!(actual < 999);
    }

    #[test]
    fn switch_pane_moves_focus_to_terminal() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![
            PaneInfo {
                id: 1,
                title: "shell 1".into(),
            },
            PaneInfo {
                id: 2,
                title: "shell 2".into(),
            },
        ];
        assert_eq!(app.focus, Focus::FileList);
        app.switch_pane(1);
        assert_eq!(app.focus, Focus::Terminal);
        assert_eq!(app.terminal.active, 1);
    }

    #[test]
    fn switch_pane_ignores_out_of_range() {
        let mut app = app_with_files(vec![]);
        app.switch_pane(5);
        assert_eq!(app.terminal.active, 0);
    }

    #[test]
    fn switch_pane_exits_diff_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.toggle_diff_fullscreen();
        assert!(app.diff.fullscreen);

        app.switch_pane(0);

        assert!(!app.diff.fullscreen);
        assert_eq!(app.focus, Focus::Terminal);
        assert_eq!(app.terminal.active, 0);
    }

    #[test]
    fn toggle_fullscreen_switches_focus_to_terminal() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        assert_eq!(app.focus, Focus::FileList);

        app.toggle_terminal_fullscreen();

        assert!(app.terminal.fullscreen);
        assert_eq!(app.focus, Focus::Terminal);
    }

    #[test]
    fn toggle_fullscreen_noop_with_no_panes() {
        let mut app = app_with_files(vec![]);
        assert!(app.terminal.panes.is_empty());

        app.toggle_terminal_fullscreen();

        assert!(!app.terminal.fullscreen);
    }

    #[test]
    fn toggle_diff_fullscreen_sets_flag_and_focuses_diff_viewer() {
        let mut app = app_with_files(vec![]);
        assert_eq!(app.focus, Focus::FileList);

        app.toggle_diff_fullscreen();

        assert!(app.diff.fullscreen);
        assert_eq!(app.focus, Focus::DiffViewer);

        app.toggle_diff_fullscreen();

        assert!(!app.diff.fullscreen);
        // Exiting zoom leaves focus on DiffViewer (no reason to bounce back).
        assert_eq!(app.focus, Focus::DiffViewer);
    }

    #[test]
    fn toggle_diff_fullscreen_exits_terminal_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.toggle_terminal_fullscreen();
        assert!(app.terminal.fullscreen);

        app.toggle_diff_fullscreen();

        assert!(app.diff.fullscreen);
        assert!(!app.terminal.fullscreen);
        assert_eq!(app.focus, Focus::DiffViewer);
    }

    #[test]
    fn toggle_terminal_fullscreen_exits_diff_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.toggle_diff_fullscreen();
        assert!(app.diff.fullscreen);

        app.toggle_terminal_fullscreen();

        assert!(app.terminal.fullscreen);
        assert!(!app.diff.fullscreen);
        assert_eq!(app.focus, Focus::Terminal);
    }

    #[test]
    fn cycle_focus_is_noop_in_diff_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.toggle_diff_fullscreen();
        assert_eq!(app.focus, Focus::DiffViewer);

        app.cycle_focus_forward();
        assert_eq!(app.focus, Focus::DiffViewer);

        app.cycle_focus_backward();
        assert_eq!(app.focus, Focus::DiffViewer);
    }

    #[test]
    fn close_last_pane_exits_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.terminal.fullscreen = true;
        app.focus = Focus::Terminal;
        app.terminal.scroll.insert(1, 3);
        app.terminal.prompt_bufs.insert(1, "cargo test".to_string());
        app.terminal.parsers.insert(1, vt100::Parser::new(3, 10, 0));

        app.close_active_pane();

        assert!(!app.terminal.fullscreen);
        assert_eq!(app.focus, Focus::DiffViewer);
        assert!(!app.terminal.scroll.contains_key(&1));
        assert!(!app.terminal.prompt_bufs.contains_key(&1));
        assert!(!app.terminal.parsers.contains_key(&1));
    }

    #[test]
    fn restore_session_restores_active_pane_even_when_focus_is_not_terminal() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![
            PaneInfo {
                id: 1,
                title: "shell 1".into(),
            },
            PaneInfo {
                id: 2,
                title: "shell 2".into(),
            },
        ];

        app.restore_session(&crate::session::SessionState {
            focus: Some(Focus::FileList),
            active_pane: 1,
            ..Default::default()
        });

        assert_eq!(app.focus, Focus::FileList);
        assert_eq!(app.terminal.active, 1);
    }

    #[test]
    fn restore_session_fullscreen_forces_terminal_focus() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];

        app.restore_session(&crate::session::SessionState {
            focus: Some(Focus::FileList),
            terminal_fullscreen: true,
            ..Default::default()
        });

        assert!(app.terminal.fullscreen);
        assert_eq!(app.focus, Focus::Terminal);
    }

    #[test]
    fn restore_session_diff_fullscreen_forces_diff_focus() {
        let mut app = app_with_files(vec![]);

        app.restore_session(&crate::session::SessionState {
            focus: Some(Focus::FileList),
            diff_fullscreen: true,
            ..Default::default()
        });

        assert!(app.diff.fullscreen);
        assert_eq!(app.focus, Focus::DiffViewer);
    }

    #[test]
    fn restore_session_prefers_terminal_fullscreen_over_diff_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];

        app.restore_session(&crate::session::SessionState {
            focus: Some(Focus::FileList),
            terminal_fullscreen: true,
            diff_fullscreen: true,
            ..Default::default()
        });

        assert!(app.terminal.fullscreen);
        assert!(!app.diff.fullscreen);
        assert_eq!(app.focus, Focus::Terminal);
    }

    #[test]
    fn save_session_round_trips_diff_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.toggle_diff_fullscreen();
        assert!(app.diff.fullscreen);

        let state = app.save_session();
        assert!(state.diff_fullscreen);

        let mut other = app_with_files(vec![]);
        other.restore_session(&state);
        assert!(other.diff.fullscreen);
        assert_eq!(other.focus, Focus::DiffViewer);
    }

    #[test]
    fn restore_session_normalizes_accent_index() {
        let mut app = app_with_files(vec![]);

        app.restore_session(&crate::session::SessionState {
            accent_idx: usize::MAX,
            ..Default::default()
        });

        assert_eq!(
            app.accent_idx,
            usize::MAX % crate::config::Accent::ALL.len()
        );
    }

    #[test]
    fn restore_session_keeps_log_scroll_after_loading_commit_diff() {
        let (_dir, path) = make_repo();
        let file_path = Path::new(&path).join("a.rs");
        std::fs::write(
            &file_path,
            "fn main() {\n    println!(\"one\");\n    println!(\"two\");\n}\n",
        )
        .unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);

        let mut app = app_with_files(vec![]);
        app.repo_path = path;

        app.restore_session(&crate::session::SessionState {
            mode: Some(ViewMode::Log),
            scroll: 2,
            ..Default::default()
        });

        assert_eq!(app.mode, ViewMode::Log);
        assert!(!app.diff.hunks.is_empty());
        assert_eq!(app.diff.scroll, 2);
    }

    #[test]
    fn log_drill_in_clears_stale_diff_for_empty_commit() {
        let (_dir, path) = make_repo();
        run_git(&path, &["commit", "--allow-empty", "-m", "empty"]);

        let mut app = app_with_files(vec![]);
        app.repo_path = path.clone();
        app.mode = ViewMode::Log;
        app.log_view.commits = load_commit_log(&open_repo(&path), 1).unwrap();
        app.diff.hunks = vec![context_hunk(&["stale"])];
        app.log_view.diff_title = "stale".to_string();

        app.log_drill_in();

        assert!(app.log_view.drill_down);
        assert!(app.log_view.commit_files.is_empty());
        assert!(app.diff.hunks.is_empty());
        assert!(app.log_view.diff_title.contains("empty"));
    }

    #[test]
    fn successful_snapshot_preserves_terminal_status() {
        let (snapshot, tx) = dummy_snapshot_channel();
        let mut app = App {
            status: Some("terminal error: backend unavailable".to_string()),
            snapshot,
            ..app_with_files(vec![])
        };

        tx.send(SnapshotMsg::Ok(
            RepoSnapshot {
                files: Vec::new(),
                tracking: None,
            },
            HashMap::new(),
        ))
        .unwrap();
        app.poll_snapshot();

        assert_eq!(
            app.status.as_deref(),
            Some("terminal error: backend unavailable")
        );
    }

    #[test]
    fn successful_snapshot_clears_git_status() {
        let (snapshot, tx) = dummy_snapshot_channel();
        let mut app = App {
            status: Some("git error: not a repo".to_string()),
            snapshot,
            ..app_with_files(vec![])
        };

        tx.send(SnapshotMsg::Ok(
            RepoSnapshot {
                files: Vec::new(),
                tracking: None,
            },
            HashMap::new(),
        ))
        .unwrap();
        app.poll_snapshot();

        assert_eq!(app.status, None);
    }

    #[test]
    fn snapshot_refresh_clamps_selection_to_active_filter() {
        let (snapshot, tx) = dummy_snapshot_channel();
        let mut app = App {
            snapshot,
            ..app_with_files(vec!["bar.rs"])
        };
        app.status_view.search_query = "bar".to_string();
        app.status_view.search_query_lower = "bar".to_string();
        app.status_view.recompute_filter();

        tx.send(SnapshotMsg::Ok(
            RepoSnapshot {
                files: vec![
                    ChangedFile::new("aaa.rs".to_string(), ChangeStatus::Modified),
                    ChangedFile::new("bar2.rs".to_string(), ChangeStatus::Modified),
                ],
                tracking: None,
            },
            HashMap::new(),
        ))
        .unwrap();
        app.poll_snapshot();

        assert_eq!(app.filtered_indices(), &[1]);
        assert_eq!(app.status_view.selected, 1);
        assert_eq!(
            app.status_view.files[app.status_view.selected].path,
            "bar2.rs"
        );
    }

    #[test]
    fn snapshot_refresh_with_no_filter_matches_clears_stale_diff() {
        let (snapshot, tx) = dummy_snapshot_channel();
        let mut app = App {
            snapshot,
            ..app_with_files(vec!["bar.rs"])
        };
        app.status_view.search_query = "bar".to_string();
        app.status_view.search_query_lower = "bar".to_string();
        app.status_view.recompute_filter();
        app.diff.hunks = vec![context_hunk(&["stale"])];

        tx.send(SnapshotMsg::Ok(
            RepoSnapshot {
                files: vec![ChangedFile::new(
                    "aaa.rs".to_string(),
                    ChangeStatus::Modified,
                )],
                tracking: None,
            },
            HashMap::new(),
        ))
        .unwrap();
        app.poll_snapshot();

        assert!(app.filtered_indices().is_empty());
        assert!(app.diff.hunks.is_empty());
    }

    #[test]
    fn move_selected_in_filter_resets_horizontal_scroll() {
        let mut app = app_with_files(vec!["a.rs", "b.rs"]);
        app.status_view.file_scroll_x = 12;
        app.move_selected_in_filter(1);
        assert_eq!(app.status_view.selected, 1);
        assert_eq!(app.status_view.file_scroll_x, 0);
    }

    #[test]
    fn log_select_down_resets_commit_scroll() {
        let mut app = app_with_files(vec![]);
        app.mode = ViewMode::Log;
        app.log_view.commits = vec![
            CommitEntry {
                oid: git2::Oid::zero(),
                short_id: "0000000".into(),
                summary: "first".into(),
                author: "T".into(),
                time: 0,
            },
            CommitEntry {
                oid: git2::Oid::zero(),
                short_id: "1111111".into(),
                summary: "second".into(),
                author: "T".into(),
                time: 0,
            },
        ];
        app.log_view.commit_scroll_x = 9;
        app.log_select_down();
        assert_eq!(app.log_view.selected, 1);
        assert_eq!(app.log_view.commit_scroll_x, 0);
    }

    #[test]
    fn log_file_select_down_resets_file_scroll() {
        let mut app = app_with_files(vec![]);
        app.mode = ViewMode::Log;
        app.log_view.drill_down = true;
        app.log_view.commits = vec![CommitEntry {
            oid: git2::Oid::zero(),
            short_id: "0000000".into(),
            summary: "first".into(),
            author: "T".into(),
            time: 0,
        }];
        app.log_view.commit_files = vec![
            ChangedFile::new("x.rs".into(), ChangeStatus::Modified),
            ChangedFile::new("y.rs".into(), ChangeStatus::Modified),
        ];
        app.log_view.file_scroll_x = 7;
        app.log_file_select_down();
        assert_eq!(app.log_view.file_selected, 1);
        assert_eq!(app.log_view.file_scroll_x, 0);
    }

    #[test]
    fn diff_scroll_routes_to_file_view_when_in_file_mode() {
        let mut app = app_with_files(vec![]);
        app.diff.scroll_x = 12;
        app.diff.file_view.scroll_x = 4;
        app.diff.view = DiffPaneView::File;

        app.diff.scroll_right();
        assert_eq!(app.diff.scroll_x, 12, "diff scroll_x must not change");
        assert_eq!(app.diff.file_view.scroll_x, 8);

        app.diff.scroll_left();
        assert_eq!(app.diff.file_view.scroll_x, 4);

        app.diff.view = DiffPaneView::Diff;
        app.diff.scroll_right();
        assert_eq!(app.diff.scroll_x, 16);
        assert_eq!(
            app.diff.file_view.scroll_x, 4,
            "file_view scroll_x must not change in diff mode"
        );
    }

    #[test]
    fn selected_filtered_status_file_returns_none_outside_filter() {
        let mut app = app_with_files(vec!["alpha.rs", "bravo.rs", "charlie.rs"]);
        app.status_view.search_query = "alpha".into();
        app.status_view.search_query_lower = "alpha".into();
        app.status_view.recompute_filter();
        // Filter only matches index 0; selecting index 2 must return None.
        app.status_view.selected = 2;
        assert!(app.selected_filtered_status_file().is_none());

        app.status_view.selected = 0;
        assert_eq!(
            app.selected_filtered_status_file().map(|f| f.path.as_str()),
            Some("alpha.rs")
        );
    }

    #[test]
    fn strip_escape_sequences_preserves_user_keystroke_after_bare_esc() {
        // ESC followed by an ordinary character was previously consumed; the
        // letter must now survive so user input echoed via PTY isn't lost.
        let out = super::strip_escape_sequences(b"\x1bA");
        assert_eq!(out, "A");
    }

    #[test]
    fn strip_escape_sequences_drops_csi_and_ss3() {
        // CSI (cursor key), SS3 (alternate keypad), and charset designation
        // must all be stripped fully without leaving final bytes behind.
        let out = super::strip_escape_sequences(b"hi\x1b[31mRED\x1b[0m\x1bOA\x1b(Bend");
        assert_eq!(out, "hiREDend");
    }

    #[test]
    fn strip_escape_sequences_keeps_text_after_malformed_ss3() {
        // ESC O followed by a control byte is not a valid SS3 sequence. The
        // old implementation unconditionally consumed two chars after ESC,
        // swallowing the newline (and any subsequent text relying on it).
        let out = super::strip_escape_sequences(b"\x1bO\nhello");
        assert_eq!(out, "\nhello");
    }

    #[test]
    fn strip_escape_sequences_drops_osc_until_terminator() {
        let bel = super::strip_escape_sequences(b"\x1b]0;title\x07ok");
        assert_eq!(bel, "ok");
        let st = super::strip_escape_sequences(b"\x1b]0;title\x1b\\ok");
        assert_eq!(st, "ok");
    }

    #[test]
    fn keep_scroll_clamps_when_new_diff_is_shorter() {
        let mut app = app_with_files(vec!["a.rs"]);
        // Seed a long diff and put scroll near the bottom.
        app.diff.hunks = vec![
            context_hunk(&["l1", "l2", "l3", "l4", "l5"]),
            context_hunk(&["l6", "l7", "l8"]),
        ];
        app.diff.scroll = app.diff.max_scroll();
        let prev_scroll = app.diff.scroll;
        assert!(prev_scroll > 1);

        // Apply a much shorter diff with KeepScroll; scroll must clamp.
        let shorter = vec![context_hunk(&["only"])];
        app.apply_diff_result(Ok(shorter), DiffApply::KeepScroll(prev_scroll));
        assert!(
            app.diff.scroll <= app.diff.max_scroll(),
            "scroll {} exceeded max {}",
            app.diff.scroll,
            app.diff.max_scroll()
        );
    }

    #[test]
    fn toggle_diff_file_view_ignores_selection_outside_filter() {
        let mut app = app_with_files(vec!["alpha.rs", "bravo.rs"]);
        app.status_view.search_query = "alpha".into();
        app.status_view.search_query_lower = "alpha".into();
        app.status_view.recompute_filter();
        // selected points outside the filter — toggle must refuse to open
        // a file view rather than loading the hidden entry.
        app.status_view.selected = 1;
        app.toggle_diff_file_view();
        assert_eq!(app.diff.view, DiffPaneView::Diff);
        assert!(app.diff.file_view.key.is_none());
    }

    /// Helper: build a populated FileViewState so tests can assert that
    /// downstream operations either preserve or invalidate it without
    /// going through the disk-reading `load_file_view` path.
    fn seeded_file_view(path: &str) -> FileViewState {
        FileViewState {
            key: Some(FileViewKey::Status(path.to_string())),
            content: "one\ntwo\nthree\n".to_string(),
            scroll: 1,
            scroll_x: 4,
            total_lines: 3,
            ..Default::default()
        }
    }

    #[test]
    fn keep_scroll_preserves_open_file_view() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.diff.hunks = vec![context_hunk(&["l1", "l2"])];
        app.diff.scroll = 1;
        app.diff.file_view = seeded_file_view("a.rs");
        app.diff.view = DiffPaneView::File;

        // Same file refresh through KeepScroll must leave the file view
        // alone — only Reset paths should invalidate it.
        let fresh = vec![context_hunk(&["l1", "l2", "l3"])];
        app.apply_diff_result(Ok(fresh), DiffApply::KeepScroll(app.diff.scroll));

        assert_eq!(app.diff.view, DiffPaneView::File);
        assert_eq!(
            app.diff.file_view.key,
            Some(FileViewKey::Status("a.rs".into()))
        );
        assert_eq!(app.diff.file_view.scroll, 1);
        assert_eq!(app.diff.file_view.scroll_x, 4);
    }

    #[test]
    fn clear_diff_state_invalidates_open_file_view() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.diff.hunks = vec![context_hunk(&["l1"])];
        app.diff.file_view = seeded_file_view("a.rs");
        app.diff.view = DiffPaneView::File;

        // toggle_mode and other reset paths route through clear_diff_state
        // — that single call must wipe the file view to its default.
        app.clear_diff_state();

        assert_eq!(app.diff.view, DiffPaneView::Diff);
        assert!(app.diff.file_view.key.is_none());
        assert!(app.diff.file_view.content.is_empty());
        assert_eq!(app.diff.file_view.scroll, 0);
        assert_eq!(app.diff.file_view.scroll_x, 0);
    }

    #[test]
    fn snapshot_refresh_with_no_filter_matches_clears_file_view() {
        let (snapshot, tx) = dummy_snapshot_channel();
        let mut app = App {
            snapshot,
            ..app_with_files(vec!["bar.rs"])
        };
        app.status_view.search_query = "bar".into();
        app.status_view.search_query_lower = "bar".into();
        app.status_view.recompute_filter();
        app.diff.hunks = vec![context_hunk(&["stale"])];
        app.diff.file_view = seeded_file_view("bar.rs");
        app.diff.view = DiffPaneView::File;

        tx.send(SnapshotMsg::Ok(
            RepoSnapshot {
                files: vec![ChangedFile::new(
                    "aaa.rs".to_string(),
                    ChangeStatus::Modified,
                )],
                tracking: None,
            },
            HashMap::new(),
        ))
        .unwrap();
        app.poll_snapshot();

        // No filter matches the new snapshot, so the diff and file view
        // both need to drop their stale handles on the gone path.
        assert!(app.filtered_indices().is_empty());
        assert!(app.diff.hunks.is_empty());
        assert_eq!(app.diff.view, DiffPaneView::Diff);
        assert!(app.diff.file_view.key.is_none());
    }

    fn snapshot_with(paths: &[&str]) -> RepoSnapshot {
        RepoSnapshot {
            files: paths
                .iter()
                .map(|p| ChangedFile::new((*p).to_string(), ChangeStatus::Modified))
                .collect(),
            tracking: None,
        }
    }

    #[test]
    fn ingest_snapshot_populates_hot_table_from_mtimes() {
        let mut app = app_with_files(vec![]);
        let snap = snapshot_with(&["a.rs", "b.rs"]);
        let now = SystemTime::now();
        let mtimes = HashMap::from([
            ("a.rs".to_string(), now),
            ("b.rs".to_string(), now - Duration::from_secs(5)),
        ]);

        app.ingest_snapshot(snap, mtimes);

        assert_eq!(app.status_view.hot_table.len(), 2);
        assert!(app.status_view.hot_table.contains_key("a.rs"));
        assert!(app.status_view.hot_table.contains_key("b.rs"));
    }

    #[test]
    fn merge_hot_table_drops_paths_missing_from_new_snapshot() {
        let mut app = app_with_files(vec![]);
        let now = SystemTime::now();

        app.ingest_snapshot(
            snapshot_with(&["a.rs"]),
            HashMap::from([("a.rs".to_string(), now)]),
        );
        assert!(app.status_view.hot_table.contains_key("a.rs"));

        app.ingest_snapshot(snapshot_with(&["b.rs"]), HashMap::new());
        assert!(!app.status_view.hot_table.contains_key("a.rs"));
        assert!(!app.status_view.hot_table.contains_key("b.rs"));
    }

    #[test]
    fn merge_hot_table_replaces_only_when_newer() {
        let mut app = app_with_files(vec![]);
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let newer = SystemTime::UNIX_EPOCH + Duration::from_secs(200);

        app.ingest_snapshot(
            snapshot_with(&["a.rs"]),
            HashMap::from([("a.rs".to_string(), newer)]),
        );
        app.ingest_snapshot(
            snapshot_with(&["a.rs"]),
            HashMap::from([("a.rs".to_string(), old)]),
        );

        // The earlier mtime must not overwrite the newer observation; a
        // rename-from-stash scenario can resurrect older mtimes for the
        // same path and would otherwise demote a fresh edit to cool.
        assert_eq!(app.status_view.hot_table.get("a.rs"), Some(&newer));
    }

    #[test]
    fn auto_follow_selects_freshest_hot_file_when_idle() {
        let mut app = app_with_files(vec!["a.rs", "b.rs"]);
        app.status_view.selected = 0;
        let now = SystemTime::now();

        app.ingest_snapshot(
            snapshot_with(&["a.rs", "b.rs"]),
            HashMap::from([
                ("a.rs".to_string(), now - Duration::from_secs(5)),
                ("b.rs".to_string(), now),
            ]),
        );

        // b.rs is fresher and the user is idle (last_manual_nav_at = None),
        // so selection must move from a.rs to b.rs.
        assert_eq!(app.status_view.selected, 1);
        assert_eq!(app.auto_followed_path.as_deref(), Some("b.rs"));
    }

    #[test]
    fn auto_follow_skipped_when_user_recently_navigated() {
        let mut app = app_with_files(vec!["a.rs", "b.rs"]);
        app.status_view.selected = 0;
        app.last_manual_nav_at = Some(Instant::now());
        let now = SystemTime::now();

        app.ingest_snapshot(
            snapshot_with(&["a.rs", "b.rs"]),
            HashMap::from([("b.rs".to_string(), now)]),
        );

        assert_eq!(app.status_view.selected, 0);
        assert!(app.auto_followed_path.is_none());
    }

    #[test]
    fn auto_follow_skipped_when_focus_not_filelist() {
        let mut app = app_with_files(vec!["a.rs", "b.rs"]);
        app.focus = Focus::DiffViewer;
        app.status_view.selected = 0;
        let now = SystemTime::now();

        app.ingest_snapshot(
            snapshot_with(&["a.rs", "b.rs"]),
            HashMap::from([("b.rs".to_string(), now)]),
        );

        assert_eq!(app.status_view.selected, 0);
        assert!(app.auto_followed_path.is_none());
    }

    #[test]
    fn auto_follow_skipped_when_disabled_in_config() {
        let mut app = app_with_files(vec!["a.rs", "b.rs"]);
        app.cfg_agent_indicator.auto_follow = false;
        app.status_view.selected = 0;
        let now = SystemTime::now();

        app.ingest_snapshot(
            snapshot_with(&["a.rs", "b.rs"]),
            HashMap::from([("b.rs".to_string(), now)]),
        );

        assert_eq!(app.status_view.selected, 0);
    }

    #[test]
    fn auto_follow_skipped_when_freshest_is_already_selected() {
        let mut app = app_with_files(vec!["a.rs", "b.rs"]);
        app.status_view.selected = 1;
        let now = SystemTime::now();

        app.ingest_snapshot(
            snapshot_with(&["a.rs", "b.rs"]),
            HashMap::from([("b.rs".to_string(), now)]),
        );

        // Selection already points to b.rs — no need to steer or arm the
        // "already followed here" guard.
        assert_eq!(app.status_view.selected, 1);
        assert!(app.auto_followed_path.is_none());
    }

    #[test]
    fn select_down_marks_user_active_when_focus_is_filelist() {
        let mut app = app_with_files(vec!["a.rs", "b.rs"]);
        app.focus = Focus::FileList;
        app.auto_followed_path = Some("a.rs".to_string());

        app.select_down();

        assert!(app.last_manual_nav_at.is_some());
        assert!(app.auto_followed_path.is_none());
    }

    #[test]
    fn select_down_does_not_mark_when_focus_is_diff() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;

        app.select_down();

        assert!(app.last_manual_nav_at.is_none());
    }

    #[test]
    fn auto_follow_respects_search_filter() {
        let mut app = app_with_files(vec!["alpha.rs", "beta.rs"]);
        app.status_view.search_query = "alpha".into();
        app.status_view.search_query_lower = "alpha".into();
        app.status_view.recompute_filter();
        app.status_view.selected = 0; // alpha.rs (the only filtered entry)
        let now = SystemTime::now();

        app.ingest_snapshot(
            snapshot_with(&["alpha.rs", "beta.rs"]),
            HashMap::from([
                ("alpha.rs".to_string(), now - Duration::from_secs(5)),
                ("beta.rs".to_string(), now),
            ]),
        );

        // beta.rs is fresher but filtered out, so auto-follow must not
        // jump to a row the user cannot even see.
        assert_eq!(app.status_view.selected, 0);
    }
}
