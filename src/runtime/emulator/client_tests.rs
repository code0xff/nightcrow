//! A client emulator skips the stream-boundary tracking only the session hub
//! reads, so its screen must be identical to a tracking emulator's for the
//! same bytes.

use super::{PaneEmulator, ScreenView};

type RenderedCell = (
    String,
    ratatui::style::Color,
    ratatui::style::Color,
    bool,
    bool,
    bool,
    bool,
    bool,
    bool,
);

fn rendered(view: &ScreenView<'_>) -> Vec<RenderedCell> {
    let (rows, cols) = view.size();
    let mut cells = Vec::with_capacity(usize::from(rows) * usize::from(cols));
    for row in 0..rows {
        for col in 0..cols {
            let cell = view.cell(row, col).expect("cell inside the view");
            let mut contents = String::new();
            cell.append_contents(&mut contents);
            cells.push((
                contents,
                cell.fg(),
                cell.bg(),
                cell.bold(),
                cell.italic(),
                cell.underline(),
                cell.inverse(),
                cell.dim(),
                cell.is_wide_spacer(),
            ));
        }
    }
    cells
}

#[test]
fn a_client_emulator_renders_the_same_screen_as_a_tracking_emulator() {
    let han = "\u{d55c}".as_bytes();
    // Chunks cut inside escape sequences and a multi-byte character, the way a
    // PTY read does; where a boundary falls decides only where a snapshot may
    // be anchored, never what the grid shows.
    let stream: Vec<&[u8]> = vec![
        b"\x1b]0;a title\x07".as_slice(),
        b"plain text\r\nsecond line",
        b"\x1b[1;5H\x1b[38;5",
        b";208mcolour\x1b[m",
        &han[..1],
        &han[1..],
        b"\x1b[?1h\x1b[?2004h",
        b"\x1b[2;1H\x1b[4munder\x1b[24m",
        b"\x1b[?1049h\x1b[1;1Halternate",
        b"\x1b[?2026h\x1b[3;1Hheld back",
        b"\x1b[?2026l",
        b"\x1b[?1049l",
    ];

    let mut tracking = PaneEmulator::new(6, 20, 8);
    let mut client = PaneEmulator::without_boundary_tracking(6, 20, 8);
    for chunk in stream {
        let tracking_events = tracking.process(chunk);
        let client_events = client.process(chunk);
        assert_eq!(client_events.title, tracking_events.title, "{chunk:?}");
        assert_eq!(client_events.bell, tracking_events.bell, "{chunk:?}");
        assert_eq!(
            client_events.pty_writes, tracking_events.pty_writes,
            "{chunk:?}"
        );
    }

    assert_eq!(client.view().size(), tracking.view().size());
    assert_eq!(
        client.view().cursor_position(),
        tracking.view().cursor_position()
    );
    assert_eq!(rendered(&client.view()), rendered(&tracking.view()));
}
