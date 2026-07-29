use super::Workspace;
use crate::app::NoticeKind;

/// The outcome of confirming the repo-path dialog. Opening is not carried out
/// here: it builds a whole new `App`, which needs config the project does not
/// carry, and it has to check whether another tab already holds that repo. So
/// the accepted path is handed back and the caller, which owns the workspace,
/// does the opening.
#[derive(Debug, PartialEq, Eq)]
pub enum RepoInputResult {
    /// Validation failed. The dialog stays open with the text intact and a
    /// notice naming the problem.
    Rejected,
    /// Open this resolved path as a project tab.
    Open(String),
}

// Mirrors `PROMPT_BUFFER_MAX_BYTES` so a bracketed paste cannot grow this
// buffer without bound; comfortably above any realistic filesystem path.
const REPO_INPUT_MAX_BYTES: usize = 4096;

impl Workspace {
    /// Open the dialog that adds a project tab. Prefilled with the active
    /// project's repo path: a sibling checkout is the common case, and the
    /// shared prefix is most of what the user would retype. With no project
    /// open there is nothing to prefill, so the dialog starts empty.
    pub fn start_repo_input(&mut self) {
        self.repo_input.buf = self
            .active()
            .map(|p| p.repo_path.clone())
            .unwrap_or_default();
        self.repo_input.active = true;
        self.repo_input.prefilled = true;
        self.repo_input.candidates.clear();
        self.clear_notice(NoticeKind::RepoInput);
    }

    pub fn cancel_repo_input(&mut self) {
        self.repo_input.active = false;
        self.repo_input.buf.clear();
        self.repo_input.prefilled = false;
        self.repo_input.candidates.clear();
        self.clear_notice(NoticeKind::RepoInput);
    }

    pub fn confirm_repo_input(&mut self) -> RepoInputResult {
        // Validate against the live buffer so a failed attempt leaves the
        // dialog open with the user's text intact for correction; only close
        // and consume the buffer once we're committed to switching repos.
        let trimmed = self.repo_input.buf.trim();
        if trimmed.is_empty() {
            self.raise_notice(NoticeKind::RepoInput, "repo path cannot be empty");
            return RepoInputResult::Rejected;
        }
        // The dialog is not a shell, so `~` has to be expanded here or a home
        // relative path would read as a directory literally named `~`.
        let p = crate::platform::paths::expand_tilde(trimmed);
        if !p.is_dir() {
            // The rejected path is already on screen in the input itself, so
            // the message names the problem only.
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
        self.repo_input.prefilled = false;
        self.repo_input.candidates.clear();
        self.clear_notice(NoticeKind::RepoInput);
        RepoInputResult::Open(resolved)
    }

    /// Extend the typed path from disk, and offer the directories it could
    /// still become. Bound to Tab, which is otherwise inert in a text field —
    /// and this is the one field where a path is typed with no way to see what
    /// is actually there.
    pub fn repo_input_complete(&mut self) {
        // Tab reads as "extend this path", so an untouched prefill survives
        // rather than being replaced — the same reading Backspace and → give it.
        self.repo_input.prefilled = false;
        let completed = super::path_complete::complete_dir_path(&self.repo_input.buf);
        // A completion that would breach the cap is dropped whole: applying a
        // truncated path would silently point somewhere else.
        if completed.buf.len() > REPO_INPUT_MAX_BYTES {
            return;
        }
        // Any edit invalidates the verdict on the old text, completion included.
        self.clear_notice(NoticeKind::RepoInput);
        self.repo_input.buf = completed.buf;
        self.repo_input.candidates = completed.candidates;
    }

    pub fn repo_input_push(&mut self, ch: char) {
        // Typing over an untouched prefill replaces it: the dialog opens on
        // the current repo path, and a user heading somewhere unrelated would
        // otherwise have to backspace all of it first. A paste lands here one
        // char at a time, so only its first char clears.
        if self.repo_input.prefilled {
            self.repo_input.buf.clear();
            self.repo_input.prefilled = false;
        }
        if self.repo_input.buf.len() + ch.len_utf8() > REPO_INPUT_MAX_BYTES {
            return;
        }
        // Any edit invalidates the verdict on the old text.
        self.clear_notice(NoticeKind::RepoInput);
        self.repo_input.candidates.clear();
        self.repo_input.buf.push(ch);
    }

    /// Leave prefill mode without changing the text, so the next keystroke
    /// appends. Bound to →/End: the sub-directory case (`<prefix> o`, then
    /// type `src` onto the trailing slash) needs a gesture that says "edit
    /// this" without Backspace eating the separator first.
    pub fn repo_input_accept_prefill(&mut self) {
        self.repo_input.prefilled = false;
        self.repo_input.candidates.clear();
    }

    pub fn repo_input_pop(&mut self) {
        // Backspace means "edit this path", not "replace it" — keep the text
        // and just leave prefill mode.
        self.repo_input.prefilled = false;
        self.clear_notice(NoticeKind::RepoInput);
        self.repo_input.candidates.clear();
        self.repo_input.buf.pop();
    }
}
