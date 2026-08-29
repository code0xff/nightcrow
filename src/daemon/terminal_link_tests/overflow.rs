use super::*;

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
