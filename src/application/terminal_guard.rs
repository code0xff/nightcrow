//! Raw mode and the alternate screen: entered once at startup, restored on the
//! way out however the process leaves.

use anyhow::Result;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use crossterm::{
    Command, execute,
    terminal::{
        DisableLineWrap, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen,
        disable_raw_mode, enable_raw_mode,
    },
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, BufWriter, Write};

/// The render target for every screen the TUI draws.
///
/// `io::stdout()` is line buffered, and a Ratatui frame carries no newlines —
/// so a frame leaves this process in buffer-sized pieces rather than as one
/// write. On Windows each piece is its own console write that the host has to
/// re-parse, which is both the expensive path and the one where a frame can be
/// split. Buffering a whole frame collapses that to a single write per
/// `flush`, which Ratatui already performs exactly once per `draw`.
pub(crate) type TuiTerminal = Terminal<CrosstermBackend<BufWriter<io::Stdout>>>;

/// Sized so a full-screen redraw with per-cell styling never has to be split:
/// worst case is roughly one SGR sequence per cell, and a large window is a few
/// hundred thousand bytes of those.
const FRAME_BUFFER_BYTES: usize = 1 << 20;

/// Build the buffered terminal. Enter [`TerminalGuard`] first — this only opens
/// the writer, it does not touch the terminal's modes.
pub(crate) fn open_terminal() -> io::Result<TuiTerminal> {
    Terminal::new(CrosstermBackend::new(BufWriter::with_capacity(
        FRAME_BUFFER_BYTES,
        io::stdout(),
    )))
}

pub(crate) struct TerminalGuard;

impl TerminalGuard {
    pub(crate) fn enter(mouse: bool) -> Result<Self> {
        enable_raw_mode()?;
        // EnableBracketedPaste makes crossterm surface paste as
        // `Event::Paste(String)` instead of a flood of `Event::Key` chars —
        // the latter would each be filtered as control chars by the search
        // handler and silently drop newlines. Unix only — the Windows console
        // has no paste record, so `input::burst` reassembles the flood.
        // Ratatui positions every changed cell itself. Host-side autowrap is
        // therefore both unnecessary and dangerous: writing the bottom-right
        // cell can scroll the physical screen while Ratatui's back buffer still
        // describes the pre-scroll frame, leaving duplicated rows and stale
        // fragments on subsequent partial draws.
        if let Err(err) = execute!(io::stdout(), EnterAlternateScreen, DisableLineWrap) {
            restore_terminal();
            return Err(err.into());
        }
        if let Err(err) = execute!(io::stdout(), EnableBracketedPaste) {
            if err.kind() == io::ErrorKind::Unsupported {
                tracing::warn!("bracketed paste unavailable; multi-line paste will be degraded");
            } else {
                restore_terminal();
                return Err(err.into());
            }
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
            restore_terminal();
            return Err(err.into());
        }

        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

/// Restore every terminal mode nightcrow may have enabled.
///
/// The reset sequences and `disable_raw_mode` are safe to repeat, which lets
/// the panic hook restore the terminal immediately while `TerminalGuard` still
/// performs the same cleanup during unwinding.
pub(crate) fn restore_terminal() {
    let _ = write_terminal_restore(io::stdout());
    let _ = disable_raw_mode();
}

fn write_terminal_restore(mut writer: impl Write) -> io::Result<()> {
    #[cfg(windows)]
    {
        // Mouse capture is a WinAPI console-input mode even when output VT
        // sequences are supported. Restore it through WinAPI unconditionally;
        // emitting its ANSI spelling does not restore that saved input mode.
        let mouse = DisableMouseCapture.execute_winapi();
        if EnableLineWrap.is_ansi_code_supported() {
            let ansi = write_terminal_restore_ansi(&mut writer);
            return mouse.and(ansi);
        }

        // Bracketed paste has no legacy WinAPI equivalent. Run the remaining
        // restores independently so its Unsupported error cannot prevent the
        // line-wrap or screen-buffer restoration.
        let wrap = EnableLineWrap.execute_winapi();
        let screen = LeaveAlternateScreen.execute_winapi();
        mouse.and(wrap).and(screen)
    }

    #[cfg(not(windows))]
    {
        write_terminal_restore_ansi(&mut writer)
    }
}

fn write_terminal_restore_ansi(mut writer: impl Write) -> io::Result<()> {
    // Mouse capture is unconditional: the reset sequences are harmless when
    // capture was never enabled.
    let mut commands = String::new();
    DisableMouseCapture
        .write_ansi(&mut commands)
        .map_err(io::Error::other)?;
    DisableBracketedPaste
        .write_ansi(&mut commands)
        .map_err(io::Error::other)?;
    EnableLineWrap
        .write_ansi(&mut commands)
        .map_err(io::Error::other)?;
    LeaveAlternateScreen
        .write_ansi(&mut commands)
        .map_err(io::Error::other)?;
    writer.write_all(commands.as_bytes())?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::write_terminal_restore_ansi;

    #[test]
    fn terminal_restore_disables_paste_and_leaves_the_alternate_screen() {
        let mut output = Vec::new();

        write_terminal_restore_ansi(&mut output).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("\u{1b}[?2004l"));
        assert!(output.contains("\u{1b}[?7h"));
        assert!(output.contains("\u{1b}[?1049l"));
    }
}
