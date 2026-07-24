use crate::backend::PaneId;
use crate::runtime::emulator::{PaneEmulator, ScreenView};

use super::{TerminalFullscreen, TerminalState, visible_range};

impl TerminalState {
    pub fn active_pane_id(&self) -> Option<PaneId> {
        self.panes.get(self.active).map(|p| p.id)
    }

    /// Maximum number of panes shown at once in the current fullscreen state.
    /// `Zoom` caps at 1 so the shared grid path renders only the active pane.
    pub fn max_visible(&self) -> usize {
        match self.fullscreen {
            TerminalFullscreen::Off => self.max_visible_normal,
            TerminalFullscreen::Grid => self.max_visible_fullscreen,
            TerminalFullscreen::Zoom => 1,
        }
    }

    /// Whether `Zoom` would render differently from `Grid` — i.e. whether
    /// `Grid` would show more than one pane. When false the two are
    /// indistinguishable, so the fullscreen cycle skips `Zoom` and a pane
    /// close normalizes `Zoom` back to `Grid`. Guards against both a lone pane
    /// and a `max_visible_fullscreen` of 1, so no site has to assume the cap
    /// is ≥ 2.
    pub fn zoom_distinct_from_grid(&self) -> bool {
        self.max_visible_fullscreen.min(self.panes.len()) > 1
    }

    /// Last known content size for `id`, falling back to the default pane
    /// size for a pane that hasn't been through a layout resize yet.
    pub fn pane_size(&self, id: PaneId) -> (u16, u16) {
        self.last_content_size
            .get(&id)
            .copied()
            .unwrap_or(self.size)
    }

    /// Row count used for terminal-scroll paging: the active pane's own
    /// content height when known, otherwise the default pane size. Callers
    /// used to read `size` directly, which no longer tracks per-pane height.
    pub fn active_pane_rows(&self) -> usize {
        self.active_pane_id()
            .map(|id| self.pane_size(id).0 as usize)
            .unwrap_or(self.size.0 as usize)
    }

    /// Re-clamp `visible_start` against the current active pane and pane
    /// count. Must be called after anything that changes `active` or
    /// `panes.len()` (focus jumps, pane create/close, session restore) so
    /// the split-view window always contains the active pane.
    pub fn sync_visible_window(&mut self) {
        let range = visible_range(
            self.visible_start,
            self.active,
            self.panes.len(),
            self.max_visible(),
        );
        self.visible_start = range.start;
    }

    /// Screen for a specific pane, independent of which pane is currently
    /// active — the split-view renderer draws every visible pane, not just
    /// the focused one.
    pub fn screen_for_pane(&self, id: PaneId) -> Option<ScreenView<'_>> {
        self.emulators.get(&id).map(PaneEmulator::view)
    }

    pub fn active_screen(&self) -> Option<ScreenView<'_>> {
        let id = self.active_pane_id()?;
        self.screen_for_pane(id)
    }

    pub fn is_scrolled(&self) -> bool {
        self.active_pane_id()
            .and_then(|id| self.scroll.get(&id))
            .is_some_and(|&v| v > 0)
    }
}