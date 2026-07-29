use super::DaemonSocket;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;

/// Socket paths have a length limit (~104 bytes on macOS), so tests bind
/// directly in the temp directory root rather than nesting.
fn socket_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join("d.sock")
}

#[test]
fn binding_creates_a_socket_a_client_can_reach() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = socket_path(&dir);

    let socket = DaemonSocket::bind(&path).expect("binds");

    assert!(path.exists());
    UnixStream::connect(&path).expect("a client connects");
    drop(socket);
}

#[test]
fn binding_creates_the_parent_directory() {
    // First run on a machine with no ~/.nightcrow yet.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("nested").join("d.sock");

    let socket = DaemonSocket::bind(&path).expect("binds into a directory it created");

    assert!(path.exists());
    drop(socket);
}

#[test]
fn the_socket_is_readable_only_by_its_owner() {
    // The socket is the authentication: reaching it grants the shells the
    // daemon serves, so group and other must not.
    let dir = tempfile::TempDir::new().unwrap();
    let path = socket_path(&dir);

    let socket = DaemonSocket::bind(&path).expect("binds");

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "got {mode:o}");
    drop(socket);
}

#[test]
fn a_second_daemon_refuses_to_displace_the_first() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = socket_path(&dir);
    let first = DaemonSocket::bind(&path).expect("the first binds");

    let err = DaemonSocket::bind(&path).expect_err("the second must refuse");

    assert!(
        err.to_string().contains("already running"),
        "the error should say why: {err}"
    );
    // The loser must not have disturbed the winner.
    UnixStream::connect(&path).expect("the first daemon is still reachable");
    drop(first);
}

#[test]
fn a_socket_left_behind_by_a_dead_daemon_is_replaced() {
    // What a crash or `kill -9` leaves: the socket file is still there and
    // nothing is listening. Holding the lock is what proves that, so the
    // leftover is cleared rather than refused forever.
    let dir = tempfile::TempDir::new().unwrap();
    let path = socket_path(&dir);
    drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
    assert!(path.exists(), "the stale file is still in place");

    let fresh = DaemonSocket::bind(&path).expect("a stale socket is not a live daemon");

    UnixStream::connect(&path).expect("the fresh daemon answers");
    drop(fresh);
}

#[test]
fn whatever_sits_at_the_socket_path_is_cleared_once_the_lock_is_held() {
    // Not a socket at all, but the lock already proved no daemon is serving
    // this path — so it is debris, and refusing to start over it would strand
    // the user with a manual cleanup.
    let dir = tempfile::TempDir::new().unwrap();
    let path = socket_path(&dir);
    std::fs::write(&path, b"not a socket").unwrap();

    let socket = DaemonSocket::bind(&path).expect("binds over the debris");

    UnixStream::connect(&path).expect("the daemon answers");
    drop(socket);
}

#[test]
fn the_lock_outlives_the_bind_so_a_second_daemon_still_loses() {
    // The claim has to be held, not just taken: a lock dropped after binding
    // would let a second daemon start and displace the socket.
    let dir = tempfile::TempDir::new().unwrap();
    let path = socket_path(&dir);
    let first = DaemonSocket::bind(&path).expect("the first binds");

    // Reported rather than asserted bare: which of the three steps failed is
    // the whole diagnosis if this ever goes intermittent.
    match DaemonSocket::bind(&path) {
        Ok(_) => panic!("a second daemon bound while the first held the lock"),
        Err(err) => assert!(
            err.to_string().contains("already running"),
            "refused for the wrong reason: {err:#}"
        ),
    }

    drop(first);
    if let Err(err) = DaemonSocket::bind(&path) {
        panic!("the path must be free once the first releases: {err:#}");
    }
}

#[test]
fn dropping_the_socket_removes_the_file() {
    // So the next start finds a clean directory instead of probing a leftover.
    let dir = tempfile::TempDir::new().unwrap();
    let path = socket_path(&dir);

    let socket = DaemonSocket::bind(&path).expect("binds");
    assert!(path.exists());
    drop(socket);

    assert!(!path.exists());
}

#[test]
fn the_default_path_sits_beside_the_other_nightcrow_state() {
    let path = super::default_socket_path().expect("a home directory exists");
    assert!(path.ends_with(".nightcrow/daemon.sock"), "got {path:?}");
}

/// Stop and start a daemon hundreds of times over.
///
/// This is the regression guard for a release that was not synchronous: the
/// lock used to be dropped by closing its descriptor, and a `flock` a
/// millisecond later could still see it held — so restarting the daemon failed
/// with "already running" for a daemon that had just gone, about once in every
/// few hundred cycles. One cycle almost never catches that; five hundred do.
#[test]
fn the_lock_holds_and_releases_across_many_cycles() {
    let dir = tempfile::TempDir::new().unwrap();
    for round in 0..500 {
        let path = dir.path().join(format!("c{round}.sock"));
        let first = match DaemonSocket::bind(&path) {
            Ok(socket) => socket,
            Err(err) => panic!("round {round}: the first bind failed: {err:#}"),
        };
        match DaemonSocket::bind(&path) {
            Ok(_) => panic!("round {round}: a second daemon bound while the first held the lock"),
            Err(err) => assert!(
                err.to_string().contains("already running"),
                "round {round}: refused for the wrong reason: {err:#}"
            ),
        }
        drop(first);
        if let Err(err) = DaemonSocket::bind(&path) {
            panic!("round {round}: the path must be free the instant the first releases: {err:#}");
        }
    }
}
