use std::sync::mpsc;

use super::diff_load::DiffApply;
use super::load_controller::{DiffLoadMode, DiffTarget};
use super::{App, DiffPaneView, FileViewState, NoticeKind, ViewMode};
use crate::git::diff::{GitLoadPayload, GitLoadReply, LoadLane};

impl App {
    pub(crate) fn poll_git_loads(&mut self) {
        loop {
            match self.load_controller.worker.try_recv() {
                Ok(reply) => self.apply_git_load(reply),
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => return,
            }
        }
    }

    fn apply_git_load(&mut self, reply: GitLoadReply) {
        if reply.request.repo != self.repo_path {
            return;
        }
        match reply.request.operation.lane() {
            LoadLane::Diff => self.apply_diff_reply(reply),
            LoadLane::File => self.apply_file_reply(reply),
            LoadLane::CommitFiles => self.apply_commit_files_reply(reply),
            LoadLane::Decorations => self.apply_decorations_reply(reply),
        }
    }

    fn apply_diff_reply(&mut self, reply: GitLoadReply) {
        let Some(intent) = self.load_controller.diff.as_ref() else {
            return;
        };
        if intent.generation != reply.request.generation || intent.repo != reply.request.repo {
            return;
        }
        let intent = self.load_controller.diff.take().unwrap();
        if !self.diff_target_is_current(&intent.target) {
            return;
        }
        let result = match reply.result {
            Ok(GitLoadPayload::Diff(hunks)) => Ok(hunks),
            Ok(_) => return,
            Err(error) => {
                tracing::warn!(error = %error, "background diff load failed");
                self.raise_notice(NoticeKind::Diff, error.clone());
                Err(anyhow::anyhow!(error))
            }
        };
        let current_file_key_matches = self
            .current_file_view_key()
            .as_ref()
            .is_some_and(|key| self.diff.file_view.key.as_ref() == Some(key));
        let preserve_file = current_file_key_matches
            && (self
                .load_controller
                .file_generation()
                .is_some_and(|generation| generation > intent.generation)
                || self.diff.view == DiffPaneView::File);
        match intent.mode {
            DiffLoadMode::Reset if preserve_file => {
                self.apply_diff_result(result, DiffApply::ResetPreservingFile);
            }
            DiffLoadMode::Reset => self.apply_diff_result(result, DiffApply::Reset),
            DiffLoadMode::KeepScroll(scroll) => {
                self.apply_diff_result(result, DiffApply::KeepScroll(scroll));
            }
            DiffLoadMode::ResetWithTitle(title) if preserve_file => {
                self.apply_diff_result(result, DiffApply::ResetWithTitlePreservingFile(&title));
            }
            DiffLoadMode::ResetWithTitle(title) => {
                self.apply_diff_result(result, DiffApply::ResetWithTitle(&title));
            }
        }
        if let Some(scroll) = intent.restore_scroll {
            self.diff.scroll = scroll.min(self.diff.max_scroll());
        }
    }

    fn diff_target_is_current(&self, target: &DiffTarget) -> bool {
        match target {
            DiffTarget::Status(path) => {
                self.mode == super::ViewMode::Status
                    && self.selected_filtered_status_path().as_deref() == Some(path)
            }
            DiffTarget::Commit(oid) => {
                self.mode == super::ViewMode::Log
                    && !self.log_view.drill_down
                    && self
                        .log_view
                        .commits
                        .get(self.log_view.selected)
                        .is_some_and(|commit| commit.oid == *oid)
            }
            DiffTarget::CommitFile { oid, path } => {
                self.mode == super::ViewMode::Log
                    && self.log_view.drill_down
                    && self
                        .log_view
                        .commits
                        .get(self.log_view.selected)
                        .is_some_and(|commit| commit.oid == *oid)
                    && self
                        .log_view
                        .commit_files
                        .get(self.log_view.file_selected)
                        .is_some_and(|file| file.path == *path)
            }
        }
    }

    fn apply_file_reply(&mut self, reply: GitLoadReply) {
        let Some(intent) = self.load_controller.file.as_ref() else {
            return;
        };
        if intent.generation != reply.request.generation || intent.repo != reply.request.repo {
            return;
        }
        let intent = self.load_controller.file.take().unwrap();
        if self.current_file_view_key().as_ref() != Some(&intent.key) {
            return;
        }
        let mut file_view = FileViewState {
            key: Some(intent.key),
            anchor_line: intent.anchor,
            ..Default::default()
        };
        match reply.result {
            Ok(GitLoadPayload::File(content)) => {
                file_view.set_content(content);
                let initial = intent
                    .anchor
                    .map(|line| line.saturating_sub(1).saturating_sub(2))
                    .unwrap_or(0);
                file_view.scroll = initial.min(file_view.max_scroll());
            }
            Ok(_) => return,
            Err(error) => file_view.error = Some(error),
        }
        self.diff.file_view = file_view;
    }

    fn apply_commit_files_reply(&mut self, reply: GitLoadReply) {
        let Some(intent) = self.load_controller.commit_files.as_ref() else {
            return;
        };
        if intent.generation != reply.request.generation || intent.repo != reply.request.repo {
            return;
        }
        let intent = self.load_controller.commit_files.take().unwrap();
        if self.mode != ViewMode::Log
            || self.log_view.drill_down
            || self
                .log_view
                .commits
                .get(self.log_view.selected)
                .is_none_or(|commit| commit.oid != intent.oid)
        {
            return;
        }
        match reply.result {
            Ok(GitLoadPayload::CommitFiles(files)) => {
                self.log_view.set_commit_files(files);
                self.log_view.file_selected = 0;
                self.log_view.drill_down = true;
                if self.log_view.commit_files.is_empty() {
                    self.clear_diff_state();
                    self.log_view.diff_title = intent.title;
                } else {
                    self.load_file_diff_for_log_file_selected();
                }
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(error = %error, "failed to load commit files"),
        }
    }

    fn apply_decorations_reply(&mut self, reply: GitLoadReply) {
        let Some(intent) = self.load_controller.decorations.as_ref() else {
            return;
        };
        if intent.generation != reply.request.generation || intent.repo != reply.request.repo {
            return;
        }
        let intent = self.load_controller.decorations.take().unwrap();
        match reply.result {
            Ok(GitLoadPayload::Decorations(decorations)) => {
                self.log_decorations = decorations;
                self.last_refs_fingerprint = Some(intent.fingerprint);
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(error = %error, "failed to load ref decorations"),
        }
    }

    #[cfg(test)]
    pub(crate) fn flush_git_loads_for_test(&mut self, timeout: std::time::Duration) {
        let started = std::time::Instant::now();
        while self.load_controller.diff.is_some()
            || self.load_controller.file.is_some()
            || self.load_controller.commit_files.is_some()
            || self.load_controller.decorations.is_some()
        {
            assert!(
                started.elapsed() <= timeout,
                "git load did not finish in {timeout:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
            self.poll_git_loads();
        }
    }
}
