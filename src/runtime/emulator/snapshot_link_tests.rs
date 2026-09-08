use super::PaneEmulator;
use alacritty_terminal::term::cell::Hyperlink;

#[test]
fn snapshot_round_trip_preserves_linked_wide_cells_and_following_link_state() {
    let target = "https://example.test/roundtrip";
    let input = format!("\x1b]8;;{target}\x1b\\가 \x1b]8;;\x1b\\");
    let mut origin = PaneEmulator::new(3, 30, 0);
    origin.process(input.as_bytes());
    let mut replayed = PaneEmulator::new(3, 30, 0);
    replayed.process(b"\x1b]8;;https://stale.example\x1b\\stale");
    replayed.process(&origin.screen_snapshot());

    assert_eq!(origin.view().link_at(0, 0).as_deref(), Some(target));
    assert_eq!(origin.view().link_at(0, 1).as_deref(), Some(target));
    assert_eq!(origin.view().link_at(0, 2).as_deref(), Some(target));
    assert_eq!(replayed.view().link_at(0, 0).as_deref(), Some(target));
    assert_eq!(replayed.view().link_at(0, 1).as_deref(), Some(target));
    assert_eq!(replayed.view().link_at(0, 2).as_deref(), Some(target));

    origin.process(b"X");
    replayed.process(b"X");
    assert_eq!(replayed.view().link_at(0, 3), None);
    assert_eq!(origin.view().link_at(0, 3), None);
}

#[test]
fn snapshot_round_trip_preserves_a_linked_leading_wide_spacer() {
    let target = "https://example.test/wrapped";
    let input = format!("abc\x1b]8;;{target}\x1b\\한\x1b]8;;\x1b\\");
    let mut origin = PaneEmulator::new(3, 4, 0);
    origin.process(input.as_bytes());
    let mut replayed = PaneEmulator::new(3, 4, 0);
    replayed.process(&origin.screen_snapshot());

    for (row, col) in [(0, 3), (1, 0), (1, 1)] {
        assert_eq!(origin.view().link_at(row, col).as_deref(), Some(target));
        assert_eq!(replayed.view().link_at(row, col).as_deref(), Some(target));
    }
}

#[test]
fn snapshot_restores_the_cursor_hyperlink_for_following_output() {
    let target = "https://example.test/active";
    let input = format!("\x1b]8;;{target}\x1b\\가 ");
    let mut origin = PaneEmulator::new(3, 30, 0);
    origin.process(input.as_bytes());
    let mut replayed = PaneEmulator::new(3, 30, 0);
    replayed.process(&origin.screen_snapshot());

    origin.process(b"X");
    replayed.process(b"X");
    assert_eq!(origin.view().link_at(0, 3).as_deref(), Some(target));
    assert_eq!(replayed.view().link_at(0, 3).as_deref(), Some(target));
}

#[test]
fn a_no_links_snapshot_stays_compact() {
    let mut emulator = PaneEmulator::new(40, 120, 0);
    emulator.process(b"just a small plain screen");
    let snapshot = emulator.screen_snapshot();
    assert!(snapshot.len() < 40 * 120 / 4);
    assert_eq!(
        snapshot
            .windows(3)
            .filter(|window| *window == b"\x1b]8")
            .count(),
        1,
        "a plain snapshot only needs the incoming-link reset"
    );
}

#[test]
fn unsafe_link_bytes_are_percent_encoded_in_snapshot_fields() {
    let link = Hyperlink::new(None::<String>, "https://example.test/\x1b\\\x07".to_owned());
    let mut output = String::new();
    super::snapshot::write_hyperlink(&mut output, Some(&link));
    assert_eq!(output, "\x1b]8;;https://example.test/%1B\\%07\x1b\\");
}
