//! What the reader does with a directory that holds no repository.
//!
//! Beside the worker rather than with the other reader tests, which are already
//! at the length this project splits at, and borrowing their helpers: the claim
//! is the same kind — how many reads arrive, not what they say.

use crate::runtime::snapshot::tests::{SETTLE, next_read, reads_during};
use crate::runtime::snapshot::{MIN_READ_INTERVAL, SnapshotChannel, SnapshotMsg};
use crate::test_util::run_git;

/// An empty directory, which no `git status` can be taken of.
fn not_a_repository() -> (tempfile::TempDir, String) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_string_lossy().to_string();
    (dir, path)
}

#[test]
fn a_path_with_no_repository_is_reported_once_rather_than_every_second() {
    // A pane opened on a directory that is not a checkout — a path typed wrong, a
    // clone that has not landed — used to be told so once a second for as long as
    // it stayed open, which is exactly the cost watching rather than polling
    // exists to remove.
    let (dir, path) = not_a_repository();
    let channel = SnapshotChannel::spawn(&path);

    assert!(
        matches!(next_read(&channel), SnapshotMsg::Err(_)),
        "a directory with no repository in it is reported as an error"
    );

    // Asserted rather than assumed, as the tests next door do: with no watch the
    // reader falls back to the fixed interval by design, and the count below would
    // then fail for a reason that has nothing to do with what it is checking.
    assert!(
        channel.is_watching(),
        "this directory could not be watched, so this machine cannot run this test"
    );
    assert_eq!(
        reads_during(&channel, SETTLE),
        0,
        "and reported once, not once per {MIN_READ_INTERVAL:?}"
    );
    drop(dir);
}

#[test]
fn a_repository_created_where_there_was_none_is_read() {
    // The other half of the claim above: the reader goes quiet because the
    // directory is watched, not because it gave up on it. `git init` writes
    // inside that watch, so the repository is read when it appears rather than
    // whenever the safety net next comes round.
    let (dir, path) = not_a_repository();
    let channel = SnapshotChannel::spawn(&path);
    assert!(matches!(next_read(&channel), SnapshotMsg::Err(_)));

    run_git(&path, &["init"]);

    assert!(
        matches!(next_read(&channel), SnapshotMsg::Ok(..)),
        "the repository that appeared must be read"
    );
    drop(dir);
}
