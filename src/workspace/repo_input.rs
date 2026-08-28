use super::Workspace;
use crate::app::NoticeKind;

/// Outcome of confirming the dialog. The caller owns the workspace and does
/// the opening; this only hands back an accepted path.
#[derive(Debug, PartialEq, Eq)]
pub enum RepoInputResult {
    /// Rejected — the dialog stays open with the text and a notice.
    Rejected,
    /// Open this resolved path as a project tab.
    Open(String),
}

/// Caps a bracketed paste. Mirrors `PROMPT_BUFFER_MAX_BYTES`.
pub(super) const REPO_INPUT_MAX_BYTES: usize = 4096;

impl Workspace {
    /// Open the dialog, prefilled with the active repo path — a sibling
    /// checkout is the common case. Empty when no project is open.
    pub fn start_repo_input(&mut self) {
        self.repo_input.buf = self
            .active()
            .map(|p| p.repository_path().to_string())
            .unwrap_or_default();
        self.repo_input.active = true;
        self.repo_input.candidates.clear();
        self.repo_input.picker = None;
        self.clear_notice(NoticeKind::RepoInput);
    }

    pub fn cancel_repo_input(&mut self) {
        self.repo_input.active = false;
        self.repo_input.buf.clear();
        self.repo_input.candidates.clear();
        self.repo_input.picker = None;
        self.clear_notice(NoticeKind::RepoInput);
    }

    pub fn confirm_repo_input(&mut self) -> RepoInputResult {
        // Validate the live buffer so a rejection leaves the text correctable.
        let trimmed = self.repo_input.buf.trim();
        if trimmed.is_empty() {
            self.raise_notice(NoticeKind::RepoInput, "repo path cannot be empty");
            return RepoInputResult::Rejected;
        }
        // No shell runs on this, so `~` has to be expanded here.
        let p = crate::platform::paths::expand_tilde(trimmed);
        if !p.is_dir() {
            // The path is already on screen, so name the problem only.
            self.raise_notice(
                NoticeKind::RepoInput,
                if p.exists() {
                    "not a directory"
                } else {
                    "no such directory"
                },
            );
            return RepoInputResult::Rejected;
        }
        let resolved = crate::git::resolve_repo_path(&p)
            .to_string_lossy()
            .to_string();
        self.repo_input.active = false;
        self.repo_input.buf.clear();
        self.repo_input.candidates.clear();
        self.repo_input.picker = None;
        self.clear_notice(NoticeKind::RepoInput);
        RepoInputResult::Open(resolved)
    }

    /// Extend the path from disk and offer what it could still become. Bound to
    /// Tab — the one field where a path is typed blind.
    pub fn repo_input_complete(&mut self) {
        let completed = super::path_complete::complete_dir_path(&self.repo_input.buf);
        // Dropped whole rather than truncated: a cut path points elsewhere.
        if completed.buf.len() > REPO_INPUT_MAX_BYTES {
            return;
        }
        self.clear_notice(NoticeKind::RepoInput);
        self.repo_input.buf = completed.buf;
        self.repo_input.candidates = completed.candidates;
    }

    /// Typing always extends the path, never replaces it — the prefill is
    /// there to supply a shared prefix, and wiping it on the first keystroke
    /// would throw that away with nothing to undo it. Esc and Backspace
    /// discard.
    pub fn repo_input_push(&mut self, ch: char) {
        if self.repo_input.buf.len() + ch.len_utf8() > REPO_INPUT_MAX_BYTES {
            return;
        }
        // Any edit invalidates the verdict on the old text.
        self.clear_notice(NoticeKind::RepoInput);
        self.repo_input.candidates.clear();
        self.repo_input.buf.push(ch);
    }

    pub fn repo_input_pop(&mut self) {
        self.clear_notice(NoticeKind::RepoInput);
        self.repo_input.candidates.clear();
        self.repo_input.buf.pop();
    }
}
