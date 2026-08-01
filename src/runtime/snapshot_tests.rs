use super::{IDLE_READ_INTERVAL, MIN_READ_INTERVAL, SnapshotChannel, SnapshotMsg};
use crate::test_util::{make_linked_worktree, make_repo, run_git};
use std::path::Path;
use std::time::{Duration, Instant};

#[path = "snapshot_tests/changes.rs"]
mod changes;
#[path = "snapshot_tests/lifecycle.rs"]
mod lifecycle;

pub(super) const SETTLE: Duration = Duration::from_millis(2_500);
const ARRIVAL: Duration = Duration::from_secs(15);

pub(super) fn next_read(channel: &SnapshotChannel) -> SnapshotMsg {
    let deadline = Instant::now() + ARRIVAL;
    while Instant::now() < deadline {
        if let Ok(message) = channel.try_recv() {
            return message;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no snapshot arrived within {ARRIVAL:?}");
}

pub(super) fn quiesce(channel: &SnapshotChannel) {
    let quiet_for = MIN_READ_INTERVAL * 3;
    let deadline = Instant::now() + ARRIVAL;
    let mut last_read = Instant::now();
    while Instant::now() < deadline {
        if channel.try_recv().is_ok() {
            last_read = Instant::now();
        } else if last_read.elapsed() >= quiet_for {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the reader never went quiet for {quiet_for:?}");
}

pub(super) fn reads_during(channel: &SnapshotChannel, window: Duration) -> usize {
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

fn watched(path: &str) -> SnapshotChannel {
    let channel = SnapshotChannel::spawn(path);
    next_read(&channel);
    let deadline = Instant::now() + ARRIVAL;
    while !channel.is_watching() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(channel.is_watching(), "filesystem watch was not installed");
    channel
}
