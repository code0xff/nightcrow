//! A hub whose one startup terminal runs a fixture script on a real PTY, and the
//! readers these tests need. Shared by the wake tests, which park the worker and
//! then wait, and the deadline tests, which wait for a clock instead.
//!
//! The fixture is a configured startup terminal rather than a shell a client opens:
//! a client-opened pane runs an interactive shell, and what that shell says to a
//! keypress is the user's shell's business rather than something a test can assert.

use super::{SHELL_TEST_DEADLINE, attach, created_pane, next_matching};
use crate::backend::PaneId;
use crate::config::{ShellConfig, StartupCommand};
use crate::session::terminal::TerminalHub;
use crate::session::terminal::TerminalSession;
use crate::session::terminal::frame::{ClientMessage, PaneSize, TerminalFrame};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(super) const ROWS: u16 = 24;
pub(super) const COLS: u16 = 80;

/// A hub whose one startup terminal runs `command`.
pub(super) fn fixture_hub(path: &str, command: &str) -> Arc<TerminalHub> {
    TerminalHub::spawn(
        path,
        vec![StartupCommand {
            name: None,
            command: command.to_string(),
            plugin: None,
        }],
        Vec::new(),
        false,
        fixture_shell(),
        Default::default(),
    )
}

/// PowerShell on Windows so the script reaches it as one argument rather than
/// through `cmd`'s quoting; the configured login shell elsewhere.
#[cfg(windows)]
fn fixture_shell() -> ShellConfig {
    ShellConfig {
        program: Some("powershell".to_string()),
        command_args: vec!["-NoProfile".to_string(), "-Command".to_string()],
    }
}

#[cfg(unix)]
fn fixture_shell() -> ShellConfig {
    ShellConfig::default()
}

/// Claim the startup terminal and wait for it to exist.
pub(super) fn start_pane(hub: &Arc<TerminalHub>) -> (TerminalSession, PaneId) {
    let session = attach(hub);
    session.dispatch(ClientMessage::Start {
        sizes: vec![PaneSize {
            rows: ROWS,
            cols: COLS,
        }],
    });
    let created = next_matching(&session, |f| created_pane(f).is_some()).expect("no startup pane");
    let pane = created_pane(&created).expect("a created frame names its pane");
    (session, pane)
}

/// Read `session`'s output for `pane` until `needle` has arrived, answering the
/// cursor-position queries a PowerShell child waits on before it paints.
pub(super) fn read_through(session: &TerminalSession, pane: PaneId, needle: &[u8]) -> Vec<u8> {
    let deadline = Instant::now() + SHELL_TEST_DEADLINE;
    let mut out = Vec::new();
    let mut answered = 0;
    while Instant::now() < deadline {
        if let Some(TerminalFrame::Output { pane: p, data }) =
            session.next_frame(Duration::from_millis(100))
            && p == pane
        {
            out.extend_from_slice(&data);
            answer_cursor_queries(session, pane, &out, &mut answered);
            if out.windows(needle.len()).any(|window| window == needle) {
                return out;
            }
        }
    }
    panic!(
        "the fixture never produced {:?}, got: {:?}",
        String::from_utf8_lossy(needle),
        String::from_utf8_lossy(&out)
    );
}

/// Read whatever a child has already written, answering its cursor-position
/// queries until it falls quiet for `quiet`.
///
/// PowerShell asks where the cursor is before its first write and blocks until a
/// terminal answers — frames alone leave it stuck. Reading it out here lets a
/// test park the worker afterwards, so the frame it then waits for is a reaction
/// to its own input.
pub(super) fn settle_startup(session: &TerminalSession, pane: PaneId, quiet: Duration) -> Vec<u8> {
    let mut out = Vec::new();
    let mut answered = 0;
    while let Some(frame) = session.next_frame(quiet) {
        if let TerminalFrame::Output { pane: p, data } = frame
            && p == pane
        {
            out.extend_from_slice(&data);
            answer_cursor_queries(session, pane, &out, &mut answered);
        }
    }
    out
}

/// Answer every cursor-position query in `output` that no reply has covered yet.
fn answer_cursor_queries(
    session: &TerminalSession,
    pane: PaneId,
    output: &[u8],
    answered: &mut usize,
) {
    const QUERY: &[u8] = b"\x1b[6n";
    const REPLY: &str = "\x1b[1;1R";
    let asked = output
        .windows(QUERY.len())
        .filter(|window| *window == QUERY)
        .count();
    while *answered < asked {
        session.dispatch(ClientMessage::Input {
            pane,
            data: REPLY.to_string(),
        });
        *answered += 1;
    }
}
