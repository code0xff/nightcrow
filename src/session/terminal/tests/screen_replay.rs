//! The screen a reattaching client actually ends up looking at.
//!
//! The tests in [`reattach`](super::reattach) check what the hub *sends*. This
//! one closes the loop: it renders what the original client received and what a
//! reattaching client received into two emulators and requires the screens to be
//! identical, cell for cell. Everything the path is made of is in the comparison
//! — a real PTY, the hub's own tracking emulator, the snapshot it serializes, and
//! the client-side emulator that parses it.
//!
//! Unix-only: the fixture needs a shell, `printf` and ANSI escapes.
#![cfg(unix)]

use super::{SHELL_TEST_DEADLINE, attach, created_pane, next_matching};
use crate::backend::PaneId;
use crate::runtime::emulator::PaneEmulator;
use crate::session::terminal::TerminalSession;
use crate::session::terminal::frame::{ClientMessage, TerminalFrame};
use std::time::{Duration, Instant};

const ROWS: u16 = 24;
const COLS: u16 = 80;

/// Long enough for what the hub has queued to be read, short enough not to pad
/// the suite. The frames are already sent by the time either client reads.
const QUIET: Duration = Duration::from_millis(300);

/// A full-screen paint using everything a snapshot has to carry: colours by name,
/// by index and by RGB, several attributes, wide characters, and a cursor left
/// somewhere that is not the origin. `%s` keeps the shell's echo of the command
/// line from being mistaken for the program's own output.
const RICH_PAINT: &str = concat!(
    "printf '\\033[?1049h\\033[2J",
    "\\033[1;1H\\033[31;1mRED BOLD\\033[m",
    "\\033[2;1H\\033[38;5;208morange\\033[m \\033[38;2;10;120;200mtruecolour\\033[m",
    "\\033[3;1H\\033[44;97m on blue \\033[m \\033[4munderline\\033[m \\033[7minverse\\033[m",
    "\\033[5;1H한글 텍스트가 여기 있다",
    "\\033[7;3HPAINT%sED",
    "\\033[12;41H'\n"
);
const PAINTED: &str = "PAINTED";

/// Everything `session` has been sent for `pane`, read until `marker` has arrived
/// and the stream has fallen quiet.
///
/// The marker is looked for in the accumulated bytes rather than in each frame: a
/// PTY splits its output wherever it likes, and this fixture's paint is long
/// enough that the marker straddles two chunks.
fn output_through(session: &TerminalSession, pane: PaneId, marker: &str) -> Vec<u8> {
    let deadline = Instant::now() + SHELL_TEST_DEADLINE;
    let mut out = Vec::new();
    let mut arrived = false;
    while Instant::now() < deadline {
        match session.next_frame(QUIET) {
            Some(TerminalFrame::Output { pane: p, data }) if p == pane => {
                out.extend(data);
                arrived = arrived || String::from_utf8_lossy(&out).contains(marker);
            }
            Some(_) => {}
            // Quiet. Everything the pane had to say about `marker` is in hand.
            None if arrived => return out,
            None => {}
        }
    }
    panic!(
        "the pane never produced {marker:?}, got: {:?}",
        String::from_utf8_lossy(&out)
    )
}

/// One cell as a client would draw it, through the same read-only surface the
/// renderers use.
type Drawn = (String, String, bool);

fn drawn(emulator: &PaneEmulator) -> Vec<Drawn> {
    let view = emulator.view();
    let (rows, cols) = view.size();
    let mut out = Vec::with_capacity(usize::from(rows) * usize::from(cols));
    for row in 0..rows {
        for col in 0..cols {
            let Some(cell) = view.cell(row, col) else {
                continue;
            };
            let mut contents = String::new();
            cell.append_contents(&mut contents);
            let style = format!(
                "{:?}/{:?}/{}{}{}{}{}",
                cell.fg(),
                cell.bg(),
                cell.bold() as u8,
                cell.italic() as u8,
                cell.underline() as u8,
                cell.inverse() as u8,
                cell.dim() as u8,
            );
            out.push((contents, style, cell.is_wide_spacer()));
        }
    }
    out
}

fn rendered(stream: &[u8]) -> PaneEmulator {
    let mut emulator = PaneEmulator::new(ROWS, COLS, 0);
    emulator.process(stream);
    emulator
}

/// The whole point, end to end: a client that was not there sees what the client
/// that was there sees.
#[test]
fn a_reattaching_client_ends_up_with_the_same_screen_as_the_original() {
    let dir = tempfile::TempDir::new().unwrap();
    // Bash for a deterministic fixture: an interactive zsh's rc chain would paint
    // over the screen being compared.
    let hub = super::super::TerminalHub::spawn(
        &dir.path().to_string_lossy(),
        Vec::new(),
        Vec::new(),
        crate::config::ShellConfig {
            program: Some("bash".to_string()),
            command_args: Vec::new(),
        },
        Default::default(),
    );

    let original = attach(&hub);
    original.dispatch(ClientMessage::Create {
        rows: ROWS,
        cols: COLS,
    });
    let created =
        next_matching(&original, |f| created_pane(f).is_some()).expect("no created message");
    let pane = created_pane(&created).unwrap();

    original.dispatch(ClientMessage::Input {
        pane,
        data: format!("PS1='$ '\nunset PROMPT_COMMAND\n{RICH_PAINT}"),
    });
    // Read to quiet before attaching, so the paint is wholly recorded and the
    // comparison is not racing the last chunk of it.
    let live = output_through(&original, pane, PAINTED);
    let reattached = attach(&hub);
    let replay = output_through(&reattached, pane, PAINTED);
    hub.stop();

    let from_live = rendered(&live);
    let from_replay = rendered(&replay);
    assert!(
        drawn(&from_live).iter().any(|(c, _, _)| c == "R"),
        "the fixture must have painted something to compare"
    );
    assert_eq!(
        drawn(&from_replay),
        drawn(&from_live),
        "a reattaching client's screen must match the original's"
    );
    assert_eq!(
        from_replay.view().cursor_position(),
        from_live.view().cursor_position(),
        "and its cursor must be in the same cell"
    );
}
