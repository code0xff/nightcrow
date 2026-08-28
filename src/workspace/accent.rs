//! The colour this client paints the session in.
//!
//! The session owns the accent (see `docs/architecture.md`), so this module
//! only adopts what the daemon reported or works out what to ask for next.

use super::Workspace;

impl Workspace {
    /// Adopt the session's accent. Out-of-range indices are wrapped rather than
    /// refused, matching what the daemon stores.
    pub fn set_accent_index(&mut self, idx: usize) {
        self.accent_idx = idx % crate::config::Accent::ALL.len();
    }

    /// The index the next `<prefix> p` asks for. Derived here rather than by
    /// the daemon so the request names a colour instead of a step — two
    /// clients cycling at once would otherwise land somewhere neither asked
    /// for.
    pub fn next_accent_index(&self) -> usize {
        (self.accent_idx + 1) % crate::config::Accent::ALL.len()
    }

    pub fn current_accent(&self) -> ratatui::style::Color {
        crate::config::Accent::from_index(self.accent_idx).color()
    }
}
