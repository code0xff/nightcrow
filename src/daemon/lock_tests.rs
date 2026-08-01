use super::InstanceLock;

#[test]
fn the_first_claim_takes_the_lock() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("daemon.lock");

    let lock = InstanceLock::acquire(&path).expect("no error");

    assert!(lock.is_some());
    assert!(path.exists(), "the lock file is created");
}

#[test]
fn a_second_claim_is_refused_while_the_first_is_held() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("daemon.lock");
    let first = InstanceLock::acquire(&path)
        .unwrap()
        .expect("the first wins");

    let second = InstanceLock::acquire(&path).expect("no error");

    assert!(second.is_none(), "a second daemon must not start");
    drop(first);
}

#[test]
fn releasing_the_lock_lets_the_next_daemon_start() {
    // What a clean shutdown leaves. The lock file stays, but unlocked.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("daemon.lock");
    let first = InstanceLock::acquire(&path)
        .unwrap()
        .expect("the first wins");
    drop(first);

    let next = InstanceLock::acquire(&path).expect("no error");

    assert!(next.is_some(), "a released lock is not a held one");
}

#[test]
fn the_lock_file_is_not_removed_on_release() {
    // Deliberate: unlinking on release lets a second daemon lock a file the
    // first has already deleted, and both would then hold "the" lock.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("daemon.lock");

    drop(InstanceLock::acquire(&path).unwrap().expect("locks"));

    assert!(path.exists());
}

#[test]
fn a_lock_file_left_behind_by_a_dead_daemon_is_claimable() {
    // A `kill -9` runs no cleanup, so the file survives with no holder. The
    // kernel released the flock when the process ended, so the next daemon
    // takes it — this is why the lock is a flock and not the file's existence.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("daemon.lock");
    std::fs::write(&path, b"").unwrap();

    let lock = InstanceLock::acquire(&path).expect("no error");

    assert!(lock.is_some());
}

#[test]
fn an_unopenable_path_is_an_error_not_a_refusal() {
    // The two must not be confused: "another daemon is running" is a normal
    // outcome, while a lock that cannot be opened at all is a fault to report.
    let dir = tempfile::TempDir::new().unwrap();
    // A directory in place of the lock file — open() fails with EISDIR.
    let path = dir.path().join("as-a-directory");
    std::fs::create_dir(&path).unwrap();

    assert!(InstanceLock::acquire(&path).is_err());
}

mod failure_meanings {
    use crate::daemon::lock::{Attempt, outcome_of};
    use std::fs::TryLockError;
    use std::io::{Error, ErrorKind};

    #[test]
    fn a_lock_someone_else_holds_is_the_normal_negative_answer() {
        assert_eq!(outcome_of(&TryLockError::WouldBlock), Attempt::Held);
    }

    #[test]
    fn a_signal_mid_call_means_ask_again_rather_than_give_up() {
        let err = TryLockError::Error(Error::from(ErrorKind::Interrupted));
        assert_eq!(outcome_of(&err), Attempt::Interrupted);
    }

    #[test]
    fn anything_else_is_a_failure_and_not_a_daemon() {
        for kind in [
            ErrorKind::PermissionDenied,
            ErrorKind::NotFound,
            ErrorKind::InvalidInput,
        ] {
            let err = TryLockError::Error(Error::from(kind));
            assert_eq!(outcome_of(&err), Attempt::Failed, "{kind:?}");
        }
    }
}
