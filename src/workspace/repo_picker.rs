//! The repo dialog's directory browser, opened from the path field.
//!
//! The browser only ever fills the field; opening a repo stays the field's own
//! Enter, so there is one place that decides what gets opened no matter how the
//! path was arrived at.

use super::Workspace;
use super::path_tree::PathTree;
use super::repo_input::REPO_INPUT_MAX_BYTES;
use crate::app::NoticeKind;

impl Workspace {
    /// Open the browser on whatever directory the field currently names. Bound
    /// to `↓`: the field's horizontal keys already mean "edit this path", so the
    /// vertical axis is free for the list, which is also where every other
    /// autocomplete puts it.
    pub fn repo_input_browse(&mut self) {
        // Browsing is an edit intent: the browser writes a whole path into the
        // buffer, so an untouched prefill has to stop being replaceable or the
        // first key typed after returning would wipe what was just picked.
        self.repo_input.prefilled = false;
        // The candidate row and the browser answer the same question; leaving
        // the list up would describe a fragment the browser is replacing.
        self.repo_input.candidates.clear();
        match PathTree::open(&self.repo_input.buf) {
            Some(tree) => {
                self.clear_notice(NoticeKind::RepoInput);
                self.repo_input.picker = Some(tree);
            }
            // The path is on screen in the field itself, so name the problem
            // only — and stay in the field, where it can be corrected.
            None => self.raise_notice(NoticeKind::RepoInput, "cannot browse that directory"),
        }
    }

    /// Close the browser and return to the field with the text untouched.
    pub fn repo_input_close_browser(&mut self) {
        self.repo_input.picker = None;
    }

    /// Take the browser's selection into the field and return to it, so the path
    /// can still be extended with Tab or corrected by hand before opening.
    pub fn repo_input_pick(&mut self) {
        let Some(tree) = self.repo_input.picker.take() else {
            return;
        };
        let picked = tree.selected_path();
        if picked.len() > REPO_INPUT_MAX_BYTES {
            // Refuse whole rather than truncate: a cut path silently points
            // somewhere else. Says what it means, since nothing is on screen
            // to explain the field not changing.
            self.raise_notice(NoticeKind::RepoInput, "path too long");
            return;
        }
        self.clear_notice(NoticeKind::RepoInput);
        self.repo_input.buf = picked;
        self.repo_input.candidates.clear();
    }

    /// Move the browser's cursor. Inert with the browser closed, so the caller
    /// need not re-check which surface has the keys.
    pub fn repo_picker_move(&mut self, down: bool) {
        if let Some(tree) = self.repo_input.picker.as_mut() {
            tree.move_selection(down);
        }
    }

    pub fn repo_picker_expand(&mut self) {
        if let Some(tree) = self.repo_input.picker.as_mut() {
            tree.expand();
        }
    }

    pub fn repo_picker_collapse(&mut self) {
        if let Some(tree) = self.repo_input.picker.as_mut() {
            tree.collapse_or_up();
        }
    }
}
