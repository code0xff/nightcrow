use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Quit,
    Up,
    Down,
    PageUp,
    PageDown,
    FocusToggle,
    None,
}

pub fn map_key(event: KeyEvent) -> Action {
    match (event.code, event.modifiers) {
        (KeyCode::Char('q'), _) => Action::Quit,
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => Action::Up,
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => Action::Down,
        (KeyCode::PageUp, _) => Action::PageUp,
        (KeyCode::PageDown, _) => Action::PageDown,
        (KeyCode::Tab, _) => Action::FocusToggle,
        _ => Action::None,
    }
}
