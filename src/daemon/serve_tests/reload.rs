//! Asking the daemon to re-read its config file.
//!
//! The applying itself is pinned in `web::viewer::reload`, against a temp file.
//! What is left here is the request path: who is answered.

use super::harness::*;
use crate::daemon::protocol::{ClientMessage, ServerMessage};

/// A reload is the one request whose answer is *not* broadcast.
///
/// Nothing it does shows up in what the other clients are looking at — the
/// startup list only reaches repositories opened later, and a plugin being
/// replaced is a child process nobody is watching — so a notice on every
/// attached terminal would be about something they did not do and cannot see.
///
/// Started with no repositories, so this exercises the request path without
/// fanning out to any hub.
#[test]
fn a_reload_answer_reaches_the_client_that_asked_and_no_other() {
    const QUIET: std::time::Duration = std::time::Duration::from_millis(500);
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut asker = Client::attach(daemon.path());
    let mut bystander = Client::attach(daemon.path());

    // Applied or refused depending on whether this machine has a config file;
    // either way it is an answer, and either way it is this client's.
    let answer = asker.ask(ClientMessage::ReloadConfig);
    assert!(
        matches!(
            answer,
            ServerMessage::Reloaded { .. } | ServerMessage::Error { .. }
        ),
        "expected a reload answer, got {answer:?}"
    );

    assert!(
        !bystander.hears_a_reload_answer_within(QUIET),
        "a reload answer must not reach a client that did not ask for one"
    );
}
