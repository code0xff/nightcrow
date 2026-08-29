use super::*;
use crate::session::terminal::frame::ServerMessage as HubServerMessage;

fn created(pane: PaneId) -> TerminalMessage {
    TerminalMessage::Event(HubServerMessage::Created {
        pane,
        rows: 24,
        cols: 80,
        client: None,
        title: None,
    })
}

fn pane_of(message: &TerminalMessage) -> PaneId {
    match message {
        TerminalMessage::Event(HubServerMessage::Created { pane, .. }) => *pane,
        TerminalMessage::Output { pane, .. } => *pane,
        other => panic!("expected a pane message, got {other:?}"),
    }
}

mod coalescing;
mod draining;
mod measurement;
mod overflow;
mod routing;
