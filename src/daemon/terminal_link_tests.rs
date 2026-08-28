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
fn a_busy_repository_does_not_spend_another_repositories_budget() {
    let router = TerminalRouter::default();
    for pane in 1..=TERMINAL_DRAIN_MESSAGES as PaneId + 1 {
        router.deliver("busy", created(pane)).unwrap();
    }
    router.deliver("quiet", created(999)).unwrap();

    assert_eq!(router.drain("busy").len(), TERMINAL_DRAIN_MESSAGES);
    let quiet = router.drain("quiet");
    assert_eq!(quiet.len(), 1);
    assert_eq!(pane_of(&quiet[0]), 999);
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
fn control_only_traffic_is_bounded_too() {
    let router = TerminalRouter::with_limits(usize::MAX, 1);
    router.deliver("r1", created(1)).unwrap();

    assert!(
        router.deliver("r1", created(2)).is_err(),
        "control events consume memory even though they carry no PTY bytes"
    );
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

#[test]
#[ignore = "release-only terminal inbox load measurement"]
fn measure_one_mb_per_second_terminal_inbox_drain() {
    const BYTES_PER_SECOND: usize = 1_000_000;
    const FRAMES_PER_SECOND: usize = 60;
    const OUTPUT_CHUNK_BYTES: usize = 1024;
    const FRAME_BUDGET_NS: u128 = 1_000_000_000 / FRAMES_PER_SECOND as u128;

    let router = TerminalRouter::default();
    let mut produced_bytes = 0usize;
    let mut drained_bytes = 0usize;
    let mut peak_queued_bytes = 0usize;
    let mut peak_queued_messages = 0usize;
    let mut drain_calls = 0usize;
    let mut total_drain_ns = 0u128;
    let mut max_drain_ns = 0u128;

    for frame in 0..FRAMES_PER_SECOND {
        let frame_bytes = BYTES_PER_SECOND / FRAMES_PER_SECOND
            + usize::from(frame < BYTES_PER_SECOND % FRAMES_PER_SECOND);
        let mut remaining = frame_bytes;
        while remaining > 0 {
            let chunk_bytes = remaining.min(OUTPUT_CHUNK_BYTES);
            router
                .deliver(
                    "r1",
                    TerminalMessage::Output {
                        pane: 1,
                        data: vec![b'x'; chunk_bytes],
                    },
                )
                .unwrap();
            produced_bytes += chunk_bytes;
            remaining -= chunk_bytes;
        }

        let (queued_bytes, queued_messages) = router.queued_for_test();
        peak_queued_bytes = peak_queued_bytes.max(queued_bytes);
        peak_queued_messages = peak_queued_messages.max(queued_messages);

        let started = std::time::Instant::now();
        let drained = router.drain("r1");
        let elapsed_ns = started.elapsed().as_nanos();
        drain_calls += 1;
        total_drain_ns += elapsed_ns;
        max_drain_ns = max_drain_ns.max(elapsed_ns);
        drained_bytes += drained
            .iter()
            .map(TerminalMessage::output_bytes)
            .sum::<usize>();
    }

    let (remaining_bytes, remaining_messages) = router.queued_for_test();
    let max_frame_budget_basis_points = max_drain_ns * 10_000 / FRAME_BUDGET_NS;
    println!(
        "1 MB/s terminal inbox over {FRAMES_PER_SECOND} simulated frames: \
         peak_queued_bytes={peak_queued_bytes} peak_queued_messages={peak_queued_messages} \
         drain_calls={drain_calls} total_drain_ns={total_drain_ns} \
         max_drain_ns={max_drain_ns} frame_budget_ns={FRAME_BUDGET_NS} \
         max_frame_budget_basis_points={max_frame_budget_basis_points}"
    );

    assert_eq!(produced_bytes, BYTES_PER_SECOND);
    assert_eq!(drained_bytes, BYTES_PER_SECOND);
    assert_eq!(
        peak_queued_bytes,
        BYTES_PER_SECOND.div_ceil(FRAMES_PER_SECOND)
    );
    assert_eq!(peak_queued_messages, 17);
    assert_eq!(drain_calls, FRAMES_PER_SECOND);
    assert_eq!((remaining_bytes, remaining_messages), (0, 0));
}
