//! Screen geometry the attached TUI reuses across ticks.
//!
//! An idle tick changes nothing on screen, yet the loop still needs the
//! terminal's size to lay the panes out and keep their PTYs honest. Asking the
//! backend for the size is a syscall and laying the panes out allocates, so
//! both are cached here and rebuilt only when something they depend on moved.
//! A resize event or a full repaint marks the size stale so the next tick
//! re-measures: the frame is drawn at the backend's own size, which an event's
//! dimensions do not always match, and `Terminal::clear` is the point where a
//! missed resize would otherwise leave panes at a stale size for good.

use crate::app::App;
use crate::backend::PaneId;
use crate::config::{LayoutConfig, TabStrip};
use crate::runtime::terminal::TerminalFullscreen;
use crate::ui::terminal_tab::visible_pane_ids;
use ratatui::layout::{Rect, Size};

/// Cached terminal size and pane sizes for one TUI session.
#[derive(Debug, Default)]
pub(crate) struct ScreenGeometry {
    screen: Option<Rect>,
    /// Set by [`ScreenGeometry::forget_size`]; the last usable `screen` is kept
    /// as the fallback for a transient zero measurement.
    stale: bool,
    sizes: Option<CachedSizes>,
    #[cfg(test)]
    rebuilds: usize,
}

impl ScreenGeometry {
    /// The screen rect, measuring the terminal only while no size is known.
    pub(crate) fn screen(
        &mut self,
        measure: impl FnOnce() -> anyhow::Result<Size>,
    ) -> anyhow::Result<Rect> {
        if let Some(screen) = self.screen
            && !self.stale
        {
            return Ok(screen);
        }
        let size = measure()?;
        Ok(self.record(size.width, size.height))
    }

    /// Forget the size so the next [`ScreenGeometry::screen`] measures the
    /// backend again.
    pub(crate) fn forget_size(&mut self) {
        self.stale = true;
    }

    fn record(&mut self, width: u16, height: u16) -> Rect {
        // A zero dimension is a transient no layout can express; keep the last
        // usable screen so panes are left where they are rather than collapsed.
        if width == 0 || height == 0 {
            return self.screen.unwrap_or_default();
        }
        let screen = Rect::new(0, 0, width, height);
        self.screen = Some(screen);
        self.stale = false;
        screen
    }

    /// Desired (rows, cols) for every visible pane, rebuilt only when the
    /// layout inputs or the visible pane set changed.
    pub(crate) fn pane_sizes(
        &mut self,
        app: &App,
        layout: &LayoutConfig,
        screen: Rect,
    ) -> &[(PaneId, u16, u16)] {
        let key = SizeKey::of(app, layout, screen);
        let fresh = self
            .sizes
            .as_ref()
            .is_some_and(|cached| cached.key == key && cached.covers_the_same_panes(app));
        if !fresh {
            let sizes = crate::ui::terminal_content_areas(app, screen, layout)
                .into_iter()
                .map(|(id, area)| (id, area.height, area.width))
                .collect();
            #[cfg(test)]
            {
                self.rebuilds += 1;
            }
            self.sizes = Some(CachedSizes { key, sizes });
        }
        &self.sizes.as_ref().expect("sizes were just filled").sizes
    }

    #[cfg(test)]
    pub(crate) fn rebuilds(&self) -> usize {
        self.rebuilds
    }
}

#[derive(Debug)]
struct CachedSizes {
    key: SizeKey,
    sizes: Vec<(PaneId, u16, u16)>,
}

impl CachedSizes {
    /// A same-sized set of panes can still name different panes — closing one
    /// and opening another between two ticks keeps the count but moves the
    /// rectangles to new ids.
    fn covers_the_same_panes(&self, app: &App) -> bool {
        self.sizes.is_empty()
            || self
                .sizes
                .iter()
                .map(|(id, _, _)| *id)
                .eq(visible_pane_ids(app))
    }
}

/// Everything the pane layout is derived from, in one comparable value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SizeKey {
    screen: Rect,
    upper_pct: u16,
    file_list_pct: u16,
    tabs: TabStrip,
    terminal: TerminalFullscreen,
    list_fullscreen: bool,
    diff_fullscreen: bool,
    pane_count: usize,
    active: usize,
    visible_start: usize,
    max_visible: usize,
}

impl SizeKey {
    fn of(app: &App, layout: &LayoutConfig, screen: Rect) -> Self {
        Self {
            screen,
            upper_pct: layout.upper_pct,
            file_list_pct: layout.file_list_pct,
            tabs: layout.tabs,
            terminal: app.terminal.fullscreen,
            list_fullscreen: app.list_fullscreen,
            diff_fullscreen: app.diff_pane().fullscreen,
            pane_count: app.terminal.panes.len(),
            active: app.terminal.active,
            visible_start: app.terminal.visible_start,
            max_visible: app.terminal.max_visible(),
        }
    }
}

#[cfg(test)]
#[path = "screen_geometry_tests.rs"]
mod tests;
