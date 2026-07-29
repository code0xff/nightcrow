//! Raw mode and the alternate screen: entered once at startup, restored on the
//! way out however the process leaves.

use anyhow::Result;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use std::io;

pub(crate) struct TerminalGuard;

impl TerminalGuard {
    pub(crate) fn enter(mouse: bool) -> Result<Self> {
        enable_raw_mode()?;
        // EnableBracketedPaste makes crossterm surface paste as
        // `Event::Paste(String)` instead of a flood of `Event::Key` chars —
        // the latter would each be filtered as control chars by the search
        // handler and silently drop newlines.
        if let Err(err) = execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste) {
            let _ = disable_raw_mode();
            return Err(err.into());
        }
        // Mouse capture is config-gated (`[mouse] enabled`): while captured,
        // the outer terminal only selects text with Shift held, so users who
        // prefer plain-drag selection can hand the mouse back entirely.
        if mouse && let Err(err) = execute!(io::stdout(), EnableMouseCapture) {
            // The enable may have partially reached the terminal even though
            // the call errored (e.g. the write landed but a later flush
            // failed), and no TerminalGuard exists yet to undo it on drop —
            // send the disable explicitly; it is harmless when capture never
            // took effect.
            let _ = execute!(
                io::stdout(),
                DisableMouseCapture,
                DisableBracketedPaste,
                LeaveAlternateScreen
            );
            let _ = disable_raw_mode();
            return Err(err.into());
        }

        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // DisableMouseCapture is unconditional: it merely writes the reset
        // sequences, which are harmless when capture was never enabled.
        let _ = execute!(
            io::stdout(),
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}
