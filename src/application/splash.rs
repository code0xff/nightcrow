use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

pub(crate) enum SplashOutcome {
    Enter,
    Quit,
}

/// Run the splash until it times out or a key dismisses it.
///
/// `accent_idx` is the session's, read from its file rather than taken from the
/// daemon: the splash draws before this client has attached, so the broadcast
/// that carries the colour has not arrived yet. Reading it here is what keeps
/// the splash and the view a moment later from being two different colours.
pub(crate) fn splash_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    accent_idx: usize,
) -> anyhow::Result<SplashOutcome> {
    let splash = crate::ui::splash::SplashState::new();
    let accent = crate::config::Accent::from_index(accent_idx).color();
    loop {
        terminal.draw(|frame| {
            crate::ui::splash::draw(frame, &splash, accent);
        })?;
        if splash.is_done() {
            break;
        }
        if event::poll(std::time::Duration::from_millis(16))? {
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
                Event::Resize(_, _) => terminal.clear()?,
                _ => {}
            }
        }
    }
    terminal.clear()?;
    Ok(SplashOutcome::Enter)
}
