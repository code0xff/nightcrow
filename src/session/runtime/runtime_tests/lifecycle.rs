use super::*;

#[test]
fn stop_is_idempotent() {
    let (runtime, _tx) = test_runtime();
    runtime.stop();
    runtime.stop();
}

#[test]
fn a_repository_nobody_is_watching_is_not_walked() {
    let (repo, path) = crate::test_util::make_repo();
    let runtime = RepoRuntime::spawn(&path, encode_test_status);

    assert!(!runtime.is_watching(), "opening is not reading");
    thread::sleep(Duration::from_millis(300));
    assert!(
        runtime.latest().is_none(),
        "nothing should publish without a subscriber"
    );

    let subscription = runtime.subscribe();
    assert!(runtime.is_watching(), "a subscriber starts the watch");
    assert!(
        runtime.latest().is_some(),
        "the first subscriber is answered immediately"
    );

    drop(subscription);
    assert!(
        !runtime.is_watching(),
        "the last subscriber stops the watch"
    );
    runtime.stop();
    drop(repo);
}

#[test]
fn a_second_subscriber_does_not_disturb_the_watch() {
    let (repo, path) = crate::test_util::make_repo();
    let runtime = RepoRuntime::spawn(&path, encode_test_status);
    let first = runtime.subscribe();
    let before = runtime.latest().expect("the first subscriber read once");

    let second = runtime.subscribe();

    let after = runtime.latest().expect("status remains available");
    assert!(
        Arc::ptr_eq(&after.json, &before.json),
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
