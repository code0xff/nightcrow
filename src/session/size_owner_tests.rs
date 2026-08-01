//! The rules a session's size ownership keeps.
//!
//! Asserted through what a client is *told*, not only through the stored owner:
//! the message is what makes a client re-fit its panes, so an ownership change
//! nobody was told about would not move a single PTY.

use super::*;
use crate::session::terminal::frame::TerminalFrame;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

fn viewer(name: &str) -> ViewerId {
    ViewerId::Browser(name.to_string())
}

fn channel() -> (SyncSender<TerminalFrame>, Receiver<TerminalFrame>) {
    sync_channel(16)
}

/// Every `size_owner` verdict queued for a connection, oldest first.
fn verdicts(rx: &Receiver<TerminalFrame>) -> Vec<bool> {
    let mut seen = Vec::new();
    while let Ok(TerminalFrame::Control(json)) = rx.try_recv() {
        let value: serde_json::Value = serde_json::from_str(&json).expect("control frame is JSON");
        if value["type"] == "size_owner" {
            seen.push(value["owned"].as_bool().expect("owned is a bool"));
        }
    }
    seen
}

#[test]
fn a_viewer_that_says_it_just_arrived_takes_the_sizing() {
    let ownership = SizeOwnership::new();
    let (tx, rx) = channel();

    let registration = ownership.join(viewer("a"), true, tx, Instant::now());

    assert!(registration.owned);
    assert_eq!(ownership.owner(), Some(viewer("a")));
    assert_eq!(verdicts(&rx), [true]);
}

#[test]
fn a_connection_that_is_not_arriving_leaves_the_sizing_where_it_is() {
    let ownership = SizeOwnership::new();
    let now = Instant::now();
    let (a_tx, a_rx) = channel();
    let (b_tx, b_rx) = channel();
    ownership.join(viewer("a"), true, a_tx, now);
    let _ = verdicts(&a_rx);

    // The second page is not newly arrived — a repository switch, or a socket
    // that reconnected. It must not take a screen it never claimed.
    let registration = ownership.join(viewer("b"), false, b_tx, now);

    assert!(!registration.owned);
    assert_eq!(ownership.owner(), Some(viewer("a")));
    assert_eq!(verdicts(&b_rx), [false]);
    assert!(verdicts(&a_rx).is_empty(), "the owner was not disturbed");
}

/// The bug this module exists for: switching repositories closes one socket and
/// opens another, on *every* attached page at once, because which repository is
/// in front is shared. Read as arrivals, that made the sizing fall to whichever
/// handshake finished last.
#[test]
fn switching_repositories_does_not_move_the_sizing_between_viewers() {
    let ownership = SizeOwnership::new();
    let now = Instant::now();
    let (a_tx, a_rx) = channel();
    let (b_tx, b_rx) = channel();
    let a = ownership.join(viewer("a"), true, a_tx.clone(), now);
    let b = ownership.join(viewer("b"), true, b_tx.clone(), now);
    // The second page to open owns it, as `window-size latest` says.
    assert!(b.owned);
    let _ = (verdicts(&a_rx), verdicts(&b_rx));

    // Both pages move to another repository: old socket closed, new one opened,
    // in an order neither of them controls.
    ownership.leave(a.connection, now);
    ownership.leave(b.connection, now);
    let a = ownership.join(viewer("a"), false, a_tx, now);
    let b = ownership.join(viewer("b"), false, b_tx, now);

    assert!(!a.owned);
    assert!(b.owned, "the sizing stayed with the page that had it");
    assert_eq!(ownership.owner(), Some(viewer("b")));
}

#[test]
fn a_viewer_keeps_the_sizing_across_a_gap_between_its_connections() {
    let ownership = SizeOwnership::new();
    let now = Instant::now();
    let (a_tx, _a_rx) = channel();
    let (b_tx, b_rx) = channel();
    let a = ownership.join(viewer("a"), true, a_tx.clone(), now);
    ownership.join(viewer("b"), false, b_tx, now);
    let _ = verdicts(&b_rx);

    ownership.leave(a.connection, now);
    // Inside the grace, so the owner has not gone anywhere yet.
    ownership.settle(now + RELEASE_GRACE / 2);
    assert_eq!(ownership.owner(), Some(viewer("a")));
    assert!(verdicts(&b_rx).is_empty(), "nobody was told to re-fit");

    let back = ownership.join(viewer("a"), false, a_tx, now + RELEASE_GRACE / 2);
    assert!(back.owned);
    assert_eq!(ownership.owner(), Some(viewer("a")));
}

