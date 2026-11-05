use super::*;
use std::cell::RefCell;
use std::io::Cursor;
use std::path::Path;

#[test]
fn a_running_daemon_is_handshaken_once_for_the_actual_attach_client() {
    let dir = tempfile::TempDir::new().expect("a temp dir");
    let path = dir.path().join("live.sock");
    let daemon = crate::daemon::socket::DaemonSocket::bind(&path).expect("bind daemon");
    let listener = daemon
        .listener()
        .try_clone()
        .expect("clone daemon listener");
    let server = std::thread::spawn(move || handshake_and_count_clients(listener));

    let _client = connect_or_start(&path, dir.path(), false).expect("attach succeeds");
    assert_eq!(server.join().expect("server succeeds"), 1);
}

#[test]
fn an_unbound_socket_path_does_not_read_as_a_running_daemon() {
    let dir = tempfile::TempDir::new().expect("a temp dir");
    let path = dir.path().join("nightcrow.sock");

    assert!(matches!(
        DaemonClient::connect_for_attach(&path),
        Err(ConnectError::Unavailable(_))
    ));
    std::fs::write(&path, b"").expect("write stale socket stand-in");
    assert!(matches!(
        DaemonClient::connect_for_attach(&path),
        Err(ConnectError::Unavailable(_))
    ));
}

#[test]
fn restart_confirmation_accepts_only_yes_answers() {
    for answer in ["y\n", "Y\n", "yes\n", "YeS\n"] {
        let mut input = Cursor::new(answer);
        let mut output = Vec::new();
        assert!(
            confirm_restart(&mismatch(), prompt(&mut input, &mut output)).expect("read the answer")
        );
    }

    for answer in ["\n", "n\n", "no\n", "yep\n", ""] {
        let mut input = Cursor::new(answer);
        let mut output = Vec::new();
        assert!(
            !confirm_restart(&mismatch(), prompt(&mut input, &mut output))
                .expect("read the answer")
        );
    }
}

#[test]
fn noninteractive_confirmation_does_not_prompt_or_approve() {
    assert!(!confirm_restart(&mismatch(), RestartPrompt::unavailable()).expect("noninteractive"));
}

#[test]
fn accepted_restart_runs_stop_start_and_attach_once_if_reconnect_fails() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let socket = dir.path().join("live.sock");
    let log = dir.path().join("daemon.out");
    let events = RefCell::new(Vec::new());
    let mut input = Cursor::new("YES\n");
    let mut output = Vec::new();

    let result = restart_for_mismatch(
        context(&socket, &log),
        prompt(&mut input, &mut output),
        RestartOperations {
            stop: |path: &Path| {
                assert_eq!(path, socket.as_path());
                events.borrow_mut().push("stop");
                Ok(())
            },
            start: |path: &Path| {
                assert_eq!(path, log.as_path());
                events.borrow_mut().push("start");
                Ok(())
            },
            wait: |_: &mut (), path: &Path, _: &Path, _: anyhow::Error| -> anyhow::Result<()> {
                assert_eq!(path, socket.as_path());
                events.borrow_mut().push("attach");
                anyhow::bail!("new daemon handshake failed")
            },
        },
    );

    assert!(
        result
            .expect_err("the new handshake failure propagates")
            .to_string()
            .contains("new daemon handshake failed")
    );
    assert_eq!(*events.borrow(), ["stop", "start", "attach"]);
    let prompt = String::from_utf8(output).expect("prompt is UTF-8");
    assert!(prompt.contains("client is 0.1.1, daemon is 0.1.2"));
    assert!(prompt.contains("terminate every terminal pane and running program"));
    assert_eq!(prompt.matches("Restart the session now?").count(), 1);
}

#[test]
fn stop_failure_never_starts_a_replacement() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let socket = dir.path().join("live.sock");
    let log = dir.path().join("daemon.out");
    let mut input = Cursor::new("y\n");
    let mut output = Vec::new();
    let events = RefCell::new(Vec::new());

    let error = restart_for_mismatch(
        context(&socket, &log),
        prompt(&mut input, &mut output),
        RestartOperations {
            stop: |_: &Path| {
                events.borrow_mut().push("stop");
                anyhow::bail!("stop failed")
            },
            start: |_: &Path| -> anyhow::Result<()> {
                events.borrow_mut().push("start");
                anyhow::bail!("must not start")
            },
            wait: |_: &mut (), _: &Path, _: &Path, _: anyhow::Error| -> anyhow::Result<()> {
                anyhow::bail!("must not attach")
            },
        },
    )
    .expect_err("stop error propagates");

    assert_eq!(*events.borrow(), ["stop"]);
    assert!(error.to_string().contains("stop failed"));
}

