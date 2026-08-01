use super::*;

#[test]
fn a_repository_is_read_once_on_arrival() {
    let (dir, path) = make_repo();
    let channel = SnapshotChannel::spawn(&path);

    assert!(matches!(next_read(&channel), SnapshotMsg::Ok(..)));
    drop(dir);
}

#[test]
fn a_repository_nothing_happens_in_is_not_read_again() {
    let (dir, path) = make_repo();
    let channel = watched(&path);
    quiesce(&channel);

    assert_eq!(
        reads_during(&channel, SETTLE),
        0,
        "an idle repository must not be walked; fallback is {IDLE_READ_INTERVAL:?}"
    );
    drop(dir);
}

#[test]
fn a_repository_nobody_is_reading_is_neither_read_nor_watched() {
    let (dir, path) = make_repo();
    let channel = watched(&path);

    channel.watch().set_awake(false);
    let deadline = Instant::now() + ARRIVAL;
    while channel.is_watching() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!channel.is_watching(), "the watch must be released");

    reads_during(&channel, Duration::from_millis(300));
    std::fs::write(Path::new(&path).join("unseen.rs"), "fn main() {}").unwrap();
    assert_eq!(reads_during(&channel, Duration::from_millis(500)), 0);

    channel.watch().set_awake(true);
    assert!(matches!(next_read(&channel), SnapshotMsg::Ok(..)));
    drop(dir);
}
