//! Filesystem watcher for the file-tree navigator (`ViewMode::Tree`).
//!
//! The tree caches one directory level at a time and only re-reads on Tree-mode
//! entry; this watcher closes the gap so a folder created/moved/renamed/deleted
//! while Tree mode is open shows up without leaving and re-entering. It watches
//! only the directories the user has actually expanded (plus the root) —
//! NON-recursively — mirroring yazi/broot/nvim-tree. A recursive watch over the
//! whole work tree would consume one inotify watch per directory and fall over
//! on large repositories (the reason broot keeps recursive watching off by
//! default), so the watch set is bounded to what is visible.
//!
//! Events are coalesced by `notify-debouncer-mini` over a short window and
//! surfaced as opaque "something changed" notifications: the navigator re-reads
//! the cache wholesale rather than diffing, so the event payload is irrelevant.
//! Refresh-on-entry remains the fallback when the watcher cannot start (e.g. a
//! platform/filesystem where native events never arrive), so this layer is
//! strictly additive — its absence degrades to the previous behaviour.

use notify::RecursiveMode;
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

/// Coalescing window for filesystem events. Long enough to batch the burst a
/// single `git`/editor/agent operation produces into one refresh, short enough
/// to feel live. Sits between nvim-tree (50 ms) and gitui (2 s); broot uses
/// 500 ms.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// Owns the debounced filesystem watcher and the set of currently watched
/// repo-relative directories. Dropping it stops the watcher thread.
///
/// The working-tree root is supplied to `sync` per call rather than stored, so
/// a repo switch needs only a fresh `TreeWatcher` (no stale root to carry) and
/// construction does not depend on a repository handle being open yet.
///
/// In tests (and when the watcher fails to start) `debouncer` is `None`: the
/// receiver still exists so `App` polling is uniform, and watch/unwatch calls
/// become no-ops.
pub struct TreeWatcher {
    /// `None` when the watcher could not be created or in test fixtures.
    debouncer: Option<Debouncer<notify::RecommendedWatcher>>,
    rx: Receiver<DebounceEventResult>,
    /// Repo-relative directories currently registered with the watcher, so
    /// `sync` can reconcile (add/remove) against a freshly desired set without
    /// re-registering unchanged paths.
    watched: BTreeSet<String>,
}

impl TreeWatcher {
    /// Start a watcher. A failure to construct the underlying OS watcher is
    /// non-fatal: it is logged and the watcher becomes inert (refresh-on-entry
    /// then carries the feature on its own).
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let debouncer = match new_debouncer(DEBOUNCE, tx) {
            Ok(d) => Some(d),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "failed to start file-tree watcher; falling back to refresh-on-entry"
                );
                None
            }
        };
        Self {
            debouncer,
            rx,
            watched: BTreeSet::new(),
        }
    }

    /// Build an inert watcher from a caller-held receiver. Tests keep the
    /// matching `Sender` to inject synthetic events; no OS watcher is created.
    #[cfg(test)]
    pub(crate) fn from_receiver(rx: Receiver<DebounceEventResult>) -> Self {
        Self {
            debouncer: None,
            rx,
            watched: BTreeSet::new(),
        }
    }

    /// Reconcile the watch set to exactly `desired` (repo-relative directories;
    /// the empty string is the root), resolved against `workdir`. Adds watches
    /// for newly visible directories and drops them for collapsed/removed ones,
    /// leaving unchanged paths untouched. A path that cannot be watched (e.g. it
    /// was deleted between the listing and this call) is skipped, not retried,
    /// and never enters `watched`.
    pub fn sync(&mut self, workdir: &Path, desired: &BTreeSet<String>) {
        let Some(debouncer) = self.debouncer.as_mut() else {
            // Inert watcher: track intent only so behaviour is observable in
            // tests, but perform no OS calls.
            self.watched = desired.clone();
            return;
        };
        let watcher = debouncer.watcher();
        // Drop paths no longer desired.
        let stale: Vec<String> = self.watched.difference(desired).cloned().collect();
        for rel in stale {
            let abs = join_rel(workdir, &rel);
            // Unwatch errors are benign (path already gone); drop it from the
            // set regardless so we never leak a phantom entry.
            let _ = watcher.unwatch(&abs);
            self.watched.remove(&rel);
        }
        // Add newly desired paths.
        let fresh: Vec<String> = desired.difference(&self.watched).cloned().collect();
        for rel in fresh {
            let abs = join_rel(workdir, &rel);
            match watcher.watch(&abs, RecursiveMode::NonRecursive) {
                Ok(()) => {
                    self.watched.insert(rel);
                }
                Err(e) => {
                    tracing::debug!(error = %e, path = %abs.display(), "tree watch failed");
                }
            }
        }
    }

    /// Drain pending events. Returns `true` if anything was observed since the
    /// last poll (any event or watcher error counts — the navigator re-reads
    /// wholesale rather than inspecting paths).
    pub fn drain_changed(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.rx.try_recv() {
                Ok(_) => changed = true,
                Err(TryRecvError::Empty) => break,
                // The sender is gone (watcher thread exited): nothing more will
                // ever arrive, so stop draining.
                Err(TryRecvError::Disconnected) => break,
            }
        }
        changed
    }
}

impl Default for TreeWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Join a repo-relative directory (`""` = root) onto the working-tree root.
fn join_rel(workdir: &Path, rel: &str) -> PathBuf {
    if rel.is_empty() {
        workdir.to_path_buf()
    } else {
        workdir.join(rel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inert_watcher_tracks_desired_set_without_os_calls() {
        let (_tx, rx) = mpsc::channel();
        let mut w = TreeWatcher::from_receiver(rx);
        let root = Path::new("/tmp/repo");
        let mut desired = BTreeSet::new();
        desired.insert(String::new());
        desired.insert("src".to_string());
        w.sync(root, &desired);
        assert_eq!(w.watched, desired);

        // Reconcile down to just the root.
        let mut smaller = BTreeSet::new();
        smaller.insert(String::new());
        w.sync(root, &smaller);
        assert_eq!(w.watched, smaller);
    }

    #[test]
    fn drain_changed_reports_and_clears() {
        let (tx, rx) = mpsc::channel();
        let mut w = TreeWatcher::from_receiver(rx);
        assert!(!w.drain_changed(), "no events yet");

        tx.send(Ok(Vec::new())).unwrap();
        tx.send(Ok(Vec::new())).unwrap();
        assert!(w.drain_changed(), "queued events are observed");
        // Drained: a second poll with nothing new reports no change.
        assert!(!w.drain_changed());
    }

    #[test]
    fn drain_changed_treats_watcher_error_as_change() {
        let (tx, rx) = mpsc::channel();
        let mut w = TreeWatcher::from_receiver(rx);
        tx.send(Err(notify::Error::generic("boom"))).unwrap();
        assert!(w.drain_changed());
    }

    #[test]
    fn join_rel_maps_root_and_subdir() {
        let root = Path::new("/tmp/repo");
        assert_eq!(join_rel(root, ""), PathBuf::from("/tmp/repo"));
        assert_eq!(join_rel(root, "src/ui"), PathBuf::from("/tmp/repo/src/ui"));
    }
}
