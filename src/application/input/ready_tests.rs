use super::{InputControl, process_ready_input};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use std::collections::VecDeque;

fn key(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

#[test]
fn each_tick_reads_one_source_event_and_dispatches_its_expansion_in_order() {
    let mut incoming = VecDeque::from([Event::FocusGained, Event::FocusLost]);
    let mut reads = 0;
    let mut seen = Vec::new();
    let first = process_ready_input(
        || {
            reads += 1;
            Ok(incoming.pop_front())
        },
        |_| Ok((0u8..65).map(|n| key(char::from(b'a' + n))).collect()),
        |event| {
            let Event::Key(key) = event else {
                unreachable!()
            };
            let KeyCode::Char(c) = key.code else {
                unreachable!()
            };
            seen.push(c);
            Ok(InputControl::Continue)
        },
    )
    .unwrap();
    assert_eq!(first.processed, 65);
    assert!(!first.quit);
    assert_eq!(reads, 1, "a tick must not aggregate later source events");
    assert_eq!(seen[0], 'a');
    assert_eq!(seen[64], char::from(b'a' + 64));

    let second = process_ready_input(
        || {
            reads += 1;
            Ok(incoming.pop_front())
        },
        |event| Ok(vec![event]),
        |event| {
            assert!(matches!(event, Event::FocusLost));
            Ok(InputControl::Continue)
        },
    )
    .unwrap();
    assert_eq!(second.processed, 1);
    assert_eq!(reads, 2);
}

#[test]
fn a_quit_in_a_coalesced_batch_stops_dispatching_following_events() {
    let mut incoming = VecDeque::from([Event::FocusGained]);
    let mut seen = Vec::new();
    let progress = process_ready_input(
        || Ok(incoming.pop_front()),
        |_| Ok(vec![key('q'), key('x')]),
        |event| {
            let Event::Key(key) = event else {
                unreachable!()
            };
            let KeyCode::Char(c) = key.code else {
                unreachable!()
            };
            seen.push(c);
            Ok(if c == 'q' {
                InputControl::Quit
            } else {
                InputControl::Continue
            })
        },
    )
    .unwrap();
    assert_eq!(progress.processed, 1);
    assert!(progress.quit);
    assert_eq!(seen, ['q']);
}

#[test]
fn an_empty_ready_source_returns_without_waiting_for_an_event() {
    let mut reads = 0;
    let progress = process_ready_input(
        || {
            reads += 1;
            Ok(None)
        },
        |event| Ok(vec![event]),
        |_| panic!("no event was ready"),
    )
    .unwrap();
    assert_eq!(progress.processed, 0);
    assert_eq!(reads, 1);
}
