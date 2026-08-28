use crate::application::terminal_guard::TuiTerminal;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};

pub(crate) enum SplashOutcome {
    Enter,
    Quit,
}

/// Run the splash until it times out or a key dismisses it.
///
/// `accent_idx` is the session's, read from its file rather than taken from
/// the daemon: the splash draws before this client has attached, so the
/// broadcast that carries the colour has not arrived yet.
pub(crate) fn splash_loop(
    terminal: &mut TuiTerminal,
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
                // Esc aborts during the splash (the leader needs two keys, so
                // it is not recognised here); any other key dismisses it.
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
