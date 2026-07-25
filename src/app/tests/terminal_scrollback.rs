use super::*;

#[test]
fn terminal_scrollback_uses_full_buffer() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    app.terminal.active = 0;
    app.terminal.size = (3, 10);

    let mut emulator = crate::runtime::emulator::PaneEmulator::new(3, 10, SCROLLBACK_LINES);
    emulator.process(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n9\r\n");
    app.terminal.emulators.insert(1, emulator);
    // Request scrolling well past screen height; the emulator supports
    // arbitrary offsets up to the buffered line count.
    app.terminal.scroll.insert(1, 6);

    app.terminal.sync_scroll();

    let actual = app.terminal.emulators.get(&1).unwrap().scroll_offset();
    assert_eq!(actual, 6);
    assert_eq!(app.terminal.scroll.get(&1).copied(), Some(6));
}

#[test]
fn terminal_scrollback_clamps_to_buffered_rows() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    app.terminal.active = 0;
    app.terminal.size = (3, 10);

    let mut emulator = crate::runtime::emulator::PaneEmulator::new(3, 10, SCROLLBACK_LINES);
    // Only a handful of buffered rows exist; an outsized request must
    // clamp to whatever the emulator actually has, never panic.
    emulator.process(b"1\r\n2\r\n3\r\n4\r\n5\r\n");
    app.terminal.emulators.insert(1, emulator);
    app.terminal.scroll.insert(1, 999);

    app.terminal.sync_scroll();

    let stored = app.terminal.scroll.get(&1).copied().unwrap_or(0);
    let actual = app.terminal.emulators.get(&1).unwrap().scroll_offset();
    assert_eq!(stored, actual);
    assert!(actual < 999);
}
