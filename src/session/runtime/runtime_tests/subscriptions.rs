use super::*;

#[test]
fn a_subscriber_is_seeded_with_the_current_status() {
    let (runtime, tx) = test_runtime();
    tx.send(SnapshotMsg::Ok(snapshot("main", 1), Default::default()))
        .unwrap();
    assert!(wait_for(|| runtime.latest().is_some()));

    let subscription = runtime.subscribe();
    assert!(
        subscription
            .next_update(Duration::from_millis(50))
            .is_some(),
        "a fresh subscriber must replay the latest"
    );
    runtime.stop();
}

#[test]
fn a_slow_subscriber_gets_the_newest_status_not_a_backlog() {
    let (runtime, tx) = test_runtime();
    let subscription = runtime.subscribe();

    for i in 0..5 {
        tx.send(SnapshotMsg::Ok(
            snapshot(&format!("b{i}"), 1),
            Default::default(),
        ))
        .unwrap();
        assert!(wait_for(|| runtime
            .latest()
            .is_some_and(|u| u.payload.contains(&format!("b{i}")))));
    }

    let update = subscription.next_update(Duration::from_millis(50)).unwrap();
    assert!(update.payload.contains("b4"), "got: {}", update.payload);
    assert!(
        subscription
            .next_update(Duration::from_millis(50))
            .is_none(),
        "no stale backlog may remain"
    );
    runtime.stop();
}

#[test]
fn dropping_a_subscription_unregisters_it() {
    let (runtime, _tx) = test_runtime();

    let subscription = runtime.subscribe();
    assert_eq!(runtime.subscriber_count(), 1);
    drop(subscription);

    assert_eq!(runtime.subscriber_count(), 0);
    runtime.stop();
}

#[test]
fn next_update_times_out_when_nothing_changes() {
    let (runtime, _tx) = test_runtime();
    let subscription = runtime.subscribe();
    // The seed: the test runtime's path holds no repository, so the reading
    // taken for the first subscriber says so.
    let seed = subscription.next_update(Duration::from_millis(30)).unwrap();
    assert_eq!(seed.kind, UpdateKind::Unavailable);

    assert!(
        subscription
            .next_update(Duration::from_millis(30))
            .is_none()
    );
    runtime.stop();
}
