//! Contracts that need no hub: the wire encodings, the size clamp, and the two
//! pure helpers the worker leans on.

use crate::web::viewer::limits;
use crate::web::viewer::terminal::frame::{
    ClearKeyFacts, ClientMessage, PaneSize, ServerMessage, decode_output, encode_output,
};
use crate::web::viewer::terminal::hub_helpers::{canonical_order, push_scrollback};
use std::collections::VecDeque;

#[test]
fn output_frames_round_trip_through_the_binary_encoding() {
    // Raw PTY bytes are not always valid UTF-8; the framing must not care.
    let payload = vec![0x1b, b'[', b'0', b'm', 0xff, 0xfe, 0x00];

    let encoded = encode_output(7, &payload);
    let (pane, data) = decode_output(&encoded).unwrap();

    assert_eq!(pane, 7);
    assert_eq!(data, &payload[..]);
}

#[test]
fn decode_output_rejects_a_frame_too_short_to_carry_a_pane_id() {
    assert!(decode_output(&[]).is_none());
    assert!(decode_output(&[1, 2, 3]).is_none());
    assert_eq!(decode_output(&[1, 0, 0, 0]), Some((1, &[][..])));
}

#[test]
fn client_messages_parse_from_the_wire_shape() {
    let create: ClientMessage =
        serde_json::from_str(r#"{"type":"create","rows":24,"cols":80}"#).unwrap();
    assert!(matches!(
        create,
        ClientMessage::Create { rows: 24, cols: 80 }
    ));

    let input: ClientMessage =
        serde_json::from_str(r#"{"type":"input","pane":3,"data":"ls\n"}"#).unwrap();
    assert!(matches!(input, ClientMessage::Input { pane: 3, .. }));

    let reorder: ClientMessage =
        serde_json::from_str(r#"{"type":"reorder","order":[3,1,2]}"#).unwrap();
    assert!(matches!(reorder, ClientMessage::Reorder { order } if order == vec![3, 1, 2]));

    let start: ClientMessage =
        serde_json::from_str(r#"{"type":"start","sizes":[{"rows":40,"cols":120}]}"#).unwrap();
    assert!(
        matches!(start, ClientMessage::Start { sizes } if sizes == vec![PaneSize { rows: 40, cols: 120 }])
    );
    // A client that measured nothing still answers, so the panes open.
    let empty: ClientMessage = serde_json::from_str(r#"{"type":"start","sizes":[]}"#).unwrap();
    assert!(matches!(empty, ClientMessage::Start { sizes } if sizes.is_empty()));

    let cancel: ClientMessage =
        serde_json::from_str(r#"{"type":"cancel_recovery","pane":4}"#).unwrap();
    assert_eq!(cancel, ClientMessage::CancelRecovery { pane: 4 });
    // And out again: the daemon relays what a client sent, so the tag it writes
    // has to be the tag it reads.
    assert_eq!(
        serde_json::to_string(&ClientMessage::CancelRecovery { pane: 4 }).unwrap(),
        r#"{"type":"cancel_recovery","pane":4}"#
    );

    assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"nope"}"#).is_err());
    assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"create"}"#).is_err());
}

#[test]
fn a_zoom_parses_both_with_a_pane_and_without_one() {
    let zoom: ClientMessage = serde_json::from_str(r#"{"type":"zoom","pane":3}"#).unwrap();
    assert_eq!(zoom, ClientMessage::Zoom { pane: Some(3) });

    // Going back to the grid, which is the same message and not a second one.
    let off: ClientMessage = serde_json::from_str(r#"{"type":"zoom","pane":null}"#).unwrap();
    assert_eq!(off, ClientMessage::Zoom { pane: None });

    // An absent `pane` reads as `null` — serde fills an `Option` field that is
    // not there. Pinned rather than tightened: the value it lands on is the
    // harmless one (back to the grid), and a malformed zoom that un-zooms is
    // nothing a client cannot undo.
    let absent: ClientMessage = serde_json::from_str(r#"{"type":"zoom"}"#).unwrap();
    assert_eq!(absent, ClientMessage::Zoom { pane: None });
}

#[test]
fn the_zoomed_announcement_carries_a_null_rather_than_omitting_the_pane() {
    // The client cannot infer "nothing is zoomed" from a missing field — it has
    // to be told, or a pane that stops filling the panel never stops on screen.
    assert_eq!(
        serde_json::to_string(&ServerMessage::Zoomed { pane: Some(2) }).unwrap(),
        r#"{"type":"zoomed","pane":2}"#
    );
    assert_eq!(
        serde_json::to_string(&ServerMessage::Zoomed { pane: None }).unwrap(),
        r#"{"type":"zoomed","pane":null}"#
    );
}

#[test]
fn a_clear_key_report_parses_with_and_without_a_key_event() {
    let keyed: ClientMessage = serde_json::from_str(
        r#"{"type":"clear_key_report","pane":2,"key":{"trusted":false,"repeat":true,"code":"KeyL","since_ms":3}}"#,
    )
    .unwrap();
    assert_eq!(
        keyed,
        ClientMessage::ClearKeyReport {
            pane: 2,
            key: Some(ClearKeyFacts {
                trusted: false,
                repeat: true,
                code: "KeyL".to_string(),
                since_ms: 3,
            }),
        }
    );

    // A byte with nothing behind it — a paste, an input method, or a script
    // writing into the terminal — is the report that matters most.
    let keyless: ClientMessage =
        serde_json::from_str(r#"{"type":"clear_key_report","pane":2,"key":null}"#).unwrap();
    assert_eq!(
        keyless,
        ClientMessage::ClearKeyReport { pane: 2, key: None }
    );
}

#[test]
fn server_messages_serialize_with_a_type_tag() {
    let json = serde_json::to_string(&ServerMessage::Created {
        pane: 2,
        rows: 40,
        cols: 120,
        client: None,
        title: None,
    })
    .unwrap();
    // No requester, no field: the browser reads these already, and a pane
    // nobody asked for must look to it exactly as it did before.
    assert_eq!(json, r#"{"type":"created","pane":2,"rows":40,"cols":120}"#);

    let json = serde_json::to_string(&ServerMessage::Created {
        pane: 2,
        rows: 40,
        cols: 120,
        client: Some(7),
        title: None,
    })
    .unwrap();
    assert_eq!(
        json,
        r#"{"type":"created","pane":2,"rows":40,"cols":120,"client":7}"#
    );

    // And back, because the daemon reads these off a hub session to relay them.
    let created: ServerMessage =
        serde_json::from_str(r#"{"type":"created","pane":1,"rows":2,"cols":3}"#).unwrap();
    assert!(matches!(
        created,
        ServerMessage::Created { client: None, .. }
    ));

    let json = serde_json::to_string(&ServerMessage::Reordered { order: vec![2, 1] }).unwrap();
    assert_eq!(json, r#"{"type":"reordered","order":[2,1]}"#);

    let json = serde_json::to_string(&ServerMessage::Pending { count: 2 }).unwrap();
    assert_eq!(json, r#"{"type":"pending","count":2}"#);
}

#[test]
fn a_recovery_report_has_a_fixed_wire_shape() {
    let json = serde_json::to_string(&ServerMessage::Recovery {
        pane: 6,
        state: "waiting_for_reset".to_string(),
        detail: Some("provider window closed".to_string()),
        deadline_epoch: Some(1_700_000_000),
        attempt: 2,
    })
    .unwrap();
    assert_eq!(
        json,
        r#"{"type":"recovery","pane":6,"state":"waiting_for_reset","detail":"provider window closed","deadline_epoch":1700000000,"attempt":2}"#
    );

    // Absent, not null and not zero: a client must be able to tell "no deadline"
    // from "the epoch", and rendering a wrong wall-clock time reads as fact.
    let json = serde_json::to_string(&ServerMessage::Recovery {
        pane: 6,
        state: "cancelled".to_string(),
        detail: None,
        deadline_epoch: None,
        attempt: 0,
    })
    .unwrap();
    assert_eq!(
        json,
        r#"{"type":"recovery","pane":6,"state":"cancelled","attempt":0}"#
    );

    // And back, because the daemon reads these off a hub session to relay them.
    let parsed: ServerMessage = serde_json::from_str(
        r#"{"type":"recovery","pane":6,"state":"backoff","deadline_epoch":-1,"attempt":9}"#,
    )
    .unwrap();
    assert_eq!(
        parsed,
        ServerMessage::Recovery {
            pane: 6,
            state: "backoff".to_string(),
            detail: None,
            deadline_epoch: Some(-1),
            attempt: 9,
        }
    );
}

#[test]
fn a_pane_size_is_clamped_into_the_bounds_a_pty_can_use() {
    // These arrive from the client's own measurement, so they are input from
    // outside. Zero gives the child a terminal it cannot draw in and can fail
    // `openpty`; the far end asks a full-screen program for a screen buffer of
    // rows * cells.
    assert_eq!(
        PaneSize { rows: 0, cols: 0 }.clamped(),
        PaneSize {
            rows: limits::MIN_PANE_DIMENSION,
            cols: limits::MIN_PANE_DIMENSION
        }
    );
    assert_eq!(
        PaneSize {
            rows: u16::MAX,
            cols: u16::MAX
        }
        .clamped(),
        PaneSize {
            rows: limits::MAX_PANE_ROWS,
            cols: limits::MAX_PANE_COLS
        }
    );
    // A real display passes through untouched.
    let real = PaneSize {
        rows: 48,
        cols: 210,
    };
    assert_eq!(real.clamped(), real);
}

#[test]
fn canonical_order_reconciles_a_request_against_the_live_panes() {
    // A full permutation is honored verbatim.
    assert_eq!(canonical_order(&[1, 2, 3], &[3, 1, 2]), vec![3, 1, 2]);
    // A partial request moves the named panes; the rest keep their order.
    assert_eq!(canonical_order(&[1, 2, 3], &[3]), vec![3, 1, 2]);
    // An id that is no longer live (closed in a race) is dropped.
    assert_eq!(canonical_order(&[1, 2], &[9, 2, 1]), vec![2, 1]);
    // A repeated id is taken once, keeping the result a permutation.
    assert_eq!(canonical_order(&[1, 2], &[2, 2, 1]), vec![2, 1]);
    // An empty request leaves the order untouched.
    assert_eq!(canonical_order(&[1, 2], &[]), vec![1, 2]);
}

#[test]
fn scrollback_is_bounded_and_keeps_the_most_recent_bytes() {
    let cap = limits::MAX_TERMINAL_SCROLLBACK_BYTES;
    let mut buf = VecDeque::new();
    for _ in 0..(cap / 1000 + 5) {
        push_scrollback(&mut buf, &vec![b'x'; 1000]);
    }
    assert_eq!(buf.len(), cap, "scrollback must be capped");

    // The tail is what restores the visible screen, so the newest bytes must
    // survive eviction.
    push_scrollback(&mut buf, b"TAIL");
    assert_eq!(buf.len(), cap);
    let contents: Vec<u8> = buf.iter().copied().collect();
    assert!(contents.ends_with(b"TAIL"), "newest bytes must be retained");
}
