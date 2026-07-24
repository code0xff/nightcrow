mod helpers;

// `use super::*` re-exports app.rs's `use` declarations and public items
// (App, AutoFollow, Focus, ViewMode, Notice, NoticeKind, DiffPaneView,
// FileViewKey, FileViewState, CommitLogPagination, SnapshotChannel, etc.)
// so every test submodule can pull them in with `use super::*;`.
use super::*;
use crate::git::diff::{
    ChangedFile, CommitEntry, RepoSnapshot, StatusKind, load_commit_log,
};
use crate::runtime::snapshot::SnapshotMsg;
use crate::runtime::terminal::{PaneInfo, TerminalFullscreen, SCROLLBACK_LINES};
use crate::test_util::{make_repo, open_repo, run_git};
use crate::app::commit_log_fetch::{CommitLogFetchKind, CommitLogPageMsg};
use super::diff_load::DiffApply;
use super::strip_escape_sequences;
use crossterm::event::{KeyCode, KeyModifiers};
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

mod auto_follow;
mod clamp_pane;
mod commit_log;
mod diff_file_view;
mod fullscreen;
mod head_change;
mod leader_notice;
mod log_drill;
mod log_search;
mod mode_toggle;
mod pane;
mod scroll_misc;
mod session_restore;
mod snapshot;
mod snapshot_refresh;
mod status_diff;
mod strip_escape;
mod terminal_init;
mod terminal_scrollback;
mod tree;
mod tree_session;
mod tree_watcher;

pub(crate) use helpers::*;