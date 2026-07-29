//! What the reader does with its time.
//!
//! The point of watching rather than polling is a repository that costs nothing
//! while nothing happens, so most of these are about *absence*: how many reads
//! arrive, not what they say. They use short windows against a ten-second idle
//! interval, so a slow machine delays them rather than failing them.

use super::{IDLE_READ_INTERVAL, MIN_READ_INTERVAL, SnapshotChannel, SnapshotMsg};
use crate::test_util::{make_repo, run_git};
use std::path::Path;
use std::time::{Duration, Instant};

/// Long enough for a filesystem event to travel and a read to happen, without
/// reaching the idle interval that would make the assertion meaningless.
const SETTLE: Duration = Duration::from_millis(2_500);

/// Wait for one snapshot, or fail. Errors count: what is being timed is the read,
/// not what it found.
fn next_read(channel: &SnapshotChannel) -> SnapshotMsg {
    let deadline = Instant::now() + SETTLE;
    while Instant::now() < deadline {
        match channel.try_recv() {
            Ok(msg) => return msg,
            Err(_) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    panic!("no snapshot arrived within {SETTLE:?}");
}

/// How many snapshots arrive over `window`.
fn reads_during(channel: &SnapshotChannel, window: Duration) -> usize {
    let deadline = Instant::now() + window;
    let mut reads = 0;
    while Instant::now() < deadline {
        while channel.try_recv().is_ok() {
            reads += 1;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    reads
}

/// A channel on a real repository, past its first read, with the watch confirmed.
///
/// The watch is asserted rather than assumed: without it the reader falls back to
/// the old fixed interval, and every absence test below would fail for a reason
/// that has nothing to do with what it is checking.
fn watched(path: &str) -> SnapshotChannel {
    let channel = SnapshotChannel::spawn(path);
    next_read(&channel);
    let deadline = Instant::now() + SETTLE;
    while !channel.is_watching() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        channel.is_watching(),
        "the work tree could not be watched, so this machine cannot run these tests"
    );
    channel
}

#[test]
fn a_repository_is_read_once_on_arrival() {
    // A client that has just opened one has nothing to render until this lands,
    // so it does not wait for a change.
    let (dir, path) = make_repo();
    let channel = SnapshotChannel::spawn(&path);

    assert!(matches!(next_read(&channel), SnapshotMsg::Ok(..)));
    drop(dir);
}

#[test]
fn a_repository_nothing_happens_in_is_not_read_again() {
    // The whole point: a session left open costs nothing. The old reader walked
    // the tree once a second forever, which on a large checkout was 129 ms of
    // every second spent finding nothing.
    let (dir, path) = make_repo();
    let channel = watched(&path);

    let reads = reads_during(&channel, SETTLE);

    assert_eq!(
        reads, 0,
        "an idle repository must not be walked; the idle interval is {IDLE_READ_INTERVAL:?}"
    );
    drop(dir);
}

#[test]
fn a_file_written_in_the_work_tree_is_read_at_once() {
    // And promptly: the old reader took up to its interval to notice, this one is
    // told. Bounded well under the idle interval so what is being measured is the
    // watch, not the safety net.
    let (dir, path) = make_repo();
    let channel = watched(&path);

    std::fs::write(Path::new(&path).join("edited.rs"), "fn main() {}").unwrap();

    let SnapshotMsg::Ok(snapshot, _) = next_read(&channel) else {
        panic!("the read failed");
    };
    assert!(
        snapshot.files.iter().any(|f| f.path == "edited.rs"),
        "the read that followed the change must see it: {:?}",
        snapshot.files
    );
    drop(dir);
}

#[test]
fn staging_a_file_is_read_even_though_the_work_tree_did_not_change() {
    // `git add` moves the change from one status column to the other and touches
    // nothing but `.git/index`. A watch on the work tree alone would miss it.
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("staged.rs"), "fn main() {}").unwrap();
    let channel = watched(&path);
    // The write above may already have been read; take whatever is pending so the
    // next read is the one the staging causes.
    reads_during(&channel, Duration::from_millis(300));

    run_git(&path, &["add", "staged.rs"]);

    let SnapshotMsg::Ok(snapshot, _) = next_read(&channel) else {
        panic!("the read failed");
    };
    let staged = snapshot
        .files
        .iter()
        .find(|f| f.path == "staged.rs")
        .expect("the file is still listed");
    assert_eq!(staged.short_code(), "A ", "staged, not untracked");
    drop(dir);
}

#[test]
fn a_burst_of_changes_is_read_at_the_old_pace_rather_than_per_event() {
    // The rate limit is what makes watching safe: a tree that churns without
    // pause — a build writing files git has not been told to ignore — costs
    // exactly what the fixed-interval poll cost, never more.
    let (dir, path) = make_repo();
    let channel = watched(&path);

    for i in 0..400 {
        std::fs::write(Path::new(&path).join(format!("f{i}.rs")), "fn main() {}").unwrap();
    }
    let window = Duration::from_millis(2_500);
    let reads = reads_during(&channel, window);

    let ceiling = (window.as_millis() / MIN_READ_INTERVAL.as_millis()) as usize + 1;
    assert!(
        reads <= ceiling,
        "400 writes produced {reads} reads in {window:?}; the rate limit allows {ceiling}"
    );
    assert!(reads >= 1, "and the changes must be read at all");
    drop(dir);
}

#[test]
fn changes_git_ignores_do_not_cause_a_read() {
    // Build output is the loudest thing in a working tree and cannot appear in a
    // status, so it is not worth a walk — which matters because a pane running a
    // build is this app's ordinary state.
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join(".gitignore"), "out/\n").unwrap();
    std::fs::create_dir_all(Path::new(&path).join("out")).unwrap();
    let channel = watched(&path);
    reads_during(&channel, Duration::from_millis(300));

    for i in 0..200 {
        std::fs::write(
            Path::new(&path).join("out").join(format!("artifact{i}.o")),
            "x",
        )
        .unwrap();
    }

    assert_eq!(
        reads_during(&channel, SETTLE),
        0,
        "ignored paths must not wake the reader"
    );
    drop(dir);
}

#[test]
fn a_repository_nobody_is_reading_is_neither_read_nor_watched() {
    // The viewer turns this off for a repository with no subscriber. Both halves
    // stop: no walks, and no watch descriptors held for a tree nobody looks at.
    let (dir, path) = make_repo();
    let channel = watched(&path);

    channel.watch().set_awake(false);
    let deadline = Instant::now() + SETTLE;
    while channel.is_watching() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !channel.is_watching(),
        "the watch is released with the reads"
    );
    reads_during(&channel, Duration::from_millis(300));
    std::fs::write(Path::new(&path).join("unseen.rs"), "fn main() {}").unwrap();
    assert_eq!(
        reads_during(&channel, Duration::from_millis(500)),
        0,
        "a change in a repository nobody reads costs nothing"
    );

    // Resuming answers now rather than on the interval: a client is waiting.
    channel.watch().set_awake(true);
    assert!(matches!(next_read(&channel), SnapshotMsg::Ok(..)));
    drop(dir);
}
