//! Server-sent events over a plain synchronous writer.
//!
//! The ordinary response builder in [`super::http`] always emits a
//! `Content-Length` and `Connection: close`, which ends the connection after
//! one body — the opposite of what a live stream needs. An SSE response
//! instead keeps the socket open and appends events until one side gives up,
//! so it writes its own head and owns the connection from then on.
//!
//! Generic over [`Write`] so the framing is unit-testable against a buffer
//! and the same code drives a real `TcpStream`.
//!
use std::io::{self, Write};

/// Written when a client subscribes, before any event.
const HEAD: &[u8] = b"HTTP/1.1 200 OK\r\n\
Content-Type: text/event-stream\r\n\
Cache-Control: no-store\r\n\
X-Content-Type-Options: nosniff\r\n\
Connection: keep-alive\r\n\
X-Accel-Buffering: no\r\n\
\r\n";

/// A live event stream that has taken over a connection.
///
/// Every write is flushed: an event still sitting in a buffer is not an event
/// the browser has seen. Write failures are returned rather than swallowed —
/// they are how a disconnect is detected, since a peer that closes the tab is
/// only noticed when the next write fails.
pub struct SseStream<W: Write> {
    sink: W,
    next_id: u64,
}

impl<W: Write> SseStream<W> {
    /// Send the response head and take over the connection.
    pub fn start(mut sink: W) -> io::Result<Self> {
        sink.write_all(HEAD)?;
        sink.flush()?;
        Ok(Self { sink, next_id: 0 })
    }

    /// Append one event and return the sequence number it was given. Clients
    /// use the id to tell a replayed snapshot from a newer one.
    ///
    /// `event` must be a plain name: a newline or carriage return in it would
    /// let the caller forge additional SSE fields, so it is rejected rather
    /// than escaped. `data` needs no such guard — it is split across `data:`
    /// lines, which is both the spec's framing and what makes it inert.
    pub fn send(&mut self, event: &str, data: &str) -> io::Result<u64> {
        if event.is_empty() || event.contains(['\r', '\n']) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "SSE event name must be non-empty and single-line",
            ));
        }
        let id = self.next_id;
        self.next_id += 1;

        let mut frame = String::with_capacity(data.len() + 64);
        frame.push_str("id: ");
        frame.push_str(&id.to_string());
        frame.push('\n');
        frame.push_str("event: ");
        frame.push_str(event);
        frame.push('\n');
        // `\r\n`, `\n` and `\r` all terminate a line in the SSE grammar, so
        // each has to become its own `data:` field.
        for line in data.split("\r\n").flat_map(|l| l.split(['\n', '\r'])) {
            frame.push_str("data: ");
            frame.push_str(line);
            frame.push('\n');
        }
        // The blank line is what dispatches the event.
        frame.push('\n');

        self.sink.write_all(frame.as_bytes())?;
        self.sink.flush()?;
        Ok(id)
    }

    /// Send a comment line. Carries no event, but proves the socket is still
    /// writable and stops an idle proxy from reaping the connection.
    pub fn heartbeat(&mut self) -> io::Result<()> {
        self.sink.write_all(b": keep-alive\n\n")?;
        self.sink.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sink that fails every write, standing in for a client that vanished.
    struct BrokenPipe;

    impl Write for BrokenPipe {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "peer went away"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn text(buf: &[u8]) -> String {
        String::from_utf8(buf.to_vec()).unwrap()
    }

    #[test]
    fn start_writes_a_streaming_head_without_content_length() {
        let mut buf = Vec::new();
        SseStream::start(&mut buf).unwrap();

        let head = text(&buf);
        assert!(head.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(head.contains("Content-Type: text/event-stream\r\n"));
        assert!(head.contains("Cache-Control: no-store\r\n"));
        assert!(head.contains("X-Content-Type-Options: nosniff\r\n"));
        assert!(
            !head.contains("Content-Length"),
            "a streaming response must not declare a length"
        );
        assert!(
            !head.contains("Connection: close"),
            "the connection must stay open for later events"
        );
    }

    #[test]
    fn send_frames_an_event_with_an_increasing_id() {
        let mut buf = Vec::new();
        let mut sse = SseStream::start(&mut buf).unwrap();

        assert_eq!(sse.send("status", "{\"a\":1}").unwrap(), 0);
        assert_eq!(sse.send("status", "{\"a\":2}").unwrap(), 1);

        let body = text(&buf);
        assert!(body.ends_with("id: 1\nevent: status\ndata: {\"a\":2}\n\n"));
    }

    #[test]
    fn send_splits_every_newline_form_into_its_own_data_line() {
        let mut buf = Vec::new();
        let mut sse = SseStream::start(&mut buf).unwrap();

        sse.send("log", "one\ntwo\r\nthree\rfour").unwrap();

        let body = text(&buf);
        assert!(
            body.ends_with("data: one\ndata: two\ndata: three\ndata: four\n\n"),
            "unexpected framing: {body}"
        );
    }

    #[test]
    fn send_cannot_forge_extra_fields_through_a_data_payload() {
        // The classic injection: a payload that tries to close its own event
        // and open another. Splitting on newlines makes each line inert.
        let mut buf = Vec::new();
        {
            let mut sse = SseStream::start(&mut buf).unwrap();
            sse.send("status", "safe\n\nevent: admin\ndata: forged")
                .unwrap();
        }

        // Every injected line is prefixed into a `data:` field, and the blank
        // line the payload tried to smuggle becomes `data: ` — inert, because
        // only a truly empty line dispatches an event.
        let body = text(&buf[HEAD.len()..]);
        assert_eq!(
            body,
            "id: 0\nevent: status\ndata: safe\ndata: \ndata: event: admin\ndata: data: forged\n\n"
        );
        assert_eq!(
            body.matches("\n\n").count(),
            1,
            "exactly one event terminator: {body}"
        );
    }

    #[test]
    fn send_rejects_a_multiline_event_name() {
        let mut buf = Vec::new();
        let mut sse = SseStream::start(&mut buf).unwrap();

        for forged in ["status\nevent: admin", "status\rdata: x", ""] {
            let err = sse.send(forged, "payload").unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        }
        assert!(
            !text(&buf).contains("admin"),
            "a rejected event must write nothing"
        );
    }

    #[test]
    fn send_reports_a_disconnected_client() {
        let mut sse = SseStream {
            sink: BrokenPipe,
            next_id: 0,
        };

        let err = sse.send("status", "{}").unwrap_err();

        assert_eq!(
            err.kind(),
            io::ErrorKind::BrokenPipe,
            "a vanished peer must surface, not be swallowed"
        );
    }

    #[test]
    fn heartbeat_writes_a_comment_that_is_not_an_event() {
        let mut buf = Vec::new();
        {
            let mut sse = SseStream::start(&mut buf).unwrap();
            sse.heartbeat().unwrap();
        }

        let sent = text(&buf[HEAD.len()..]);
        assert_eq!(sent, ": keep-alive\n\n");
        assert!(!sent.contains("event:"), "a heartbeat carries no event");
    }
}
