//! The deadline and the polling every hub test waits on. `connect` starts a real
//! shell, so no test may assume the frame it wants is already queued.

use super::TerminalSession;
use crate::session::terminal::frame::TerminalFrame;
use std::thread;
use std::time::{Duration, Instant};

/// Deadline for the real-shell tests below. `connect` spawns the user's
/// actual `$SHELL` (an interactive zsh sources its full rc chain), and
/// cargo runs tests in parallel, so several shells initialize at once — a
/// tighter budget was measurably flaky under load. A generous bound only
/// delays the failure verdict; passing runs still finish the instant the
/// frame arrives. Mirrors `backend::pty::tests::PTY_TEST_DEADLINE`.
pub(in crate::session::terminal) const SHELL_TEST_DEADLINE: Duration = Duration::from_secs(15);

pub(in crate::session::terminal) fn wait_for<T>(mut take: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + SHELL_TEST_DEADLINE;
    while Instant::now() < deadline {
        if let Some(value) = take() {
            return Some(value);
        }
        thread::sleep(Duration::from_millis(10));
    }
    None
}

/// Pull frames until one satisfies `want`, ignoring the rest.
pub(in crate::session::terminal) fn next_matching(
    session: &TerminalSession,
    mut want: impl FnMut(&TerminalFrame) -> bool,
) -> Option<TerminalFrame> {
    wait_for(|| {
        session
            .next_frame(Duration::from_millis(50))
            .filter(|f| want(f))
    })
}
