use super::*;
use crate::session::terminal::frame::TerminalFrame;
use std::sync::mpsc::sync_channel;

fn viewer(name: &str) -> ViewerId {
    ViewerId::Browser(name.to_string())
}

#[test]
fn fallback_prefers_recent_activity_over_a_passive_arrival() {
    let ownership = SizeOwnership::new();
    let now = Instant::now();
    let (a_tx, _a_rx) = sync_channel::<TerminalFrame>(16);
    let (b_tx, _b_rx) = sync_channel::<TerminalFrame>(16);
    let (c_tx, _c_rx) = sync_channel::<TerminalFrame>(16);
    let a = ownership.join(viewer("a"), true, a_tx, now);
    let b = ownership.join(viewer("b"), true, b_tx, now);
    let _c = ownership.join(viewer("c"), true, c_tx, now);

    ownership.claim(b.connection, now);
    ownership.claim(a.connection, now);
    ownership.leave(a.connection, now);
    ownership.settle(now + RELEASE_GRACE);

    assert_eq!(
        ownership.owner(),
        Some(viewer("b")),
        "the passive viewer c must not outrank b's last interaction"
    );
}

#[test]
fn a_queued_resize_generation_does_not_survive_an_aba_owner_change() {
    let ownership = SizeOwnership::new();
    let now = Instant::now();
    let (a_tx, _a_rx) = sync_channel::<TerminalFrame>(16);
    let (b_tx, _b_rx) = sync_channel::<TerminalFrame>(16);
    let a = ownership.join(viewer("a"), true, a_tx, now);
    let b = ownership.join(viewer("b"), true, b_tx, now);
    let queued_generation = ownership.owner_generation(a.connection).unwrap();

    ownership.claim(b.connection, now);
    ownership.claim(a.connection, now);

    let mut applied = false;
    assert_eq!(
        ownership.with_owner_generation(a.connection, queued_generation, || applied = true),
        None
    );
    assert!(!applied);
    assert!(
        ownership
            .with_owner_generation(
                a.connection,
                ownership.owner_generation(a.connection).unwrap(),
                || {
                    applied = true;
                }
            )
            .is_some()
    );
    assert!(applied);
}

#[test]
fn a_full_owner_queue_does_not_block_and_a_repeat_claim_reannounces_state() {
    let ownership = SizeOwnership::new();
    let now = Instant::now();
    let (a_tx, _a_rx) = sync_channel::<TerminalFrame>(4);
    let (b_tx, b_rx) = sync_channel::<TerminalFrame>(1);
    ownership.join(viewer("a"), true, a_tx, now);
    let b = ownership.join(viewer("b"), true, b_tx.clone(), now);
    assert!(b_rx.try_recv().is_ok(), "the join verdict fills the queue");
    b_tx.try_send(TerminalFrame::Control("pending".into()))
        .unwrap();

    ownership.claim(b.connection, now);
    assert!(ownership.owns(b.connection));
    let generation = ownership
        .owner_generation(b.connection)
        .unwrap()
        .to_string();
    assert!(
        b_rx.try_recv().is_ok(),
        "the pending frame remains in front"
    );
    assert!(b_rx.try_recv().is_err(), "the full queue drops the verdict");

    ownership.claim(b.connection, now);

    let TerminalFrame::Control(json) = b_rx.try_recv().unwrap() else {
        panic!("expected the current owner verdict");
    };
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["type"], "size_owner");
    assert_eq!(value["owned"], true);
    assert_eq!(value["generation"], generation);
}

#[test]
fn a_returning_owner_can_detect_a_missed_loss_from_its_new_generation() {
    let ownership = SizeOwnership::new();
    let now = Instant::now();
    let (a_tx, a_rx) = sync_channel::<TerminalFrame>(1);
    let (b_tx, b_rx) = sync_channel::<TerminalFrame>(4);
    let a = ownership.join(viewer("a"), true, a_tx.clone(), now);
    let initial = owner_verdict(&a_rx);
    let b = ownership.join(viewer("b"), true, b_tx, now);
    let _ = owner_verdict(&b_rx);

    a_tx.try_send(TerminalFrame::Control("backlog".into()))
        .unwrap();
    ownership.claim(b.connection, now);
    let _ = a_rx.try_recv().unwrap();
    ownership.claim(a.connection, now);

    let returned = owner_verdict(&a_rx);
    assert!(returned.0);
    assert_ne!(returned.1, initial.1);
}

fn owner_verdict(rx: &std::sync::mpsc::Receiver<TerminalFrame>) -> (bool, String) {
    let TerminalFrame::Control(json) = rx.try_recv().unwrap() else {
        panic!("expected an ownership control frame");
    };
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    (
        value["owned"].as_bool().unwrap(),
        value["generation"].as_str().unwrap().to_string(),
    )
}
