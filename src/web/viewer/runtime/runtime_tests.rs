use super::*;
use crate::git::diff::{ChangedFile, StatusKind};
use std::sync::mpsc::SyncSender as StdSyncSender;

/// Build an inert runtime plus the sender that feeds it snapshots.
fn test_runtime() -> (
    Arc<RepoRuntime>,
    StdSyncSender<SnapshotMsg>,
    StdSyncSender<()>,
) {
    let (tx, rx) = mpsc::channel::<SnapshotMsg>();
    let (stop_tx, _stop_rx) = mpsc::sync_channel::<()>(0);
    let channel = SnapshotChannel::from_endpoints(rx);
    let runtime = RepoRuntime::start(channel, "test".to_string());
    // The mpsc Sender is not a SyncSender; wrap through a relay so the test
    // helper returns one uniform type.
    let (relay_tx, relay_rx) = mpsc::sync_channel::<SnapshotMsg>(16);
    thread::spawn(move || {
        while let Ok(msg) = relay_rx.recv() {
            if tx.send(msg).is_err() {
                break;
            }
        }
    });
    (runtime, relay_tx, stop_tx)
}

fn snapshot(branch: &str, files: usize) -> RepoSnapshot {
    RepoSnapshot {
        files: (0..files)
            .map(|i| {
                ChangedFile::from_status_columns(
                    format!("f{i}.rs"),
                    None,
                    StatusKind::Modified,
                    StatusKind::Unmodified,
                )
            })
            .collect(),
        tracking: None,
        head_oid: None,
        branch_name: Some(branch.to_string()),
    }
}

