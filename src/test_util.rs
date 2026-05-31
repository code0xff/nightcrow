//! Shared helpers used only by the in-crate `#[cfg(test)]` modules.
//!
//! Several modules drive a real `git` binary against a `TempDir` to verify
//! repository discovery, commit walking, etc. Centralizing the setup here
//! keeps the helpers in one place instead of duplicating them per test
//! module.

#![cfg(test)]

use git2::Repository;
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
}

impl crate::backend::TerminalBackend for FakeBackend {
    fn create_pane(
        &mut self,
        _rows: u16,
        _cols: u16,
        command: Option<&str>,
    ) -> anyhow::Result<crate::backend::PaneId> {
        self.next_id += 1;
        self.launched.push(command.map(str::to_string));
        Ok(self.next_id)
    }

    fn destroy_pane(&mut self, _id: crate::backend::PaneId) {}

    fn send_input(&mut self, _id: crate::backend::PaneId, data: &[u8]) -> anyhow::Result<()> {
        self.sent.push(data.to_vec());
        Ok(())
    }

    fn resize(&mut self, _id: crate::backend::PaneId, _rows: u16, _cols: u16) {}

    fn drain_events(&mut self) -> Vec<crate::backend::BackendEvent> {
        Vec::new()
    }

    fn set_cwd(&mut self, _path: &std::path::Path) {}

    fn test_sent_payloads(&self) -> Option<Vec<Vec<u8>>> {
        Some(self.sent.clone())
    }
}
