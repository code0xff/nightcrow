//! The repo dialog's directory browser. It only ever fills the field — opening
//! a repo stays the field's own Enter.

use super::Workspace;
use super::path_tree::PathTree;
use super::repo_input::REPO_INPUT_MAX_BYTES;
use crate::app::NoticeKind;

impl Workspace {
    /// Open the browser on whatever directory the field names. Bound to `↓`,
    /// where every autocomplete puts its list.
    pub fn repo_input_browse(&mut self) {
        // The candidate row answers the same question the browser does.
        self.repo_input.candidates.clear();
        match PathTree::open(&self.repo_input.buf) {
            Some(tree) => {
                self.clear_notice(NoticeKind::RepoInput);
                self.repo_input.picker = Some(tree);
            }
            // The path is on screen already, so name the problem only.
            None => self.raise_notice(NoticeKind::RepoInput, "cannot browse that directory"),
        }
    }

    /// Close the browser and return to the field with the text untouched.
    pub fn repo_input_close_browser(&mut self) {
        self.repo_input.picker = None;
    }

    /// Take the selection into the field. Enter means the same thing on every
    /// row — going anywhere the tree does not show is `←`'s job, not a row's.
    pub fn repo_input_pick(&mut self) {
        let Some(tree) = self.repo_input.picker.take() else {
            return;
        };
        let picked = tree.selected_path();
        if picked.len() > REPO_INPUT_MAX_BYTES {
            // Refuse whole rather than truncate: a cut path points elsewhere.
            self.raise_notice(NoticeKind::RepoInput, "path too long");
            return;
        }
        self.clear_notice(NoticeKind::RepoInput);
        self.repo_input.buf = picked;
        self.repo_input.candidates.clear();
    }

    /// Move the cursor. Inert with the browser closed.
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