#[test]
fn noninteractive_or_declined_mismatch_keeps_manual_guidance_and_does_not_stop() {
    for (answer, interactive) in [("y\n", false), ("n\n", true), ("", true)] {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let socket = dir.path().join("live.sock");
        let log = dir.path().join("daemon.out");
        let mut input = Cursor::new(answer);
        let mut output = Vec::new();
        let error = restart_for_mismatch(
            context(&socket, &log),
            if interactive {
                prompt(&mut input, &mut output)
            } else {
                RestartPrompt::unavailable()
            },
            RestartOperations {
                stop: |_: &Path| -> anyhow::Result<()> { panic!("decline must not stop") },
                start: |_: &Path| -> anyhow::Result<()> { panic!("decline must not start") },
                wait: |_: &mut (), _: &Path, _: &Path, _: anyhow::Error| -> anyhow::Result<()> {
                    panic!("decline must not attach")
                },
            },
        )
        .expect_err("manual action is required");

        assert!(error.to_string().contains("nightcrow stop --socket"));
        assert!(format!("{error:#}").contains("client is 0.1.1, daemon is 0.1.2"));
    }
}

#[test]
fn prompt_write_failure_propagates_before_any_lifecycle_action() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let socket = dir.path().join("live.sock");
    let log = dir.path().join("daemon.out");
    let mut input = Cursor::new("y\n");
    let mut output = FailedWriter;

    let error = restart_for_mismatch(
        context(&socket, &log),
        prompt(&mut input, &mut output),
        RestartOperations {
            stop: |_: &Path| -> anyhow::Result<()> { panic!("prompt failure must not stop") },
            start: |_: &Path| -> anyhow::Result<()> { panic!("prompt failure must not start") },
            wait: |_: &mut (), _: &Path, _: &Path, _: anyhow::Error| -> anyhow::Result<()> {
                panic!("prompt failure must not attach")
            },
        },
    )
    .expect_err("the terminal write error propagates");

    assert!(error.to_string().contains("prompt output failed"));
}

fn handshake_and_count_clients(listener: crate::daemon::transport::UnixListener) -> usize {
    use crate::daemon::frame::{Frame, read_frame, write_frame};
    use crate::daemon::protocol::{ClientMessage, ServerMessage, version};
    use std::io::Write;

    listener
        .set_nonblocking(true)
        .expect("make listener nonblocking");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut accepted = 0;
    let mut handshaken = false;
    while std::time::Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                accepted += 1;
                stream.set_nonblocking(false).expect("make stream blocking");
                stream
                    .set_read_timeout(Some(std::time::Duration::from_millis(100)))
                    .expect("set handshake timeout");
                if let Ok(Some(frame)) = read_frame(&mut stream) {
                    let message: ClientMessage =
                        serde_json::from_slice(&frame.payload).expect("decode hello");
                    assert!(matches!(message, ClientMessage::Hello { .. }));
                    let hello = serde_json::to_vec(&ServerMessage::Hello {
                        version: version(),
                        client: 1,
                    })
                    .expect("encode hello");
                    write_frame(&mut stream, &Frame::control(hello)).expect("write hello");
                    stream.flush().expect("flush hello");
                    handshaken = true;
                }
                if handshaken {
                    break;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(err) => panic!("accepting attach client: {err}"),
        }
    }
    assert!(
        handshaken,
        "the actual attach client never completed handshake"
    );
    accepted
}

fn mismatch() -> VersionMismatch {
    VersionMismatch::new("0.1.1".into(), "0.1.2".into())
}

fn context<'a>(socket: &'a std::path::Path, log: &'a std::path::Path) -> RestartContext<'a> {
    RestartContext {
        socket,
        log,
        original_error: anyhow::anyhow!(mismatch()),
        mismatch: mismatch(),
    }
}

fn prompt<'a>(input: &'a mut dyn BufRead, output: &'a mut dyn Write) -> RestartPrompt<'a> {
    RestartPrompt::interactive(input, output)
}

struct FailedWriter;

impl Write for FailedWriter {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("prompt output failed"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::other("prompt output failed"))
    }
}
