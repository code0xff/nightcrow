//! The Windows paste-burst discriminator, tested on every platform — gating it
//! would leave the code that can swallow typed keys unverified where most
//! changes are written.

use crate::application::input::burst::classify;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};

fn key(code: KeyCode, mods: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, mods))
}

fn ch(c: char) -> Event {
    key(KeyCode::Char(c), KeyModifiers::NONE)
}

fn release(c: char) -> Event {
    Event::Key(KeyEvent::new_with_kind_and_state(
        KeyCode::Char(c),
        KeyModifiers::NONE,
        KeyEventKind::Release,
        KeyEventState::NONE,
    ))
}

fn pasted(events: Vec<Event>) -> Option<String> {
    match classify(events).as_slice() {
        [Event::Paste(text)] => Some(text.clone()),
        _ => None,
    }
}

#[test]
fn a_multi_line_burst_is_one_paste_with_carriage_returns() {
    let events = vec![
        ch('h'),
        ch('i'),
        key(KeyCode::Enter, KeyModifiers::NONE),
        ch('y'),
        ch('o'),
    ];

    assert_eq!(
        pasted(events).as_deref(),
        Some("hi\ryo"),
        "Enter inside a burst is a pasted line break, not a submitted line"
    );
}

#[test]
fn a_long_single_line_burst_is_a_paste() {
    // 17 characters: past what one burst can collect from typing.
    let events: Vec<Event> = "abcdefghijklmnopq".chars().map(ch).collect();

    assert_eq!(pasted(events).as_deref(), Some("abcdefghijklmnopq"));
}

#[test]
fn one_typed_character_is_not_a_paste() {
    let events = vec![ch('a')];

    assert!(
        pasted(events).is_none(),
        "typing must keep reaching the per-key dispatch"
    );
}

#[test]
fn a_short_typed_run_is_not_a_paste() {
    let events: Vec<Event> = "abcdefgh".chars().map(ch).collect();

    assert!(pasted(events).is_none());
}

#[test]
fn a_lone_enter_is_not_a_paste() {
    let events = vec![key(KeyCode::Enter, KeyModifiers::NONE)];

    assert!(
        pasted(events).is_none(),
        "a submitted empty line must stay an Enter keypress"
    );
}

#[test]
fn a_ctrl_chord_in_the_burst_forces_per_key_dispatch() {
    let events = vec![
        ch('h'),
        key(KeyCode::Char('c'), KeyModifiers::CONTROL),
        key(KeyCode::Enter, KeyModifiers::NONE),
        ch('i'),
    ];
    let expected = events.clone();

    assert_eq!(
        classify(events),
        expected,
        "an interrupt must not be swallowed into pasted text"
    );
}

#[test]
fn a_function_key_in_the_burst_forces_per_key_dispatch() {
    let events = vec![ch('h'), key(KeyCode::F(2), KeyModifiers::NONE), ch('i')];
    let expected = events.clone();

    assert_eq!(classify(events), expected);
}

#[test]
fn a_mouse_report_in_the_burst_forces_per_event_dispatch() {
    let events = vec![
        ch('h'),
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        }),
        key(KeyCode::Enter, KeyModifiers::NONE),
    ];
    let expected = events.clone();

    assert_eq!(classify(events), expected);
}

#[test]
fn key_releases_do_not_count_as_pasted_characters() {
    // Counting both halves would double every character and make a short typed
    // run look like a paste.
    let events = vec![ch('h'), release('h'), ch('i'), release('i')];

    assert!(pasted(events).is_none());
}

#[test]
fn shifted_characters_stay_in_the_paste() {
    let events = vec![
        key(KeyCode::Char('H'), KeyModifiers::SHIFT),
        ch('i'),
        key(KeyCode::Enter, KeyModifiers::NONE),
        key(KeyCode::Char('Y'), KeyModifiers::SHIFT),
    ];

    assert_eq!(pasted(events).as_deref(), Some("Hi\rY"));
}
