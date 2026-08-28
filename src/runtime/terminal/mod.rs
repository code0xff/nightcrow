use crate::backend::{PaneId, TerminalBackend};
use crate::runtime::emulator::PaneEmulator;
use std::collections::HashMap;
use std::time::Instant;

mod attention;
mod escape;
mod input;
mod lifecycle;
mod recovery;
mod resize;
mod scroll;
mod session_panes;
mod state;
mod sync;

pub(crate) use escape::strip_escape_sequences;
pub use recovery::PaneRecovery;

/// Upper bound on a pane's in-flight prompt buffer before further chars are
/// dropped. Prevents unbounded growth when a program writes a stream of bytes
/// without ever sending `\r` / `\n` (progress bars, large pastes, `yes` piped
/// to cat).
const PROMPT_BUFFER_MAX_BYTES: usize = 4096;

/// Scrollback line cap for every pane emulator.
pub const SCROLLBACK_LINES: usize = 1000;

/// Lines moved by a single line-scroll keypress (`Shift+Up`/`Shift+Down`).
pub const SCROLL_LINE_STEP: usize = 3;

/// Lines one mouse wheel notch scrolls, by terminal convention. Used to
/// convert a line count into a notch count when a pane wants wheel events,
/// and by the mouse handler as the line count of one captured wheel event.
pub const WHEEL_LINES_PER_NOTCH: usize = 3;

pub struct PaneInfo {
    pub id: PaneId,
    pub title: String,
}

/// Default count of panes shown side by side in the normal (non-fullscreen)
/// lower panel before the visible window starts sliding.
pub const MAX_VISIBLE_NORMAL: usize = 4;

/// Default count of panes shown side by side when the terminal panel is
/// fullscreen (still bounded by the F3–F10 direct-jump range).
pub const MAX_VISIBLE_FULLSCREEN: usize = 8;

/// Fullscreen state of the lower terminal panel. `<leader> f` cycles through
/// `Off → Grid → Zoom → Off`.
/// - `Off`: normal split — top viewer above, terminal split-view below.
/// - `Grid`: terminal fills the body; up to `MAX_VISIBLE_FULLSCREEN` panes.
/// - `Zoom`: terminal fills the body showing only the active pane.
///
/// `Grid` and `Zoom` are visually identical whenever `Grid` would show a
/// single pane, so the cycle skips `Zoom` in that case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalFullscreen {
    #[default]
    Off,
    Grid,
    Zoom,
}

impl TerminalFullscreen {
    /// Whether the terminal panel takes over the whole body area (both `Grid`
    /// and `Zoom`), hiding the top diff/list viewer.
    pub fn fills_body(self) -> bool {
        !matches!(self, TerminalFullscreen::Off)
    }
}

/// Compute the visible pane-index window `[start, start+len)` for a split
/// grid capped at `max_visible` panes. `prev_start` is the previous window's
/// start (0 for a fresh terminal); the window is nudged the minimum amount
/// needed to keep `active` inside it, rather than re-centering every call.
/// Shared by `TerminalState::sync_visible_window` and `ui::terminal_tab` so
/// both always agree on what's visible.
pub(crate) fn visible_range(
    prev_start: usize,
    active: usize,
    pane_count: usize,
    max_visible: usize,
) -> std::ops::Range<usize> {
    if pane_count == 0 || max_visible == 0 {
        return 0..0;
    }
    let window = max_visible.min(pane_count);
    let active = active.min(pane_count - 1);
    let max_start = pane_count - window;

    let mut start = prev_start.min(max_start);
    if active < start {
        start = active;
    } else if active >= start + window {
        start = active + 1 - window;
    }
    start..(start + window)
}

pub struct TerminalState {
    pub panes: Vec<PaneInfo>,
    pub active: usize,
    /// Default size used to create a pane before any layout resize has run
    /// (e.g. the very first pane on startup).
    pub size: (u16, u16),
    pub scroll: HashMap<PaneId, usize>,
    pub fullscreen: TerminalFullscreen,
    /// Desired (rows, cols) for each pane while this client owns sizing; the
    /// confirmed session size while it observes another owner. Panes scrolled
    /// out of the visible window keep their last value.
    pub last_content_size: HashMap<PaneId, (u16, u16)>,
    /// Last size confirmed as applied by the backend.
    pub(crate) confirmed_content_size: HashMap<PaneId, (u16, u16)>,
    /// Resize requests awaiting confirmation or a retry deadline.
    pub(crate) pending_content_size: HashMap<PaneId, PendingPaneResize>,
    /// Whether this client's layout is what sets the pane sizes.
    ///
    /// True unless a shared session says otherwise: a PTY has one size, so one
    /// client decides it and the others render the grid they are given.
    pub owns_size: bool,
    /// What each pane's plugin last reported about recovering it, for the panes
    /// any has spoken about. Deliberately outlives a pane's process: the report
    /// that matters most arrives while the pane is gone and its slot is held for
    /// a relaunch. Cleared only by a `cancelled` report (see
    /// [`recovery::RECOVERY_CANCELLED`]).
    pub(crate) recovery: HashMap<PaneId, PaneRecovery>,
    /// Index of the first pane in the visible split-view window.
    pub visible_start: usize,
    pub max_visible_normal: usize,
    pub max_visible_fullscreen: usize,
    /// Titles for panes this client has asked for and not yet been given, in
    /// the order they were asked for. See `create_pane_with`.
    pub(crate) pending_titles: std::collections::VecDeque<Option<String>>,
    pub(crate) emulators: HashMap<PaneId, PaneEmulator>,
    pub(crate) prompt_bufs: HashMap<PaneId, String>,
    /// Title animations currently being observed, keyed by stable pane id.
    title_activity: HashMap<PaneId, attention::TitleActivity>,
    /// Whether this client has an unseen terminal event for the project.
    unread_attention: bool,
    pub(super) prompt_log_enabled: bool,
    pub(crate) backend: Option<Box<dyn TerminalBackend>>,
}

impl TerminalState {
    pub fn new(backend: Option<Box<dyn TerminalBackend>>, prompt_log_enabled: bool) -> Self {
        Self {
            panes: Vec::new(),
            active: 0,
            size: (22, 78),
            scroll: HashMap::new(),
            fullscreen: TerminalFullscreen::Off,
            last_content_size: HashMap::new(),
            confirmed_content_size: HashMap::new(),
            pending_content_size: HashMap::new(),
            owns_size: true,
            recovery: HashMap::new(),
            visible_start: 0,
            max_visible_normal: MAX_VISIBLE_NORMAL,
            max_visible_fullscreen: MAX_VISIBLE_FULLSCREEN,
            pending_titles: std::collections::VecDeque::new(),
            emulators: HashMap::new(),
            prompt_bufs: HashMap::new(),
            title_activity: HashMap::new(),
            unread_attention: false,
            prompt_log_enabled,
            backend,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PendingPaneResize {
    size: (u16, u16),
    attempted_at: Instant,
}

#[cfg(test)]
mod tests;
