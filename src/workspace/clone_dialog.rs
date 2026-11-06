//! Clone mode state and the asynchronous job handoff for the repo dialog.

use super::Workspace;
use super::repo_input::REPO_INPUT_MAX_BYTES;
use crate::app::NoticeKind;
use crate::ui::status_view::{CloneUiStatus, RepoInputMode};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const CLONE_POLL_INTERVAL: Duration = Duration::from_millis(250);

pub(crate) enum CloneDialogAction {
    None,
    Start { parent: String, url: String },
    Open(String),
}

impl Workspace {
    pub(crate) fn toggle_repo_input_mode(&mut self) -> bool {
        self.repo_input.picker = None;
        self.repo_input.mode = match self.repo_input.mode {
            RepoInputMode::Open => {
                if self.repo_input.clone_parent.is_empty() {
                    self.repo_input.clone_parent = self.default_clone_parent();
                }
                if matches!(self.repo_input.clone_status, CloneUiStatus::Idle) {
                    RepoInputMode::CloneParent
                } else {
                    RepoInputMode::CloneUrl
                }
            }
            RepoInputMode::CloneParent => {
                self.repo_input.clone_started_visible = false;
                RepoInputMode::Open
            }
            RepoInputMode::CloneUrl => {
                self.repo_input.clone_started_visible = false;
                RepoInputMode::CloneParent
            }
        };
        self.repo_input.candidates.clear();
        self.clear_notice(NoticeKind::RepoInput);
        if self.repo_input.mode != RepoInputMode::Open
            && self.repo_input.clone_job.is_none()
            && self.repo_input.clone_available
            && matches!(self.repo_input.clone_status, CloneUiStatus::Idle)
        {
            self.repo_input.clone_status = CloneUiStatus::Discovering;
            self.repo_input.clone_poll_at = Some(Instant::now() + CLONE_POLL_INTERVAL);
            return true;
        }
        false
    }

    pub(crate) fn confirm_clone_parent(&mut self) -> bool {
        let raw = self.repo_input.clone_parent.trim();
        if raw.is_empty() {
            self.raise_notice(NoticeKind::RepoInput, "clone parent cannot be empty");
            return false;
        }
        let expanded = crate::platform::paths::expand_tilde(raw);
        let absolute = if expanded.is_absolute() {
            expanded
        } else {
            match std::env::current_dir() {
                Ok(current) => current.join(expanded),
                Err(_) => {
                    self.raise_notice(
                        NoticeKind::RepoInput,
                        "could not resolve the current directory",
                    );
                    return false;
                }
            }
        };
        if !absolute.is_dir() {
            self.raise_notice(
                NoticeKind::RepoInput,
                if absolute.exists() {
                    "not a directory"
                } else {
                    "no such directory"
                },
            );
            return false;
        }
        let Ok(canonical) = absolute.canonicalize() else {
            self.raise_notice(NoticeKind::RepoInput, "could not resolve that directory");
            return false;
        };
        self.repo_input.clone_parent = canonical.to_string_lossy().into_owned();
        self.repo_input.mode = RepoInputMode::CloneUrl;
        self.repo_input.candidates.clear();
        self.clear_notice(NoticeKind::RepoInput);
        true
    }

