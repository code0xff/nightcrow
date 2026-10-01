use crate::workspace::Workspace;
use crossterm::event::{Event, KeyEventKind, MouseEventKind};

/// Inputs from the person at a screen choose which layout controls shared PTYs.
pub(crate) fn is_user_activity(event: &Event) -> bool {
    match event {
        Event::Key(key) => matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat),
        Event::Paste(_) => true,
        Event::Mouse(mouse) => matches!(
            mouse.kind,
            MouseEventKind::Down(_)
                | MouseEventKind::ScrollDown
                | MouseEventKind::ScrollUp
                | MouseEventKind::ScrollLeft
                | MouseEventKind::ScrollRight
        ),
        _ => false,
    }
}

pub(crate) fn claim_for_user_activity(ws: &mut Workspace, event: &Event) {
    if is_user_activity(event)
        && let Some(app) = ws.active_mut()
    {
        app.request_pane_sizing();
    }
}
