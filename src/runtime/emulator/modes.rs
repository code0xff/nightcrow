//! The terminal modes a pane carries, and how to put another terminal into
//! them. Split from `mod.rs` so the alacritty-facing wrapper and this plain
//! description of a pane's state stay separately readable.

/// The terminal modes a program sets once, at startup, and never repeats —
/// a later-attaching client cannot learn them from replayed output. Carried
/// as plain flags so a caller outside this module can hold and compare them,
/// and turned back into sequences by [`PaneModes::prelude`].
///
/// [`Default`] is a *freshly opened* terminal rather than all-false: `25`
/// (visible cursor), `7` (autowrap) and `1007` (alternate scroll) are on until
/// a program turns them off. Pinned to what the emulator actually starts with,
/// so an emulator upgrade that changes its initial mode set fails there rather
/// than silently mis-describing a pane that has printed nothing yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneModes {
    /// DECSET 1049: the program draws on the alternate screen (vim, htop,
    /// Claude Code in fullscreen rendering). The one mode that decides whether
    /// a pane's recorded history is worth replaying at all — an
    /// alternate-screen program's transcript lives in its own memory, and what
    /// reached the pane is incremental paint, not text.
    pub alt_screen: bool,
    /// DECSET 1: arrows send `ESC O A` rather than `ESC [ A`.
    pub app_cursor: bool,
    /// DECSET 2004: pasted text arrives wrapped in `ESC [ 200~`/`ESC [ 201~`.
    pub bracketed_paste: bool,
    /// DECSET 25.
    pub show_cursor: bool,
    /// DECSET 7 (autowrap).
    pub line_wrap: bool,
    /// DECSET 1000: report button presses.
    pub mouse_click: bool,
    /// DECSET 1002: report drags.
    pub mouse_drag: bool,
    /// DECSET 1003: report every motion.
    pub mouse_motion: bool,
    /// DECSET 1006: report in SGR form.
    pub sgr_mouse: bool,
    /// DECSET 1005.
    pub utf8_mouse: bool,
    /// DECSET 1007: the wheel sends arrow keys on the alternate screen.
    pub alternate_scroll: bool,
    /// DECSET 1004: report focus changes.
    pub focus_in_out: bool,
}

impl Default for PaneModes {
    fn default() -> Self {
        Self {
            alt_screen: false,
            app_cursor: false,
            bracketed_paste: false,
            show_cursor: true,
            line_wrap: true,
            mouse_click: false,
            mouse_drag: false,
            mouse_motion: false,
            sgr_mouse: false,
            utf8_mouse: false,
            alternate_scroll: true,
            focus_in_out: false,
        }
    }
}

impl PaneModes {
    /// The sequences that put another terminal into this state.
    ///
    /// **Every** tracked mode is emitted, set or reset, rather than only those
    /// differing from a fresh terminal: the receiver is xterm.js, whose
    /// defaults are its own business and need not match this emulator's. An
    /// absolute prelude cannot be wrong about them; a relative one would
    /// silently leave a mode at whatever the other side happens to start with.
    /// The cost is a hundred bytes once per pane per connection. `1049` leads:
    /// it switches buffers, and the rest must land in the buffer the program
    /// is drawing on.
    pub fn prelude(&self) -> Vec<u8> {
        let modes = [
            (1049, self.alt_screen),
            (1, self.app_cursor),
            (7, self.line_wrap),
            (25, self.show_cursor),
            (1000, self.mouse_click),
            (1002, self.mouse_drag),
            (1003, self.mouse_motion),
            (1004, self.focus_in_out),
            (1005, self.utf8_mouse),
            (1006, self.sgr_mouse),
            (1007, self.alternate_scroll),
            (2004, self.bracketed_paste),
        ];
        let mut out = Vec::new();
        for (number, enabled) in modes {
            let action = if enabled { 'h' } else { 'l' };
            out.extend_from_slice(format!("\x1b[?{number}{action}").as_bytes());
        }
        out
    }
}
