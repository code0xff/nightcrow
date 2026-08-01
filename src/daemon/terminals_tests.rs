//! What the relay is allowed to change on its way through.
//!
//! `Created` is the one event whose requester is rewritten into the attach
//! socket's id space; everything else has to arrive at the client exactly as the
//! hub said it, and a recovery report in particular — a rewritten deadline or a
//! dropped detail would be a silent lie about a pane nobody can see.

use super::rewrite_requester;
use crate::daemon::protocol::ServerMessage;
use crate::session::terminal::frame::ServerMessage as HubServerMessage;

const HUB_CLIENT: u64 = 4;
const ATTACHED_CLIENT: u64 = 91;

fn recovery() -> HubServerMessage {
    HubServerMessage::Recovery {
        pane: 6,
        state: "waiting_for_reset".to_string(),
        detail: Some("provider window closed".to_string()),
        deadline_epoch: Some(1_700_000_000),
        attempt: 2,
    }
}

#[test]
fn a_recovery_report_is_relayed_unchanged() {
    assert_eq!(
        rewrite_requester(recovery(), HUB_CLIENT, ATTACHED_CLIENT),
        recovery()
    );
}

#[test]
fn a_recovery_report_survives_the_attach_envelope_intact() {
    // The relay parses a hub control frame and re-encodes it under a repository
    // tag, so the round trip is part of the delivery path rather than a test
    // convenience.
    let tagged = ServerMessage::Terminal {
        repo: "repo-a".to_string(),
        event: rewrite_requester(recovery(), HUB_CLIENT, ATTACHED_CLIENT),
    };
    let json = serde_json::to_string(&tagged).unwrap();
    let back: ServerMessage = serde_json::from_str(&json).unwrap();

    match back {
        ServerMessage::Terminal { repo, event } => {
            assert_eq!(repo, "repo-a");
            assert_eq!(event, recovery());
        }
        other => panic!("the envelope changed shape: {other:?}"),
    }
}

#[test]
fn a_recovery_report_with_no_deadline_or_detail_relays_as_absent_not_as_zero() {
    let bare = HubServerMessage::Recovery {
        pane: 6,
        state: "cancelled".to_string(),
        detail: None,
        deadline_epoch: None,
        attempt: 0,
    };
    let relayed = rewrite_requester(bare.clone(), HUB_CLIENT, ATTACHED_CLIENT);
    assert_eq!(relayed, bare);

    let json = serde_json::to_string(&relayed).unwrap();
    assert!(!json.contains("deadline_epoch"), "{json}");
    assert!(!json.contains("detail"), "{json}");
}
