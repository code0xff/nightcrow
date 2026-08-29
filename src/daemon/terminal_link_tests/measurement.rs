use super::*;

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
    assert_eq!(peak_queued_messages, 1);
    assert_eq!(drain_calls, FRAMES_PER_SECOND);
    assert_eq!((remaining_bytes, remaining_messages), (0, 0));
}
