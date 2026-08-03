//! What a client gets when it attaches to panes that were already running.
//!
//! The pane's recorded history is a byte window, so everything a program did
//! before that window is unrecoverable from it — the modes it set at startup, and
//! (for a program drawing on the alternate screen) the screen itself. These are
//! the two answers the hub gives instead: state it tracked, and a repaint it asks
//! the program for.
//!
//! These tests are Unix-only: they use `printf`, `trap`, and ANSI escape
//! sequences that require a Unix shell.
#![cfg(unix)]

use super::{attach, created_pane, created_title, next_matching};
use crate::backend::PaneId;
use crate::session::terminal::TerminalHub;
use crate::session::terminal::TerminalSession;
use crate::session::terminal::frame::{ClientMessage, TerminalFrame};
use std::time::Duration;

/// Long enough for anything the hub has already queued to be read, short enough
/// that a test asserting silence does not sit through the shell deadline.
const QUIET: Duration = Duration::from_millis(300);

const ROWS: u16 = 24;
const COLS: u16 = 80;

/// Everything a program on the alternate screen turns on at startup, and the one
/// marker it paints there. Sent as one `printf` so the pane's tracker sees the
/// modes and the paint together.
///
/// The markers carry an empty `%s` so that the shell's echo of the command line —
/// which reaches the pane before the program's own bytes do — cannot be mistaken
/// for the output. `printf 'PAINT%sED'` echoes as `PAINT%sED` and prints
/// `PAINTED`, so a test waiting for the latter is waiting for the program.
const ENTER_FULLSCREEN: &str =
    "printf '\\033[?1049h\\033[?1002h\\033[?1006h\\033[?2004hPAINT%sED'\n";
const PAINTED: &str = "PAINTED";

/// A program naming itself. Written with the same `%s` marker so the assertion
/// waits for the program's bytes: the shell's echo of this command line carries
/// the *text* `\033]2;` rather than an escape, so it cannot set a title.
fn set_title(title: &str) -> String {
    format!("printf '\\033]2;{title}\\007PAINT%sED'\n")
}

/// What a client that was not there is told the pane is called.
fn title_on_attach(hub: &std::sync::Arc<TerminalHub>) -> Option<String> {
    let second = attach(hub);
    let created =
        next_matching(&second, |f| created_pane(f).is_some()).expect("the pane was not announced");
    created_title(&created)
}

/// Output this session has been sent for `pane`, read until it falls quiet.
fn output_for(session: &TerminalSession, pane: PaneId) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(frame) = session.next_frame(QUIET) {
        if let TerminalFrame::Output { pane: p, data } = frame
            && p == pane
        {
            out.extend(data);
        }
    }
    out
}

/// A hub with one pane whose program has run `sequences`, held together so the
/// temporary directory outlives the panes running in it.
struct Running {
    hub: std::sync::Arc<TerminalHub>,
    pane: PaneId,
    /// The client that opened the pane. Kept connected: a pane's only client
    /// going away is a different situation from a second one arriving.
    _first: TerminalSession,
    _dir: tempfile::TempDir,
}

fn pane_running(sequences: &str) -> Running {
    let dir = tempfile::TempDir::new().unwrap();
    // These assertions exercise the hub's repaint protocol, not the user's rc
    // files or shell-specific trap syntax. Bash is present on both supported
    // Unix targets and gives the fixture one deterministic WINCH handler.
    let hub = TerminalHub::spawn(
        &dir.path().to_string_lossy(),
        Vec::new(),
        Vec::new(),
        crate::config::ShellConfig {
            program: Some("bash".to_string()),
            command_args: Vec::new(),
        },
        Default::default(),
    );
    let session = attach(&hub);
    session.dispatch(ClientMessage::Create {
        rows: ROWS,
        cols: COLS,
    });
    let created =
        next_matching(&session, |f| created_pane(f).is_some()).expect("no created message");
    let pane = created_pane(&created).unwrap();

    session.dispatch(ClientMessage::Input {
        pane,
        data: sequences.to_string(),
    });
    // The tracker only knows what it has seen, so the assertions have to wait
    // for the program's own bytes to come back.
    let echoed = next_matching(&session, |f| {
        matches!(f, TerminalFrame::Output { pane: p, data }
            if *p == pane && String::from_utf8_lossy(data).contains(PAINTED))
    });
    assert!(echoed.is_some(), "the pane never produced its output");
    Running {
        hub,
        pane,
        _first: session,
        _dir: dir,
    }
}

#[test]
fn a_reattaching_client_is_told_the_modes_its_pane_is_in() {
    let running = pane_running(ENTER_FULLSCREEN);
    let (hub, pane) = (&running.hub, running.pane);

    let second = attach(hub);
    let replay = String::from_utf8_lossy(&output_for(&second, pane)).to_string();

    // Nothing in the pane's history says these are on any more; the hub does.
    for expected in ["\x1b[?1049h", "\x1b[?1002h", "\x1b[?1006h", "\x1b[?2004h"] {
        assert!(
            replay.contains(expected),
            "the replay must restore {expected:?}, got: {replay:?}"
        );
    }
    hub.stop();
}

#[test]
fn an_alternate_screen_panes_history_is_not_replayed() {
    let running = pane_running(ENTER_FULLSCREEN);
    let (hub, pane) = (&running.hub, running.pane);

    let second = attach(hub);
    let replay = String::from_utf8_lossy(&output_for(&second, pane)).to_string();

    // Those bytes are cell updates against a screen this client does not have.
    // Replaying them paints fragments; the repaint that was asked for instead is
    // what puts a screen there.
    assert!(
        !replay.contains(PAINTED),
        "an alternate-screen pane must not have its paint bytes replayed, got: {replay:?}"
    );
    hub.stop();
}

#[test]
fn a_reattaching_client_makes_an_alternate_screen_program_draw_again() {
    // A program cannot be asked to repaint in the abstract: what it is told is
    // that its size changed. The trap stands in for a program's redraw.
    let running = pane_running("trap 'printf REDR%sEW' WINCH; printf '\\033[?1049hPAINT%sED'\n");
    let (hub, pane) = (&running.hub, running.pane);

    let second = attach(hub);
    let redrew = String::from_utf8_lossy(&output_for(&second, pane)).to_string();

    assert!(
        redrew.contains("REDREW"),
        "attaching must make the pane's program draw its screen again, got: {redrew:?}"
    );
    hub.stop();
}

#[test]
fn a_plain_shells_history_is_still_replayed() {
    // Nothing is wrong with a normal-screen pane's history: it is the text that
    // was on screen, and a reattaching client wants exactly that.
    let running = pane_running("printf 'PAINT%sED'\n");
    let (hub, pane) = (&running.hub, running.pane);

    let second = attach(hub);
    let replay = String::from_utf8_lossy(&output_for(&second, pane)).to_string();

    assert!(
        replay.contains(PAINTED),
        "a normal-screen pane must still be replayed its history, got: {replay:?}"
    );
    assert!(
        replay.contains("\x1b[?1049l"),
        "and be told it is on the normal buffer, got: {replay:?}"
    );
    hub.stop();
}

/// A program sets its title once, with an OSC that is out of the ring within
/// seconds. Left to each client to notice, that made the pane running an agent
/// read `term 1` on every page that arrived after it -- and on the same page
/// after any reconnect, which is a thing that happens.
#[test]
fn a_reattaching_client_is_told_what_the_pane_calls_itself() {
    let running = pane_running(&set_title("agent"));

    assert_eq!(title_on_attach(&running.hub).as_deref(), Some("agent"));
    running.hub.stop();
}
