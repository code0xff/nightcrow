//! Polling and completion policy for the repo dialog's clone job.

use super::Workspace;
use crate::app::NoticeKind;
use crate::session::{CloneJobState, CloneLookup};
use crate::ui::status_view::{CloneUiStatus, RepoInputMode};
use std::time::{Duration, Instant};

const CLONE_POLL_INTERVAL: Duration = Duration::from_millis(250);

impl Workspace {
    pub(crate) fn take_clone_poll(&mut self, now: Instant) -> Option<Option<u64>> {
        if !self.repo_input.clone_available {
            self.repo_input.clone_poll_at = None;
            return None;
        }
        let due = self.repo_input.clone_poll_at?;
        if now < due {
            return None;
        }
        let active_discovery = self.repo_input.active
            && matches!(self.repo_input.clone_status, CloneUiStatus::Discovering);
        let in_flight = matches!(
            self.repo_input.clone_status,
            CloneUiStatus::Starting | CloneUiStatus::Running
        );
        if !active_discovery && !in_flight {
            self.repo_input.clone_poll_at = None;
            return None;
        }
        self.repo_input.clone_poll_at = Some(now + CLONE_POLL_INTERVAL);
        Some(self.repo_input.clone_job)
    }

    pub(crate) fn set_clone_available(&mut self, available: bool) {
        self.repo_input.clone_available = available;
    }

    pub(crate) fn clone_started(&mut self, job: u64) {
        self.repo_input.clone_job = Some(job);
        self.repo_input.clone_status = CloneUiStatus::Running;
        self.repo_input.clone_poll_at = Some(Instant::now() + CLONE_POLL_INTERVAL);
    }

    pub(crate) fn clone_rejected(&mut self, message: &str) {
        let message = clean_clone_text(message);
        self.repo_input.clone_job = None;
        self.repo_input.clone_owned = false;
        self.repo_input.clone_started_visible = false;
        self.repo_input.clone_poll_at = None;
        self.repo_input.clone_status = CloneUiStatus::Failed(message.clone());
        if !self.repo_input.active {
            self.raise_notice(NoticeKind::Project, format!("Clone failed: {message}"));
        }
    }

    pub(crate) fn apply_clone_lookup(&mut self, lookup: CloneLookup) -> Option<String> {
        match lookup {
            CloneLookup::Idle => {
                match &self.repo_input.clone_status {
                    CloneUiStatus::Discovering => {
                        self.repo_input.clone_status = CloneUiStatus::Idle;
                        self.repo_input.clone_poll_at = None;
                    }
                    CloneUiStatus::Starting => {
                        self.repo_input.clone_status = CloneUiStatus::Unconfirmed;
                        self.repo_input.clone_owned = false;
                        self.repo_input.clone_started_visible = false;
                        self.repo_input.clone_poll_at = None;
                        if !self.repo_input.active {
                            self.raise_notice(
                                NoticeKind::Project,
                                "Clone outcome could not be confirmed; check the destination before retrying",
                            );
                        }
                    }
                    _ => {}
                }
                None
            }
            CloneLookup::Unknown { job } => {
                if self.repo_input.clone_job == Some(job) {
                    self.clone_rejected("clone job is no longer available");
                }
                None
            }
            CloneLookup::Found { job, state } => {
                self.repo_input.clone_job = Some(job);
                match state {
                    CloneJobState::Running => {
                        self.repo_input.clone_status = CloneUiStatus::Running;
                        self.repo_input.clone_poll_at = Some(Instant::now() + CLONE_POLL_INTERVAL);
                        None
                    }
                    CloneJobState::Done { path } => {
                        let display_path = clean_clone_text(&path);
                        self.repo_input.clone_status = CloneUiStatus::Done(path.clone());
                        self.repo_input.clone_poll_at = None;
                        let auto_open = self.repo_input.clone_owned
                            && self.repo_input.clone_started_visible
                            && self.repo_input.active
                            && self.repo_input.mode == RepoInputMode::CloneUrl;
                        self.repo_input.clone_started_visible = false;
                        if auto_open {
                            self.repo_input.active = false;
                            self.clear_notice(NoticeKind::RepoInput);
                            self.clone_open_requested(&path);
                            Some(path)
                        } else {
                            self.raise_notice(
                                NoticeKind::Project,
                                format!("Clone finished: {display_path}. Open the repo dialog and press Shift+Tab, then Enter."),
                            );
                            None
                        }
                    }
                    CloneJobState::Failed { message } => {
                        self.clone_rejected(&message);
                        None
                    }
                }
            }
        }
    }

    pub(crate) fn clone_open_requested(&mut self, path: &str) -> bool {
        let path = clone_path_key(path);
        if !matches!(&self.repo_input.clone_status, CloneUiStatus::Done(done) if clone_path_key(done) == path)
        {
            return false;
        }
        self.repo_input.clone_open_pending = Some(path);
        true
    }

    pub(crate) fn clone_open_is_present(&mut self) -> bool {
        let Some(pending) = self.repo_input.clone_open_pending.as_deref() else {
            return false;
        };
        if self
            .projects
            .iter()
            .any(|project| project.repository_path() == pending)
        {
            self.consume_clone_result();
            true
        } else {
            false
        }
    }

    pub(crate) fn clone_open_confirmed(&mut self, paths: &[String]) -> bool {
        let Some(pending) = self.repo_input.clone_open_pending.as_deref() else {
            return false;
        };
        if paths.iter().any(|path| path == pending) {
            self.consume_clone_result();
            true
        } else {
            false
        }
    }

    pub(crate) fn clone_open_failed(&mut self) -> bool {
        self.repo_input.clone_open_pending.take().is_some()
    }

    pub(crate) fn clear_clone_notice(&mut self) {
        let is_clone_notice = self
            .active()
            .and_then(|app| app.notice.as_ref())
            .or_else(|| self.empty_notice())
            .is_some_and(|notice| {
                notice.kind == NoticeKind::Project
                    && (notice.text.starts_with("Clone finished:")
                        || notice.text.starts_with("Clone failed:"))
            });
        if is_clone_notice {
            self.clear_notice(NoticeKind::Project);
        }
    }

    fn consume_clone_result(&mut self) {
        self.repo_input.clone_status = CloneUiStatus::Idle;
        self.repo_input.clone_job = None;
        self.repo_input.clone_owned = false;
        self.repo_input.clone_started_visible = false;
        self.repo_input.clone_open_pending = None;
        self.repo_input.clone_poll_at = None;
        self.repo_input.clone_url.clear();
        self.repo_input.mode = RepoInputMode::Open;
        self.clear_clone_notice();
    }
}

fn clone_path_key(path: &str) -> String {
    let path = crate::platform::paths::expand_tilde(path);
    crate::git::resolve_repo_path(path)
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn clean_clone_text(text: &str) -> String {
    text.chars()
        .take(2048)
        .map(clean_display_char)
        .collect::<String>()
}

pub(crate) fn clean_clone_display(text: &str) -> String {
    text.chars().map(clean_display_char).collect::<String>()
}

fn clean_display_char(ch: char) -> char {
    if ch.is_control() { ' ' } else { ch }
}
