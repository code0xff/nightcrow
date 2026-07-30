//! Finding the rollout file a codex pane is writing, inside `CODEX_HOME`.
//!
//! Every filesystem error yields no candidate rather than a panic: a missing
//! `CODEX_HOME` is the normal state before codex has ever run in this account.

use super::rollout::session_id_from_filename;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Length of the year directory name, and of the month and day directory names.
const YEAR_DIR_LEN: usize = 4;
const MONTH_DAY_DIR_LEN: usize = 2;

/// How many day directories are searched for the pane's session.
///
/// The directories are named in *local* time and this crate has no date library,
/// so instead of computing today's name the `sessions/` tree is listed and the
/// lexicographically greatest day directories are taken — zero-padded
/// `YYYY/MM/DD` sorts chronologically. Two of them, because a session started
/// before local midnight keeps writing into yesterday's directory.
const CANDIDATE_DAY_DIRS: usize = 2;

/// Rollout files in the newest day directories that were modified at or after
/// `since`.
pub(super) fn candidate_rollouts(sessions_dir: &Path, since: i64) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for day in newest_day_dirs(sessions_dir) {
        let Ok(entries) = std::fs::read_dir(&day) else {
            continue;
        };
        for entry in entries.flatten() {
            let named = entry
                .file_name()
                .to_str()
                .and_then(session_id_from_filename)
                .is_some();
            let fresh = entry
                .metadata()
                .ok()
                .and_then(|meta| mtime_secs(&meta))
                .is_some_and(|t| t >= since);
            if named && fresh {
                out.push(entry.path());
            }
        }
    }
    out
}

fn newest_day_dirs(sessions_dir: &Path) -> Vec<PathBuf> {
    let mut days = Vec::new();
    for year in numeric_children(sessions_dir, YEAR_DIR_LEN) {
        for month in numeric_children(&year, MONTH_DAY_DIR_LEN) {
            days.extend(numeric_children(&month, MONTH_DAY_DIR_LEN));
        }
    }
    days.sort();
    let start = days.len().saturating_sub(CANDIDATE_DAY_DIRS);
    days.split_off(start)
}

/// Subdirectories whose name is exactly `len` ASCII digits, sorted by name.
fn numeric_children(dir: &Path, len: usize) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.len() == len && n.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

fn mtime_secs(meta: &Metadata) -> Option<i64> {
    let since_epoch = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(since_epoch.as_secs()).ok()
}
