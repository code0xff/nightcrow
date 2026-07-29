mod behavior;
mod scrollback_depth;
mod size_owner;

use crate::backend::PaneId;
use crate::web::viewer::limits;
use crate::web::viewer::terminal::frame::{
    ClientMessage, PaneSize, ServerMessage, TerminalFrame, decode_output, encode_output,
};
use crate::web::viewer::terminal::hub_helpers::{canonical_order, push_scrollback};
use std::collections::VecDeque;
use std::thread;
use std::time::{Duration, Instant};

use super::TerminalSession;

/// Deadline for the real-shell tests below. `connect` spawns the user's
/// actual `$SHELL` (an interactive zsh sources its full rc chain), and
/// cargo runs tests in parallel, so several shells initialize at once — a
/// tighter budget was measurably flaky under load. A generous bound only
/// delays the failure verdict; passing runs still finish the instant the
/// frame arrives. Mirrors `backend::pty::tests::PTY_TEST_DEADLINE`.
pub(super) const SHELL_TEST_DEADLINE: Duration = Duration::from_secs(15);

pub(super) fn wait_for<T>(mut take: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + SHELL_TEST_DEADLINE;
    while Instant::now() < deadline {
        if let Some(value) = take() {
            return Some(value);
        }
        thread::sleep(Duration::from_millis(10));
    }
    None
}

/// Pull frames until one satisfies `want`, ignoring the rest.
pub(super) fn next_matching(
    session: &TerminalSession,
    mut want: impl FnMut(&TerminalFrame) -> bool,
) -> Option<TerminalFrame> {
    wait_for(|| {
        session
            .next_frame(Duration::from_millis(50))
            .filter(|f| want(f))
    })
}

pub(super) fn created_pane(frame: &TerminalFrame) -> Option<PaneId> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] == "created" {
        return value["pane"].as_u64().map(|n| n as PaneId);
    }
    None
}

/// The pane count a `pending` frame offers for sizing.
pub(super) fn pending_count(frame: &TerminalFrame) -> Option<usize> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "pending" {
        return None;
    }
    value["count"].as_u64().map(|n| n as usize)
}

/// The name a `created` frame gives its pane, if the session named it.
pub(super) fn created_title(frame: &TerminalFrame) -> Option<String> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "created" {
        return None;
    }
    value["title"].as_str().map(str::to_string)
}

/// The size a `created` frame reports for its pane.
pub(super) fn created_size(frame: &TerminalFrame) -> Option<(u16, u16)> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "created" {
        return None;
    }
    Some((
        value["rows"].as_u64()? as u16,
        value["cols"].as_u64()? as u16,
    ))
}

pub(super) fn reordered_order(frame: &TerminalFrame) -> Option<Vec<PaneId>> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "reordered" {
        return None;
    }
    Some(
        value["order"]
            .as_array()?
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as PaneId))
            .collect(),
    )
}

/// Collect the ids of the first `n` distinct panes announced to `session`,
/// in the order the `created` frames arrive.
pub(super) fn collect_created(session: &TerminalSession, n: usize) -> Vec<PaneId> {
    let mut ids = Vec::new();
    while ids.len() < n {
        let created =
            next_matching(session, |f| created_pane(f).is_some()).expect("no created message");
        let id = created_pane(&created).unwrap();
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

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

    assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"nope"}"#).is_err());
    assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"create"}"#).is_err());
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
