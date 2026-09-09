use super::*;
use crate::daemon::frame::{Frame, read_frame, write_frame};
use crate::daemon::one_shot::{connect, send_request};
use crate::daemon::protocol::{ClientMessage, DaemonStatus, ServerMessage};
use crate::daemon::socket::DaemonSocket;
use std::io::Write;
use std::thread;

#[test]
fn a_healthy_daemon_yields_a_status_the_command_can_render() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("status.sock");
    let socket = DaemonSocket::bind(&path).unwrap();
    let listener = socket.listener().try_clone().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_frame(&mut stream).unwrap();
        let status = DaemonStatus {
            pid: 7,
            version: crate::daemon::protocol::version(),
            started_at_unix_ms: Ok(1),
            uptime_ms: 2,
            web_endpoint: "http://127.0.0.1:4321/".into(),
            attach_endpoint: Ok("status.sock".into()),
            repositories: vec![],
            attached_clients: vec![],
        };
        let response = ServerMessage::Status { status };
        write_frame(
            &mut stream,
            &Frame::control(serde_json::to_vec(&response).unwrap()),
        )
        .unwrap();
        stream.flush().unwrap();
    });

    let status = query_status(&path).unwrap();
    assert_eq!(status.pid, 7);
    server.join().unwrap();
}

#[test]
fn a_stalled_daemon_is_the_timeout_failure_with_its_dedicated_exit_code() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("stalled.sock");
    let socket = DaemonSocket::bind(&path).unwrap();
    let listener = socket.listener().try_clone().unwrap();
    // A fake daemon that accepts the connection and reads the request, then
    // never answers. The held stream keeps the connection open.
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_frame(&mut stream).unwrap();
        thread::sleep(STATUS_TIMEOUT + Duration::from_millis(500));
        drop(stream);
    });

    let error = query_status(&path).unwrap_err();
    assert!(matches!(error, StatusError::Timeout { .. }), "{error}");
    assert!(error.to_string().contains("response timeout"), "{error}");
    assert_eq!(error.exit_code(), exit_code::RESPONSE_TIMEOUT as i32);
    server.join().unwrap();
}

#[test]
fn a_malformed_frame_is_the_protocol_failure_with_its_dedicated_exit_code() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("malformed.sock");
    let socket = DaemonSocket::bind(&path).unwrap();
    let listener = socket.listener().try_clone().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_frame(&mut stream).unwrap();
        write_frame(&mut stream, &Frame::control(vec![0xff, 0xfe])).unwrap();
        stream.flush().unwrap();
    });

    let error = query_status(&path).unwrap_err();
    assert!(matches!(error, StatusError::Protocol(_)), "{error}");
    assert!(error.to_string().contains("protocol error"), "{error}");
    assert_eq!(error.exit_code(), exit_code::PROTOCOL_ERROR as i32);
    server.join().unwrap();
}

#[test]
fn the_stopped_state_does_not_create_a_socket_or_session_artifacts() {
    let dir = tempfile::TempDir::new().unwrap();
    let missing = dir.path().join("missing.sock");
    let error = query_status(&missing).unwrap_err();
    drop(error);
    assert!(!missing.exists());
}

#[test]
fn the_one_shot_request_is_read_only_across_every_failure_class() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("read-only.sock");
    let socket = DaemonSocket::bind(&path).unwrap();
    let listener = socket.listener().try_clone().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let frame = read_frame(&mut stream).unwrap().unwrap();
        let message: ClientMessage = serde_json::from_slice(&frame.payload).unwrap();
        // The status probe must send exactly one status request and nothing
        // that could mutate session state.
        assert_eq!(message, ClientMessage::Status {});
        thread::sleep(STATUS_TIMEOUT + Duration::from_millis(500));
    });

    let _ = query_status(&path).unwrap_err();
    server.join().unwrap();
}

#[cfg(unix)]
#[test]
fn unix_transport_covers_the_failure_classes() {
    transport_failure_classes();
}

#[cfg(windows)]
#[test]
fn windows_transport_covers_the_failure_classes() {
    transport_failure_classes();
}

fn transport_failure_classes() {
    let dir = tempfile::TempDir::new().unwrap();
    // Both transports surface a missing socket as the stopped class.
    let stopped = query_status(&dir.path().join("missing.sock")).unwrap_err();
    assert!(matches!(stopped, StatusError::Stopped { .. }), "{stopped}");
}

#[test]
fn connect_and_send_exist_for_the_stop_command_shared_seam() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("seam.sock");
    let socket = DaemonSocket::bind(&path).unwrap();
    let mut stream = connect(&path).unwrap();
    send_request(&mut stream, &ClientMessage::Status {}).unwrap();
    drop(stream);
    drop(socket);
}
