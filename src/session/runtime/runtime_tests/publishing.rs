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
    let value: serde_json::Value = serde_json::from_str(&update.json).unwrap();
    assert_eq!(value["version"], TEST_PAYLOAD_VERSION);
    assert_eq!(value["branch"], "main");
    assert_eq!(value["files"].as_array().unwrap().len(), 2);
    runtime.stop();
}

#[test]
fn an_unchanged_snapshot_is_not_republished() {
    let (runtime, tx) = test_runtime();
    tx.send(SnapshotMsg::Ok(snapshot("main", 1), Default::default()))
        .unwrap();
    assert!(wait_for(|| runtime.latest().is_some()));
    let first = runtime.latest().unwrap();
    let subscription = runtime.subscribe();
    assert!(
        subscription
            .next_update(Duration::from_millis(50))
            .is_some()
    );

    for _ in 0..3 {
        tx.send(SnapshotMsg::Ok(snapshot("main", 1), Default::default()))
            .unwrap();
    }
    thread::sleep(Duration::from_millis(200));

    assert!(Arc::ptr_eq(&runtime.latest().unwrap().json, &first.json));
    assert!(
        subscription
            .next_update(Duration::from_millis(50))
            .is_none(),
        "an unchanged snapshot must not wake subscribers"
    );
    runtime.stop();
}
