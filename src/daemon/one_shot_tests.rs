use super::*;
use crate::daemon::protocol::{ClientMessage, DaemonStatus, ServerMessage};
use crate::daemon::socket::DaemonSocket;
use std::io::Write;
use std::thread;

#[test]
fn one_shot_request_uses_the_configured_endpoint_and_reads_one_typed_response() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("status.sock");
    let socket = DaemonSocket::bind(&path).unwrap();
    let listener = socket.listener().try_clone().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let frame = read_frame(&mut stream).unwrap().unwrap();
        let message: ClientMessage = serde_json::from_slice(&frame.payload).unwrap();
        assert_eq!(message, ClientMessage::Status {});
        let status = DaemonStatus {
            pid: 7,
            version: "test".into(),
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

    let response = request(
        &path,
        &ClientMessage::Status {},
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    assert!(matches!(response, ServerMessage::Status { .. }));
    server.join().unwrap();
}

#[test]
fn a_legacy_status_response_reports_that_the_daemon_must_be_restarted() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("status.sock");
    let socket = DaemonSocket::bind(&path).unwrap();
    let listener = socket.listener().try_clone().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_frame(&mut stream).unwrap();
        let response = format!(
            r#"{{"type":"status","status":{{"pid":7,"version":"{}","started_at_unix_ms":{{"Ok":1}},"uptime_ms":2,"endpoint":{{"Ok":"status.sock"}},"repositories":[],"attached_clients":[]}}}}"#,
            crate::daemon::protocol::version()
        );
        write_frame(&mut stream, &Frame::control(response.into_bytes())).unwrap();
        stream.flush().unwrap();
    });

    let error = request(
        &path,
        &ClientMessage::Status {},
        std::time::Duration::from_secs(1),
    )
    .unwrap_err();

    assert!(
        error.to_string().contains("protocol incompatibility"),
        "{error:#}"
    );
    assert!(error.to_string().contains("legacy endpoint"), "{error:#}");
    assert!(
        error.to_string().contains("restart the daemon"),
        "{error:#}"
    );
    server.join().unwrap();
}

#[test]
fn an_incomplete_legacy_status_marker_remains_a_malformed_response() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("status.sock");
    let socket = DaemonSocket::bind(&path).unwrap();
    let listener = socket.listener().try_clone().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_frame(&mut stream).unwrap();
        let response = format!(
            r#"{{"type":"status","status":{{"pid":7,"version":"{}","started_at_unix_ms":{{"Ok":1}},"uptime_ms":2,"endpoint":7,"repositories":[],"attached_clients":[]}}}}"#,
            crate::daemon::protocol::version()
        );
        write_frame(&mut stream, &Frame::control(response.into_bytes())).unwrap();
        stream.flush().unwrap();
    });

    let error = request(
        &path,
        &ClientMessage::Status {},
        std::time::Duration::from_secs(1),
    )
    .unwrap_err();

    assert!(
        error.to_string().contains("malformed daemon response JSON"),
        "{error:#}"
    );
    assert!(!error.to_string().contains("protocol incompatibility"));
    server.join().unwrap();
}

#[test]
fn a_terminal_response_is_a_wire_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("status.sock");
    let socket = DaemonSocket::bind(&path).unwrap();
    let listener = socket.listener().try_clone().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_frame(&mut stream).unwrap();
        write_frame(&mut stream, &Frame::terminal(vec![1])).unwrap();
        stream.flush().unwrap();
    });
    let error = request(
        &path,
        &ClientMessage::Status {},
        std::time::Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(error.to_string().contains("wire error"), "{error:#}");
    server.join().unwrap();
}

#[cfg(unix)]
#[test]
fn unix_endpoint_override_uses_the_unix_transport_seam() {
    endpoint_override_is_a_path();
}

#[cfg(windows)]
#[test]
fn windows_endpoint_override_uses_the_uds_transport_seam() {
    endpoint_override_is_a_path();
}

fn endpoint_override_is_a_path() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("custom.sock");
    let socket = DaemonSocket::bind(&path).unwrap();
    assert!(connect(&path).is_ok());
    drop(socket);
}
