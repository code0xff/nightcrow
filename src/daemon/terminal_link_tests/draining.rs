use super::*;

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
fn the_byte_budget_leaves_the_next_output_for_the_next_drain() {
    let router = TerminalRouter::default();
    let chunk = vec![b'x'; TERMINAL_DRAIN_BYTES / 2];
    for pane in 1..=3 {
        router
            .deliver(
                "r1",
                TerminalMessage::Output {
                    pane,
                    data: chunk.clone(),
                },
            )
            .unwrap();
    }

    let first = router.drain("r1");
    assert_eq!(first.len(), 2);
    assert_eq!(pane_of(&first[0]), 1);
    assert_eq!(pane_of(&first[1]), 2);
    let second = router.drain("r1");
    assert_eq!(second.len(), 1);
    assert_eq!(pane_of(&second[0]), 3);
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
