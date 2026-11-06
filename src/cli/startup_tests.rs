use super::{wait_for_daemon, wait_for_daemon_with_timeout};
use crate::daemon::frame::{Frame, read_frame, write_frame};
use crate::daemon::protocol::{ClientMessage, ServerMessage, version};
use crate::daemon::transport::UnixListener;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const CHILD_FIXTURE: &str = "NIGHTCROW_STARTUP_TEST_CHILD";
const CHILD_FIXTURE_TEST: &str = "cli::startup::tests::child_process_fixture_exits_successfully";

#[test]
#[ignore = "spawned as an isolated child process fixture"]
fn child_process_fixture_exits_successfully() {
    assert!(std::env::var_os(CHILD_FIXTURE).is_some());
}

#[test]
fn an_exited_child_is_reported_immediately_with_its_log_path() {
    let dir = tempfile::TempDir::new().expect("a temp directory");
    let socket = dir.path().join("daemon.sock");
    let log = dir.path().join("daemon.out");
    let mut child = child_fixture();
    let pid = child.id();
    let status = child.wait().expect("the child fixture exits");
    assert!(status.success(), "fixture status: {status}");
    let started = Instant::now();

    let err = wait_for_daemon_with_timeout(
        &mut child,
        &socket,
        &log,
        anyhow::anyhow!("not ready"),
        Duration::from_secs(5),
    )
    .expect_err("the exited child cannot start a daemon");

    let message = format!("{err:#}");
    assert!(
        message.contains(&format!("process {pid} exited (")),
        "{message}"
    );
    assert!(message.contains(&status.to_string()), "{message}");
    assert!(message.contains(&log.display().to_string()), "{message}");
    assert!(started.elapsed() < Duration::from_secs(5), "{message}");
}

#[test]
fn a_ready_daemon_wins_before_the_child_exit_is_checked() {
    let dir = tempfile::TempDir::new().expect("a temp directory");
    let socket = dir.path().join("daemon.sock");
    let log = dir.path().join("daemon.out");
    let mut child = child_fixture();
    let status = child.wait().expect("the child fixture exits");
    assert!(status.success(), "fixture status: {status}");
    assert!(child.try_wait().expect("read child status").is_some());
    let server = start_handshake_server(&socket, version());

    let client = wait_for_daemon(&mut child, &socket, &log, anyhow::anyhow!("not ready"))
        .expect("a concurrently started daemon is attachable");

    drop(client);
    server
        .join()
        .expect("the fake daemon completes the handshake");
}

#[test]
fn handshake_failure_is_returned_instead_of_misreported_as_child_exit() {
    let dir = tempfile::TempDir::new().expect("a temp directory");
    let socket = dir.path().join("daemon.sock");
    let log = dir.path().join("daemon.out");
    let mut child = child_fixture();
    let status = child.wait().expect("the child fixture exits");
    assert!(status.success(), "fixture status: {status}");
    let server = start_handshake_server(&socket, "other-version".to_string());

    let err = wait_for_daemon(&mut child, &socket, &log, anyhow::anyhow!("not ready"))
        .expect_err("a protocol mismatch is a daemon failure");

    server
        .join()
        .expect("the fake daemon completes the handshake");
    let message = format!("{err:#}");
    assert!(message.contains("daemon is other-version"), "{message}");
    assert!(!message.contains("exited"), "{message}");
}

fn child_fixture() -> Child {
    Command::new(std::env::current_exe().expect("the test executable path"))
        .arg("--exact")
        .arg(CHILD_FIXTURE_TEST)
        .arg("--ignored")
        .env(CHILD_FIXTURE, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the single-test child fixture")
}

fn start_handshake_server(socket: &Path, server_version: String) -> std::thread::JoinHandle<()> {
    let listener = UnixListener::bind(socket).expect("bind the fake daemon socket");
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept the attach client");
        let frame = read_frame(&mut stream)
            .expect("read the client hello")
            .expect("the client sends a hello");
        let hello: ClientMessage = serde_json::from_slice(&frame.payload).expect("decode hello");
        assert!(matches!(hello, ClientMessage::Hello { .. }));
        let response = serde_json::to_vec(&ServerMessage::Hello {
            version: server_version,
            client: 1,
            can_clone: true,
        })
        .expect("encode the daemon hello");
        write_frame(&mut stream, &Frame::control(response)).expect("write the daemon hello");
    })
}
