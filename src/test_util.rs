//! Shared helpers used only by the in-crate `#[cfg(test)]` modules.
//!
//! Several modules drive a real `git` binary against a `TempDir` to verify
//! repository discovery, commit walking, etc. Centralizing the setup here
//! keeps the helpers in one place instead of duplicating them per test
//! module.

#![cfg(test)]

use git2::Repository;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

pub fn open_repo(path: &str) -> Repository {
    Repository::discover(path).expect("discover test repo")
}

pub fn run_git(repo_path: &str, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn make_repo() -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_string_lossy().to_string();
    run_git(&path, &["init"]);
    run_git(&path, &["config", "user.email", "t@t.com"]);
    run_git(&path, &["config", "user.name", "T"]);
    (dir, path)
}

/// A linked worktree, whose git directory therefore lives outside its own tree.
///
/// Returns both temporaries — the main repository's and the one holding the
/// worktree — and the worktree's path. Both must be kept alive for the duration
/// of the test: the worktree cannot be read without the repository it points at.
pub fn make_linked_worktree() -> (TempDir, TempDir, String) {
    let (main, main_path) = make_repo();
    // `git worktree add` needs a commit to base the new tree on.
    std::fs::write(Path::new(&main_path).join("seed.rs"), "fn main() {}").unwrap();
    run_git(&main_path, &["add", "seed.rs"]);
    run_git(&main_path, &["commit", "-m", "seed"]);
    let elsewhere = TempDir::new().unwrap();
    let tree = elsewhere.path().join("wt").to_string_lossy().to_string();
    run_git(&main_path, &["worktree", "add", &tree]);
    (main, elsewhere, tree)
}

/// A session serving `repos`, with no browser listener bound.
///
/// The state, not the server: everything the daemon serves hangs off this, so a
/// test can drive a real session — catalog, terminal hubs and all — without a
/// TCP port. Never persists, so tests cannot touch the real
/// `~/.nightcrow/workspace.json`.
pub fn session_state(repos: &[String]) -> std::sync::Arc<crate::web::viewer::server::ViewerState> {
    std::sync::Arc::new(crate::web::viewer::server::ViewerState::new(
        crate::web::viewer::server::ViewerOptions {
            bind: "127.0.0.1".parse().unwrap(),
            port: 0,
            auth: crate::web::common::auth::Auth::from_plaintext("swordfish").unwrap(),
            repos: repos.to_vec(),
            persist: false,
            startup_commands: Vec::new(),
            hot: crate::config::AgentIndicatorConfig::default(),
            prefs: crate::web::viewer::prefs::PrefsStore::at(std::path::PathBuf::from(
                "/nonexistent/nightcrow/viewer.json",
            )),
        },
    ))
}

/// In-memory `TerminalBackend` for tests: spawns nothing, just records the
/// command each `create_pane` was asked to run so pane-creation logic can be
/// asserted deterministically without a real PTY or shell.
#[derive(Default)]
pub struct FakeBackend {
    next_id: crate::backend::PaneId,
    pub launched: Vec<Option<String>>,
    /// Byte payloads passed to `send_input`, in call order. Lets input tests
    /// assert the exact bytes forwarded to the PTY (pass-through, literal
    /// leader) without a real terminal.
    pub sent: Vec<Vec<u8>>,
    /// Events handed out by the next `drain_events` call. Shared handle so a
    /// test can keep a clone and inject synthetic pane output/exit after the
    /// backend was boxed into `TerminalState`.
    pub pending_events: std::rc::Rc<std::cell::RefCell<Vec<crate::backend::BackendEvent>>>,
}

impl crate::backend::TerminalBackend for FakeBackend {
    fn create_pane(&mut self, rows: u16, cols: u16, command: Option<&str>) -> anyhow::Result<()> {
        self.next_id += 1;
        self.launched.push(command.map(str::to_string));
        // Queued like the real backend queues it, so a test that polls sees
        // the pane and one that does not sees the same "not yet" a remote
        // backend would give.
        self.pending_events
            .borrow_mut()
            .push(crate::backend::BackendEvent::Created {
                pane: self.next_id,
                rows,
                cols,
                requested: true,
                // A local backend gives no name; the caller's queued title or the
                // position decides.
                title: None,
            });
        Ok(())
    }

    /// Echoed as a shared session would: a close is a request, and the pane
    /// goes when the exit comes back.
    fn destroy_pane(&mut self, id: crate::backend::PaneId) {
        self.pending_events
            .borrow_mut()
            .push(crate::backend::BackendEvent::Exited { pane: id });
    }

    /// Echoed back as the event a shared session would send, so a test drives
    /// the same round trip the real thing does: a reorder is asked for and
    /// applied when it comes back.
    fn reorder(&mut self, order: &[crate::backend::PaneId]) {
        self.pending_events
            .borrow_mut()
            .push(crate::backend::BackendEvent::Reordered {
                order: order.to_vec(),
            });
    }

    fn send_input(&mut self, _id: crate::backend::PaneId, data: &[u8]) -> anyhow::Result<()> {
        self.sent.push(data.to_vec());
        Ok(())
    }

    fn resize(&mut self, _id: crate::backend::PaneId, _rows: u16, _cols: u16) {}

    fn drain_events(&mut self) -> Vec<crate::backend::BackendEvent> {
        std::mem::take(&mut *self.pending_events.borrow_mut())
    }

    fn test_sent_payloads(&self) -> Option<Vec<Vec<u8>>> {
        Some(self.sent.clone())
    }
}
