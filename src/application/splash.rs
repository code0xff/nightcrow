use crate::application::terminal_guard::TuiTerminal;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};

pub(crate) enum SplashOutcome {
    Enter,
    Quit,
}

/// Draw the splash once, then block until the user presses a key.
///
/// `accent_idx` is the session's, read from its file rather than taken from the
/// daemon: the splash draws before this client has attached, so the broadcast
/// that carries the colour has not arrived yet. Reading it here is what keeps
/// the splash and the view a moment later from being two different colours.
pub(crate) fn splash_loop(
    terminal: &mut TuiTerminal,
    accent_idx: usize,
) -> anyhow::Result<SplashOutcome> {
    let accent = crate::config::Accent::from_index(accent_idx).color();
    terminal.draw(|frame| {
        crate::ui::splash::draw(frame, accent);
    })?;

    loop {
        if event::poll(std::time::Duration::from_millis(100))? {
            match event::read()? {
                // Honour Esc so the user can abort during the splash instead
                // of being forced to wait for it to clear and quit from the
                // main view. (Leader-based quit needs a two-key sequence, so
                // it isn't recognised on the one-shot splash screen.) Any
                // other key dismisses the splash.
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    if k.code == KeyCode::Esc {
                        return Ok(SplashOutcome::Quit);
                    }
                    break;
                }
                Event::Resize(_, _) => {
                    terminal.clear()?;
                    terminal.draw(|frame| {
                        crate::ui::splash::draw(frame, accent);
                    })?;
                }
                _ => {}
            }
        }
    }

    terminal.clear()?;
    Ok(SplashOutcome::Enter)
}
