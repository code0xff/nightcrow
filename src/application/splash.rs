use crate::workspace::Workspace;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

pub(crate) enum SplashOutcome {
    Enter,
    Quit,
}

pub(crate) fn splash_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ws: &Workspace,
    fallback_accent: usize,
) -> anyhow::Result<SplashOutcome> {
    let splash = crate::ui::splash::SplashState::new();
    // With no project open there is no restored accent to honour, so the
    // configured preset stands in.
    let accent = ws
        .active()
        .map(|p| p.current_accent())
        .unwrap_or_else(|| crate::config::Accent::from_index(fallback_accent).color());
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
