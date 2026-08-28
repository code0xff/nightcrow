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

#[test]
fn traffic_that_arrives_before_a_repository_has_a_reader_is_kept() {
    // The daemon subscribes a client to every open repository the moment it
    // connects, so a pane and its scrollback can be on the wire before the
    // client has been told the repository exists. The replay happens once —
    // dropping it would lose those panes for good.
    let router = TerminalRouter::default();

    router.deliver("r1", created(1)).unwrap();
    router
        .deliver(
            "r1",
            TerminalMessage::Output {
                pane: 1,
                data: b"prompt$ ".to_vec(),
            },
        )
        .unwrap();

    let inbox = router.drain("r1");
    assert_eq!(inbox.len(), 2);
    assert_eq!(pane_of(&inbox[0]), 1);
}

#[test]
fn each_repository_drains_only_its_own_traffic() {
    let router = TerminalRouter::default();
    router.deliver("r1", created(1)).unwrap();
    router.deliver("r2", created(2)).unwrap();

    let first = router.drain("r1");
    assert_eq!(first.len(), 1);
    assert_eq!(pane_of(&first[0]), 1);
    let second = router.drain("r2");
    assert_eq!(second.len(), 1);
    assert_eq!(pane_of(&second[0]), 2);
}

#[test]
fn a_drained_inbox_is_empty_until_more_arrives() {
    let router = TerminalRouter::default();
    router.deliver("r1", created(1)).unwrap();

    assert_eq!(router.drain("r1").len(), 1);
    assert!(router.drain("r1").is_empty());
    assert!(
        router.drain("never-heard-of-it").is_empty(),
        "and an unknown repository is empty rather than a panic"
    );
}

#[test]
fn closing_a_repository_drops_what_was_queued_for_it() {
    // Its backend went with its tab, so nothing will ever drain this.
    let router = TerminalRouter::default();
    router.deliver("r1", created(1)).unwrap();
    router.deliver("gone", created(9)).unwrap();

    router.retain(&["r1".to_string()]);

    assert_eq!(router.drain("r1").len(), 1);
    assert!(router.drain("gone").is_empty());
}

#[test]
fn a_drain_takes_a_bounded_fifo_prefix() {
    let router = TerminalRouter::default();
    for pane in 1..=TERMINAL_DRAIN_MESSAGES as PaneId + 1 {
        router.deliver("r1", created(pane)).unwrap();
    }

    let first = router.drain("r1");
    assert_eq!(first.len(), TERMINAL_DRAIN_MESSAGES);
    assert_eq!(pane_of(&first[0]), 1);
    assert_eq!(
        pane_of(first.last().expect("the bounded batch is not empty")),
        TERMINAL_DRAIN_MESSAGES as PaneId
    );
    let second = router.drain("r1");
    assert_eq!(second.len(), 1);
    assert_eq!(pane_of(&second[0]), TERMINAL_DRAIN_MESSAGES as PaneId + 1);
}

#[test]
fn an_oversized_head_advances_but_exit_waits_behind_its_output() {
    let router = TerminalRouter::default();
    let output = vec![b'x'; TERMINAL_DRAIN_BYTES + 1];
    router
        .deliver(
            "r1",
            TerminalMessage::Output {
                pane: 1,
                data: output.clone(),
            },
        )
        .unwrap();
    router
        .deliver(
            "r1",
            TerminalMessage::Event(HubServerMessage::Exited { pane: 1 }),
        )
        .unwrap();

    let first = router.drain("r1");
    assert!(matches!(
        first.as_slice(),
        [TerminalMessage::Output { pane: 1, data }] if data == &output
    ));
    assert!(matches!(
        router.drain("r1").as_slice(),
        [TerminalMessage::Event(HubServerMessage::Exited { pane: 1 })]
    ));
}

#[test]
fn crossing_the_byte_ceiling_poison_disconnects_instead_of_making_a_hole() {
    let router = TerminalRouter::with_byte_limit(4);
    router
        .deliver(
            "r1",
            TerminalMessage::Output {
                pane: 1,
                data: b"abcd".to_vec(),
            },
        )
        .unwrap();

    let overflow = router.deliver(
        "r1",
        TerminalMessage::Output {
            pane: 1,
            data: b"e".to_vec(),
        },
    );
    assert!(overflow.is_err());
    assert!(
        router.deliver("r1", created(2)).is_err(),
        "after one rejected frame, accepting later traffic would create a stream hole"
    );

    let kept = router.drain("r1");
    assert!(matches!(
        kept.as_slice(),
        [TerminalMessage::Output { data, .. }] if data == b"abcd"
    ));
}

#[test]
fn closing_a_repository_returns_its_share_of_the_byte_allowance() {
    let router = TerminalRouter::with_byte_limit(4);
    router
        .deliver(
            "gone",
            TerminalMessage::Output {
                pane: 1,
                data: b"abcd".to_vec(),
            },
        )
        .unwrap();

    router.retain(&[]);

    router
        .deliver(
            "open",
            TerminalMessage::Output {
                pane: 2,
                data: b"wxyz".to_vec(),
            },
        )
        .expect("discarding an unopened repository frees its queued bytes");
}
