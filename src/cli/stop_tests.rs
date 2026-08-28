use super::*;
use crate::daemon::frame::write_frame;
use std::io::{self, Cursor, Read};
use std::time::{Duration, Instant};

fn encoded(frame: &Frame) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_frame(&mut bytes, frame).expect("encode frame");
    bytes
}

fn response(message: &ServerMessage) -> Frame {
    Frame::control(serde_json::to_vec(message).expect("encode response"))
}

fn unsolicited_frames() -> Vec<u8> {
    let mut bytes = encoded(&response(&ServerMessage::Repos {
        repos: Vec::new(),
        active: None,
        accent: 0,
    }));
    bytes.extend(encoded(&Frame::terminal(b"pane output".to_vec())));
    bytes.extend(encoded(&response(&ServerMessage::Hello {
        version: "test".into(),
        client: 1,
    })));
    bytes
}

#[test]
fn unsolicited_frames_are_consumed_before_clean_eof() {
    assert!(
        wait_for_shutdown_ack(&mut Cursor::new(unsolicited_frames()), future_deadline()).is_ok()
    );
}

#[test]
fn unsolicited_frames_are_consumed_before_a_read_timeout() {
    let mut reader = TrailingErrorReader {
        data: Cursor::new(unsolicited_frames()),
        kind: io::ErrorKind::TimedOut,
    };

    let error =
        wait_for_shutdown_ack(&mut reader, future_deadline()).expect_err("timeout is not an ack");
    assert!(
        error
            .to_string()
            .contains("waiting for the daemon to acknowledge the shutdown")
    );
    assert!(
        reader.exhausted(),
        "every unsolicited frame must be consumed"
    );
}

#[test]
fn daemon_error_is_a_failed_shutdown_acknowledgment() {
    let bytes = encoded(&response(&ServerMessage::Error {
        message: "old daemon rejected shutdown".into(),
    }));

    let error = wait_for_shutdown_ack(&mut Cursor::new(bytes), future_deadline())
        .expect_err("error response");
    assert!(error.to_string().contains("old daemon rejected shutdown"));
}

#[test]
fn clean_eof_is_a_shutdown_acknowledgment() {
    assert!(wait_for_shutdown_ack(&mut Cursor::new(Vec::new()), future_deadline()).is_ok());
}

#[test]
fn reset_and_abort_are_shutdown_acknowledgments() {
    for kind in [
        io::ErrorKind::ConnectionReset,
        io::ErrorKind::ConnectionAborted,
    ] {
        let mut reader = FailingReader { kind };
        assert!(
            wait_for_shutdown_ack(&mut reader, future_deadline()).is_ok(),
            "{kind:?}"
        );
    }
}

#[test]
fn unrelated_socket_errors_are_failures() {
    let mut reader = FailingReader {
        kind: io::ErrorKind::PermissionDenied,
    };

    let error =
        wait_for_shutdown_ack(&mut reader, future_deadline()).expect_err("unrelated socket error");
    assert!(
        error
            .to_string()
            .contains("waiting for the daemon to acknowledge the shutdown")
    );
}

#[test]
fn an_expired_shutdown_ack_deadline_is_a_failure() {
    let error = wait_for_shutdown_ack(&mut Cursor::new(Vec::new()), Instant::now())
        .expect_err("expired deadline");

    assert!(
        error
            .to_string()
            .contains("timed out waiting for the daemon to acknowledge the shutdown")
    );
}

fn future_deadline() -> Instant {
    Instant::now() + Duration::from_secs(1)
}

struct TrailingErrorReader {
    data: Cursor<Vec<u8>>,
    kind: io::ErrorKind,
}

impl TrailingErrorReader {
    fn exhausted(&self) -> bool {
        self.data.position() == self.data.get_ref().len() as u64
    }
}

impl Read for TrailingErrorReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.exhausted() {
            Err(io::Error::new(self.kind, "trailing test socket error"))
        } else {
            self.data.read(buf)
        }
    }
}

struct FailingReader {
    kind: io::ErrorKind,
}

impl Read for FailingReader {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(self.kind, "test socket error"))
    }
}
