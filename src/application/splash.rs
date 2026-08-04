use crate::application::terminal_guard::TuiTerminal;
use crate::ui::splash::FLAP_FRAME;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::Instant;

pub(crate) enum SplashOutcome {
    Enter,
    Quit,
}

/// Flap the crow until the user presses a key.
///
/// There is no dismissal timer — only the animation is timed, so the splash
/// waits as long as the user does.
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
    let mut tick = 0usize;
    let mut next_frame = Instant::now();

    loop {
        if Instant::now() >= next_frame {
            terminal.draw(|frame| {
                crate::ui::splash::draw(frame, accent, tick);
            })?;
            tick = tick.wrapping_add(1);
            next_frame = Instant::now() + FLAP_FRAME;
        }

        // Wait out the rest of the frame rather than a fixed slice, so input
        // stays responsive and mouse traffic cannot race the animation ahead.
        if event::poll(next_frame.saturating_duration_since(Instant::now()))? {
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
                    next_frame = Instant::now();
                }
                _ => {}
            }
        }
    }

    terminal.clear()?;
    Ok(SplashOutcome::Enter)
}