/// Poll until `check` passes or the budget runs out. The runtime thread
/// wakes on its own schedule, so tests wait on the condition, not a sleep.
fn wait_for(mut check: impl FnMut() -> bool) -> bool {
    for _ in 0..100 {
        if check() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn runtime_publishes_a_snapshot_as_a_versioned_payload() {
    let (runtime, tx, _stop) = test_runtime();

    tx.send(SnapshotMsg::Ok(snapshot("main", 2), Default::default()))
        .unwrap();

    assert!(
        wait_for(|| runtime.latest().is_some()),
        "no status published"
    );
    let update = runtime.latest().unwrap();
    let value: serde_json::Value = serde_json::from_str(&update.json).unwrap();
    assert_eq!(value["version"], crate::web::viewer::dto::PROTOCOL_VERSION);
    assert_eq!(value["branch"], "main");
    assert_eq!(value["files"].as_array().unwrap().len(), 2);
    runtime.stop();
}

#[test]
fn a_subscriber_is_seeded_with_the_current_status() {
    let (runtime, tx, _stop) = test_runtime();
    tx.send(SnapshotMsg::Ok(snapshot("main", 1), Default::default()))
        .unwrap();
    assert!(wait_for(|| runtime.latest().is_some()));

    // Subscribing after the fact must not wait for the next change.
    let subscription = runtime.subscribe();
    let update = subscription.next_update(Duration::from_millis(50));

    assert!(
        update.is_some(),
        "a fresh subscriber must replay the latest"
    );
    runtime.stop();
}

#[test]
fn a_slow_subscriber_gets_the_newest_status_not_a_backlog() {
    let (runtime, tx, _stop) = test_runtime();
    let subscription = runtime.subscribe();

    // Publish several without ever reading, then read once.
    for i in 0..5 {
        tx.send(SnapshotMsg::Ok(
            snapshot(&format!("b{i}"), 1),
            Default::default(),
        ))
        .unwrap();
        assert!(wait_for(|| runtime
            .latest()
            .is_some_and(|u| u.json.contains(&format!("b{i}")))));
    }

    let update = subscription.next_update(Duration::from_millis(50)).unwrap();
    assert!(
        update.json.contains("b4"),
        "a slow reader must land on the newest, got: {}",
        update.json
    );
    assert!(
        subscription
            .next_update(Duration::from_millis(50))
            .is_none(),
        "no stale backlog may remain"
    );
    runtime.stop();
}

#[test]
fn an_unchanged_snapshot_is_not_republished() {
    // The producer ticks on a timer; an idle repository must stay silent
    // rather than re-emitting the same status every second.
    let (runtime, tx, _stop) = test_runtime();
    tx.send(SnapshotMsg::Ok(snapshot("main", 1), Default::default()))
        .unwrap();
    assert!(wait_for(|| runtime.latest().is_some()));
    let first_seq = runtime.latest().unwrap().seq;
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

    assert_eq!(
        runtime.latest().unwrap().seq,
        first_seq,
        "an identical snapshot must not burn a sequence number"
    );
    assert!(
        subscription
            .next_update(Duration::from_millis(50))
            .is_none(),
        "an idle repository must not wake its subscribers"
    );
    runtime.stop();
}

#[test]
fn sequence_numbers_increase_across_updates() {
    let (runtime, tx, _stop) = test_runtime();

    tx.send(SnapshotMsg::Ok(snapshot("one", 1), Default::default()))
        .unwrap();
    assert!(wait_for(|| runtime.latest().is_some()));
    let first = runtime.latest().unwrap().seq;
    tx.send(SnapshotMsg::Ok(snapshot("two", 1), Default::default()))
        .unwrap();
    assert!(wait_for(|| runtime.latest().is_some_and(|u| u.seq > first)));

    assert_eq!(runtime.latest().unwrap().seq, first + 1);
    runtime.stop();
}

#[test]
fn dropping_a_subscription_unregisters_it() {
    let (runtime, _tx, _stop) = test_runtime();

    let subscription = runtime.subscribe();
    assert_eq!(runtime.subscriber_count(), 1);
    drop(subscription);

    assert_eq!(
        runtime.subscriber_count(),
        0,
        "a dropped client must not keep receiving fan-out"
    );
    runtime.stop();
}

#[test]
fn next_update_times_out_when_nothing_changes() {
    let (runtime, _tx, _stop) = test_runtime();
    let subscription = runtime.subscribe();

    // No snapshot has ever been published, so there is nothing to seed with.
    assert!(
        subscription
            .next_update(Duration::from_millis(30))
            .is_none()
    );
    runtime.stop();
}

#[test]
fn stop_is_idempotent() {
    let (runtime, _tx, _stop) = test_runtime();
    runtime.stop();
    runtime.stop();
}

#[test]
fn a_repository_nobody_is_watching_is_not_walked() {
    // The daemon holds every open repository, and each attached client polls the
    // same trees for itself. Walking one that nothing is reading is the half of
    // that cost with nothing to show for it.
    let (repo, path) = crate::test_util::make_repo();
    let runtime = RepoRuntime::spawn(&path);

    assert!(!runtime.is_watching(), "opening is not reading");
    // Given time to happen, rather than checked before it could: the reader used
    // to be started awake and put to sleep a moment later, and asking straight
    // away only ever caught the losing side of that race.
    thread::sleep(Duration::from_millis(300));
    assert!(
        runtime.latest().is_none(),
        "and nothing has been published yet"
    );

    let subscription = runtime.subscribe();

    assert!(runtime.is_watching(), "a subscriber starts the watch");
    // Seeded from a reading taken on subscribe, not from the tick that has not
    // happened yet: a page opened after a quiet night must not render what was
    // true when the last client left.
    assert!(
        runtime.latest().is_some(),
        "the first subscriber is answered with a reading"
    );

    drop(subscription);

    assert!(
        !runtime.is_watching(),
        "the last subscriber leaving stops it again"
    );
    runtime.stop();
    drop(repo);
}

#[test]
fn a_second_subscriber_does_not_disturb_the_watch() {
    // Only the transitions matter; a client arriving while others are reading
    // must not cost an extra walk on its request thread.
    let (repo, path) = crate::test_util::make_repo();
    let runtime = RepoRuntime::spawn(&path);
    let first = runtime.subscribe();
    let before = runtime.latest().expect("the first subscriber read once");

    let second = runtime.subscribe();

    assert_eq!(
        runtime.latest().map(|update| update.seq),
        Some(before.seq),
        "nothing was republished"
    );
    assert!(runtime.is_watching());
    drop(second);
    assert!(runtime.is_watching(), "one subscriber is still reading");
    drop(first);
    assert!(!runtime.is_watching());
    runtime.stop();
    drop(repo);
}

#[test]
fn concurrent_publishers_never_move_the_latest_status_backwards() {
    // Three threads publish — the runtime's own, a REST handler taking a
    // reading on demand, and a first subscriber taking one for itself. Deciding
    // what is new, numbering it, and storing it have to happen together: apart,
    // two readings can be numbered in one order and stored in the other, and
    // what everyone keeps is the older of the two under the higher number.
    let (runtime, _tx, _stop) = test_runtime();
    let publishers: Vec<_> = (0..4)
        .map(|thread| {
            let runtime = Arc::clone(&runtime);
            std::thread::spawn(move || {
                for round in 0..300 {
                    runtime.publish(
                        &snapshot(&format!("b{thread}-{round}"), 1),
                        &Default::default(),
                    );
                }
            })
        })
        .collect();

    let watcher = {
        let runtime = Arc::clone(&runtime);
        std::thread::spawn(move || {
            let mut highest = 0;
            for _ in 0..20_000 {
                if let Some(update) = runtime.latest() {
                    assert!(
                        update.seq >= highest,
                        "the newest status went back to {} from {highest}",
                        update.seq
                    );
                    highest = update.seq;
                }
            }
        })
    };

    for publisher in publishers {
        publisher.join().expect("a publisher panicked");
    }
    watcher.join().expect("the newest status moved backwards");
    runtime.stop();
}

#[test]
fn concurrent_arrivals_and_departures_leave_the_watch_matching_the_list() {
    // Two questions are asked of the same list — "am I the first?" and "am I
    // the last?" — and the watch is whatever the last answer said. This pins
    // the state they must agree on: a subscriber attached means the tree is
    // read, and the subscriber count is not that fact.
    //
    // It does not reproduce the interleaving those answers must be taken
    // together to survive: that window is the few instructions between reading
    // the list and joining it, and nothing available here forces a thread into
    // it. See the commit that made both take the lock once.
    let (repo, path) = crate::test_util::make_repo();
    let runtime = RepoRuntime::spawn(&path);

    let churn: Vec<_> = (0..8)
        .map(|_| {
            let runtime = Arc::clone(&runtime);
            thread::spawn(move || {
                for _ in 0..400 {
                    let held = runtime.subscribe();
                    drop(held);
                }
            })
        })
        .collect();
    let staying = runtime.subscribe();
    for thread in churn {
        thread.join().expect("a churning subscriber panicked");
    }

    assert!(
        runtime.watch.is_awake(),
        "a subscriber is attached, so the tree is read"
    );
    drop(staying);
    assert!(!runtime.watch.is_awake(), "and stops when it goes");

    runtime.stop();
    drop(repo);
}
