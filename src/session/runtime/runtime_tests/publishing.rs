use super::*;

#[test]
fn runtime_publishes_a_snapshot_as_a_versioned_payload() {
    let (runtime, tx) = test_runtime();
    tx.send(SnapshotMsg::Ok(snapshot("main", 2), Default::default()))
        .unwrap();

    assert!(
        wait_for(|| runtime.latest().is_some()),
        "no status published"
    );
    let update = runtime.latest().unwrap();
    let value: serde_json::Value = serde_json::from_str(&update.payload).unwrap();
    assert_eq!(value["version"], TEST_PAYLOAD_VERSION);
    assert_eq!(value["branch"], "main");
    assert_eq!(value["files"].as_array().unwrap().len(), 2);
    runtime.stop();
}

#[test]
fn an_unchanged_snapshot_is_not_republished() {
    let (runtime, tx) = test_runtime();
    // Subscribed first: the first subscriber's own reading of the (absent)
    // repository would otherwise replace the status under test.
    let subscription = runtime.subscribe();
    tx.send(SnapshotMsg::Ok(snapshot("main", 1), Default::default()))
        .unwrap();
    assert!(wait_for(|| runtime
        .latest()
        .is_some_and(|u| u.kind == UpdateKind::Status)));
    let first = runtime.latest().unwrap();
    while subscription
        .next_update(Duration::from_millis(50))
        .is_some()
    {}

    for _ in 0..3 {
        tx.send(SnapshotMsg::Ok(snapshot("main", 1), Default::default()))
            .unwrap();
    }
    thread::sleep(Duration::from_millis(200));

    assert!(Arc::ptr_eq(
        &runtime.latest().unwrap().payload,
        &first.payload
    ));
    assert!(
        subscription
            .next_update(Duration::from_millis(50))
            .is_none(),
        "an unchanged snapshot must not wake subscribers"
    );
    runtime.stop();
}

#[test]
fn an_unreadable_tree_is_published_as_unavailable_until_a_status_replaces_it() {
    let (runtime, tx) = test_runtime();
    tx.send(SnapshotMsg::Err("not a git repository".into()))
        .unwrap();

    assert!(
        wait_for(|| runtime.latest().is_some()),
        "a failed reading must still reach a waiting client"
    );
    let update = runtime.latest().unwrap();
    assert_eq!(update.kind, UpdateKind::Unavailable);
    assert_eq!(*update.payload, "not a git repository");

    // `git init` there: the next reading is a status again.
    tx.send(SnapshotMsg::Ok(snapshot("main", 1), Default::default()))
        .unwrap();
    assert!(wait_for(|| runtime
        .latest()
        .is_some_and(|u| u.kind == UpdateKind::Status)));
    runtime.stop();
}
