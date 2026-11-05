use super::VersionMismatch;
use crate::daemon::client::{ConnectError, DaemonClient};
use crate::daemon::frame::{Frame, read_frame, write_frame};
use crate::daemon::protocol::{ClientMessage, ServerMessage, version};
use crate::daemon::transport::UnixListener;
use std::io::Write;

#[test]
fn legacy_version_error_requires_the_exact_message_and_supported_versions() {
    let mismatch = VersionMismatch::from_legacy_error("client is 0.1.7, daemon is 0.1.8", "0.1.7")
        .expect("the exact legacy mismatch is recognized");
    assert_eq!(mismatch.to_string(), "client is 0.1.7, daemon is 0.1.8");

    for message in [
        "daemon refused the attach: client is 0.1.7, daemon is 0.1.8",
        "client is 0.1.7, daemon is 0.1.8 extra",
        "client is 0.1.7, daemon is 0.2.0",
        "client is 0.1.7, daemon is 0.1.08",
        "client is 0.1.6, daemon is 0.1.8",
    ] {
        assert!(
            VersionMismatch::from_legacy_error(message, "0.1.7").is_none(),
            "unexpectedly accepted {message:?}"
        );
    }
}

#[test]
fn hello_version_mismatch_is_a_typed_attach_failure() {
    let daemon_version = other_version();
    let error = connect_with_response(ServerMessage::Hello {
        version: daemon_version.clone(),
        client: 7,
    })
    .expect_err("different Hello version is rejected");
    let ConnectError::Failed(error) = error else {
        panic!("a listener response is not an unavailable socket");
    };
    let mismatch = error
        .downcast_ref::<VersionMismatch>()
        .expect("the mismatch type survives anyhow");
    assert_eq!(
        mismatch.to_string(),
        format!("client is {}, daemon is {daemon_version}", version())
    );
}

#[test]
fn exact_legacy_error_is_typed_but_arbitrary_error_is_not() {
    let daemon_version = other_version();
    let error = connect_with_response(ServerMessage::Error {
        message: format!("client is {}, daemon is {daemon_version}", version()),
    })
    .expect_err("legacy daemon rejects the mismatched client");
    assert!(failed_error_is_mismatch(error));

    let error = connect_with_response(ServerMessage::Error {
        message: "client is 0.1.999, daemon is 0.1.1000".into(),
    })
    .expect_err("an unrelated refusal is not accepted as a mismatch");
    let ConnectError::Failed(error) = error else {
        panic!("a listener response is not an unavailable socket");
    };
    assert!(error.downcast_ref::<VersionMismatch>().is_none());
}

fn failed_error_is_mismatch(error: ConnectError) -> bool {
    let ConnectError::Failed(error) = error else {
        panic!("a listener response is not an unavailable socket");
    };
    error.downcast_ref::<VersionMismatch>().is_some()
}

fn connect_with_response(
    response: ServerMessage,
) -> std::result::Result<DaemonClient, ConnectError> {
    let dir = tempfile::TempDir::new().expect("temp directory");
    let path = dir.path().join("mismatch.sock");
    let listener = UnixListener::bind(&path).expect("bind listener");
    let peer = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept client");
        let hello = read_frame(&mut stream)
            .expect("read client hello")
            .expect("client hello frame");
        let message: ClientMessage = serde_json::from_slice(&hello.payload).expect("decode hello");
        assert!(matches!(message, ClientMessage::Hello { .. }));
        let payload = serde_json::to_vec(&response).expect("encode response");
        write_frame(&mut stream, &Frame::control(payload)).expect("write response");
        stream.flush().expect("flush response");
    });
    let result = DaemonClient::connect_for_attach(&path);
    peer.join().expect("server completes handshake");
    result
}

fn other_version() -> String {
    if version() == "0.1.0" {
        "0.1.1".into()
    } else {
        "0.1.0".into()
    }
}
