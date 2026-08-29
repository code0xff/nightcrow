use super::*;

#[test]
fn adjacent_tiny_outputs_for_one_repository_pane_coalesce_past_the_message_limit() {
    let router = TerminalRouter::with_limits(usize::MAX, TERMINAL_INBOX_MESSAGES);
    let chunk_count = TERMINAL_INBOX_MESSAGES + 1;
    let expected: Vec<_> = (0..chunk_count).map(|index| (index % 251) as u8).collect();

    for byte in &expected {
        router
            .deliver(
                "r1",
                TerminalMessage::Output {
                    pane: 1,
                    data: vec![*byte],
                },
            )
            .unwrap();
    }

    assert_eq!(router.queued_for_test(), (chunk_count, 1));
    assert!(matches!(
        router.drain("r1").as_slice(),
        [TerminalMessage::Output { pane: 1, data }] if data == &expected
    ));
}

#[test]
fn per_repository_output_coalescing_stops_at_pane_and_event_boundaries() {
    let router = TerminalRouter::default();
    for (repo, pane, data) in [("r1", 1, b"a".as_slice()), ("r2", 1, b"b".as_slice())] {
        router
            .deliver(
                repo,
                TerminalMessage::Output {
                    pane,
                    data: data.to_vec(),
                },
            )
            .unwrap();
    }
    // A different repository's arrival does not break r1's own adjacent run.
    router
        .deliver(
            "r1",
            TerminalMessage::Output {
                pane: 1,
                data: b"c".to_vec(),
            },
        )
        .unwrap();
    router.deliver("r1", created(2)).unwrap();
    router
        .deliver(
            "r1",
            TerminalMessage::Output {
                pane: 1,
                data: b"d".to_vec(),
            },
        )
        .unwrap();
    router
        .deliver(
            "r1",
            TerminalMessage::Output {
                pane: 2,
                data: b"e".to_vec(),
            },
        )
        .unwrap();
    router
        .deliver(
            "r1",
            TerminalMessage::Output {
                pane: 2,
                data: b"f".to_vec(),
            },
        )
        .unwrap();

    let r1 = router.drain("r1");
    assert!(matches!(
        r1.as_slice(),
        [
            TerminalMessage::Output { pane: 1, data: before_event },
            TerminalMessage::Event(HubServerMessage::Created { pane: 2, .. }),
            TerminalMessage::Output { pane: 1, data: after_event },
            TerminalMessage::Output { pane: 2, data: last },
        ] if before_event == b"ac" && after_event == b"d" && last == b"ef"
    ));
    assert!(matches!(
        router.drain("r2").as_slice(),
        [TerminalMessage::Output { pane: 1, data }] if data == b"b"
    ));
}

#[test]
fn per_repository_coalescing_splits_after_exactly_one_drain_chunk() {
    let router = TerminalRouter::default();
    router
        .deliver(
            "r1",
            TerminalMessage::Output {
                pane: 1,
                data: vec![b'a'; TERMINAL_DRAIN_BYTES - 1],
            },
        )
        .unwrap();
    router
        .deliver(
            "r1",
            TerminalMessage::Output {
                pane: 1,
                data: vec![b'b'],
            },
        )
        .unwrap();
    router
        .deliver(
            "r1",
            TerminalMessage::Output {
                pane: 1,
                data: vec![b'c'],
            },
        )
        .unwrap();

    assert_eq!(
        router.queued_for_test(),
        (TERMINAL_DRAIN_BYTES + 1, 2),
        "the first two chunks fit exactly, while the third starts a new message"
    );
    let first = router.drain("r1");
    assert!(matches!(
        first.as_slice(),
        [TerminalMessage::Output { pane: 1, data }]
            if data.len() == TERMINAL_DRAIN_BYTES
                && data[..TERMINAL_DRAIN_BYTES - 1].iter().all(|&byte| byte == b'a')
                && data[TERMINAL_DRAIN_BYTES - 1] == b'b'
    ));
    assert!(matches!(
        router.drain("r1").as_slice(),
        [TerminalMessage::Output { pane: 1, data }] if data == b"c"
    ));
}
