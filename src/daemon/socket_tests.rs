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
    // What a crash or `kill -9` leaves: the file is still there, nothing is
    // listening. Binding must recover rather than refuse forever.
    let dir = tempfile::TempDir::new().unwrap();
    let path = socket_path(&dir);
    // Bound through the raw listener, not `DaemonSocket`: dropping it closes
    // the descriptor without unlinking, which is what the kernel leaves behind
    // when a daemon dies. Leaking a `DaemonSocket` instead would skip the
    // unlink but keep the descriptor open, and the file would still accept.
    drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
    assert!(path.exists(), "the stale file is still in place");

    let fresh = DaemonSocket::bind(&path).expect("a stale socket is not a live daemon");

    UnixStream::connect(&path).expect("the fresh daemon answers");
    drop(fresh);
}

#[test]
fn a_plain_file_in_the_way_is_reported_not_deleted() {
    // Not a socket at all — connect fails with something other than "refused",
    // so this must surface rather than silently remove whatever is there.
    let dir = tempfile::TempDir::new().unwrap();
    let path = socket_path(&dir);
    std::fs::write(&path, b"not a socket").unwrap();

    assert!(DaemonSocket::bind(&path).is_err());
    assert!(path.exists(), "an unrecognized file must be left alone");
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
