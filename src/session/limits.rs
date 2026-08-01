//! Resource ceilings owned by the shared terminal session.

/// Terminals one repository may hold open at once.
pub const MAX_PTYS_PER_REPO: usize = 8;

/// Bounds on a PTY's size. Client-supplied dimensions are clamped rather than
/// trusted because the child's allocation grows with the requested area.
pub const MIN_PANE_DIMENSION: u16 = 1;
pub const MAX_PANE_ROWS: u16 = 500;
pub const MAX_PANE_COLS: u16 = 1_100;

/// Raw PTY bytes retained per terminal for a reconnecting client.
pub const MAX_TERMINAL_SCROLLBACK_BYTES: usize = 256 * 1024;