    pub(crate) fn submit_clone_url(&mut self) -> CloneDialogAction {
        if let CloneUiStatus::Done(path) = &self.repo_input.clone_status {
            let path = path.clone();
            self.repo_input.active = false;
            self.repo_input.clone_started_visible = false;
            self.clone_open_requested(&path);
            return CloneDialogAction::Open(path);
        }
        if matches!(
            self.repo_input.clone_status,
            CloneUiStatus::Discovering | CloneUiStatus::Starting | CloneUiStatus::Running
        ) {
            return CloneDialogAction::None;
        }
        if !self.repo_input.clone_available {
            self.repo_input.clone_status = CloneUiStatus::Failed(
                "git is not installed or not on the daemon's PATH".to_string(),
            );
            return CloneDialogAction::None;
        }
        let url = self.repo_input.clone_url.trim().to_string();
        if let Err(error) = crate::git::clone::validate_clone_url(&url) {
            self.repo_input.clone_status = CloneUiStatus::Failed(error.message().to_string());
            return CloneDialogAction::None;
        }
        let parent = self.repo_input.clone_parent.trim();
        let expanded = crate::platform::paths::expand_tilde(parent);
        let absolute = if expanded.is_absolute() {
            expanded
        } else {
            match std::env::current_dir() {
                Ok(current) => current.join(expanded),
                Err(_) => {
                    self.repo_input.clone_status = CloneUiStatus::Failed(
                        "could not resolve the current directory".to_string(),
                    );
                    return CloneDialogAction::None;
                }
            }
        };
        let Ok(absolute) = absolute.canonicalize() else {
            self.repo_input.clone_status = CloneUiStatus::Failed("no such directory".to_string());
            return CloneDialogAction::None;
        };
        self.repo_input.clone_parent = absolute.to_string_lossy().into_owned();
        self.repo_input.clone_status = CloneUiStatus::Starting;
        self.repo_input.clone_job = None;
        self.repo_input.clone_owned = true;
        self.repo_input.clone_started_visible = self.repo_input.active;
        self.repo_input.clone_poll_at = Some(Instant::now() + CLONE_POLL_INTERVAL);
        self.clear_clone_notice();
        self.clear_notice(NoticeKind::RepoInput);
        CloneDialogAction::Start {
            parent: self.repo_input.clone_parent.clone(),
            url,
        }
    }

    pub(crate) fn repo_input_value(&self) -> &str {
        match self.repo_input.mode {
            RepoInputMode::Open => &self.repo_input.buf,
            RepoInputMode::CloneParent => &self.repo_input.clone_parent,
            RepoInputMode::CloneUrl => &self.repo_input.clone_url,
        }
    }

    pub(crate) fn set_repo_input_value(&mut self, value: String) {
        match self.repo_input.mode {
            RepoInputMode::Open => self.repo_input.buf = value,
            RepoInputMode::CloneParent => self.repo_input.clone_parent = value,
            RepoInputMode::CloneUrl => self.repo_input.clone_url = value,
        }
    }

    pub(crate) fn clone_parent_push(&mut self, ch: char) {
        push_text(&mut self.repo_input.clone_parent, ch);
        self.repo_input.candidates.clear();
        self.clear_notice(NoticeKind::RepoInput);
    }

    pub(crate) fn clone_parent_pop(&mut self) {
        self.repo_input.clone_parent.pop();
        self.repo_input.candidates.clear();
        self.clear_notice(NoticeKind::RepoInput);
    }

    pub(crate) fn clone_url_push(&mut self, ch: char) {
        if matches!(
            self.repo_input.clone_status,
            CloneUiStatus::Running | CloneUiStatus::Starting | CloneUiStatus::Done(_)
        ) {
            return;
        }
        if !ch.is_control()
            && self.repo_input.clone_url.len() + ch.len_utf8()
                <= crate::git::clone::MAX_CLONE_URL_BYTES
        {
            self.repo_input.clone_url.push(ch);
        }
        if matches!(
            self.repo_input.clone_status,
            CloneUiStatus::Failed(_) | CloneUiStatus::Unconfirmed
        ) {
            self.repo_input.clone_status = CloneUiStatus::Idle;
        }
        self.clear_notice(NoticeKind::RepoInput);
    }

    pub(crate) fn clone_url_pop(&mut self) {
        if matches!(
            self.repo_input.clone_status,
            CloneUiStatus::Running | CloneUiStatus::Starting | CloneUiStatus::Done(_)
        ) {
            return;
        }
        self.repo_input.clone_url.pop();
        if matches!(
            self.repo_input.clone_status,
            CloneUiStatus::Failed(_) | CloneUiStatus::Unconfirmed
        ) {
            self.repo_input.clone_status = CloneUiStatus::Idle;
        }
        self.clear_notice(NoticeKind::RepoInput);
    }

    fn default_clone_parent(&self) -> String {
        self.active()
            .and_then(|app| {
                PathBuf::from(app.repository_path())
                    .parent()
                    .map(PathBuf::from)
            })
            .or_else(|| std::env::current_dir().ok())
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .to_string_lossy()
            .into_owned()
    }
}

fn push_text(buffer: &mut String, ch: char) {
    if !ch.is_control() && buffer.len() + ch.len_utf8() <= REPO_INPUT_MAX_BYTES {
        buffer.push(ch);
    }
}