#[test]
fn a_viewer_that_stays_gone_hands_the_sizing_to_the_newest_one_left() {
    let ownership = SizeOwnership::new();
    let now = Instant::now();
    let (a_tx, _a_rx) = channel();
    let (b_tx, b_rx) = channel();
    let a = ownership.join(viewer("a"), true, a_tx, now);
    ownership.join(viewer("b"), false, b_tx, now);
    let _ = verdicts(&b_rx);

    ownership.leave(a.connection, now);
    ownership.settle(now + RELEASE_GRACE);

    assert_eq!(ownership.owner(), Some(viewer("b")));
    assert_eq!(verdicts(&b_rx), [true], "the heir has to know to re-fit");
}

#[test]
fn the_last_viewer_leaving_leaves_the_sizing_unowned() {
    let ownership = SizeOwnership::new();
    let now = Instant::now();
    let (tx, _rx) = channel();
    let only = ownership.join(viewer("a"), true, tx, now);

    ownership.leave(only.connection, now);
    ownership.settle(now + RELEASE_GRACE);

    assert_eq!(
        ownership.owner(),
        None,
        "no client to fit; panes keep sizes"
    );
}

/// One viewer, several connections — an attached TUI subscribes to every open
/// repository at once. Those are one screen, not one per repository.
#[test]
fn a_viewers_further_connections_neither_claim_nor_release() {
    let ownership = SizeOwnership::new();
    let now = Instant::now();
    let (a_tx, _a_rx) = channel();
    let (first_tx, _first_rx) = channel();
    let (second_tx, _second_rx) = channel();
    ownership.join(viewer("a"), true, a_tx, now);

    let tui = ViewerId::Attached(7);
    let first = ownership.join(tui.clone(), true, first_tx, now);
    // Its second repository. Arriving again must not re-take what it has.
    let second = ownership.join(tui.clone(), true, second_tx, now);
    assert_eq!(ownership.owner(), Some(tui.clone()));

    // One repository closes. The viewer is still here through the other.
    ownership.leave(second.connection, now);
    ownership.settle(now + RELEASE_GRACE);
    assert_eq!(
        ownership.owner(),
        Some(tui),
        "a viewer with a connection left has not gone"
    );

    ownership.leave(first.connection, now);
    ownership.settle(now + RELEASE_GRACE * 2);
    assert_eq!(ownership.owner(), Some(viewer("a")));
}

#[test]
fn a_client_can_take_the_sizing_back_on_request() {
    let ownership = SizeOwnership::new();
    let now = Instant::now();
    let (a_tx, a_rx) = channel();
    let (b_tx, b_rx) = channel();
    let a = ownership.join(viewer("a"), true, a_tx, now);
    let b = ownership.join(viewer("b"), true, b_tx, now);
    assert!(b.owned);
    let _ = (verdicts(&a_rx), verdicts(&b_rx));

    ownership.claim(a.connection, now);

    assert_eq!(ownership.owner(), Some(viewer("a")));
    assert!(ownership.owns(a.connection));
    assert!(!ownership.owns(b.connection));
    assert_eq!(verdicts(&a_rx), [true]);
    assert_eq!(verdicts(&b_rx), [false], "the displaced one stops resizing");
}

#[test]
fn claiming_what_this_viewer_already_owns_says_nothing() {
    let ownership = SizeOwnership::new();
    let now = Instant::now();
    let (tx, rx) = channel();
    let only = ownership.join(viewer("a"), true, tx, now);
    let _ = verdicts(&rx);

    ownership.claim(only.connection, now);

    assert!(
        verdicts(&rx).is_empty(),
        "a client is not told what it already knows"
    );
}

#[test]
fn a_claim_from_a_connection_that_is_gone_is_dropped() {
    let ownership = SizeOwnership::new();
    let now = Instant::now();
    let (a_tx, _a_rx) = channel();
    let (b_tx, _b_rx) = channel();
    let a = ownership.join(viewer("a"), true, a_tx, now);
    let b = ownership.join(viewer("b"), true, b_tx, now);

    ownership.leave(a.connection, now);
    ownership.claim(a.connection, now);

    assert_eq!(
        ownership.owner(),
        Some(viewer("b")),
        "a request arriving after its connection went cannot move the sizing"
    );
    assert!(ownership.owns(b.connection));
}

#[test]
fn a_connection_that_is_gone_owns_nothing() {
    let ownership = SizeOwnership::new();
    let now = Instant::now();
    let (tx, _rx) = channel();
    let only = ownership.join(viewer("a"), true, tx, now);
    assert!(ownership.owns(only.connection));

    ownership.leave(only.connection, now);

    assert!(
        !ownership.owns(only.connection),
        "a resize from a connection that has gone must not be honoured"
    );
}
