//! Reading terminal traffic off an attach socket.
//!
//! Split from `harness.rs`, which is about the session — the repository set,
//! which project is in front, the accent. A pane's frames are the other half of
//! what one socket carries, and telling them apart is most of what these do.

use super::harness::{Client, OUTPUT_TIMEOUT, READ_TIMEOUT, decodes_to_terminal};
use crate::backend::PaneId;
use crate::daemon::frame::FrameKind;
use crate::daemon::protocol::{ServerMessage, TerminalOutput};
use crate::session::terminal::frame::ServerMessage as HubServerMessage;

impl Client {
    /// The next pane the daemon reports, and who it says asked for it.
    pub(super) fn next_created(&mut self) -> (PaneId, Option<u64>) {
        for _ in 0..64 {
            if let (
                _,
                HubServerMessage::Created {
                    pane,
                    client: requester,
                    ..
                },
            ) = self.next_terminal_event()
            {
                return (pane, requester);
            }
        }
        panic!("no pane was created");
    }

    /// The next terminal event for any repository, stepping over the tab list.
    pub(super) fn next_terminal_event(&mut self) -> (String, HubServerMessage) {
        let frame = self
            .find(READ_TIMEOUT, |frame| {
                frame.kind == FrameKind::Control && decodes_to_terminal(frame)
            })
            .expect("no terminal event arrived");
        match serde_json::from_slice(&frame.payload) {
            Ok(ServerMessage::Terminal { repo, event }) => (repo, event),
            other => panic!("expected a terminal event, got {other:?}"),
        }
    }

    /// The next chunk of pane output.
    pub(super) fn next_output(&mut self) -> TerminalOutput {
        let frame = self
            .find(OUTPUT_TIMEOUT, |frame| frame.kind == FrameKind::Terminal)
            .expect("no pane output arrived");
        TerminalOutput::decode(&frame.payload).expect("a well-formed output frame")
    }
}
