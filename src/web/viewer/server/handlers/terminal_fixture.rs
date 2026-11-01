//! A page and a hub on either end of a real socket, for the connection tests.

use super::super::terminal_liveness::Timing;
use super::super::terminal_pipe::{Duplex, Link};
use crate::session::size_owner::ViewerId;
use crate::session::terminal::{TerminalHub, TerminalSession};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;
use tungstenite::protocol::{Role, WebSocket};

/// Short enough that a connection test ends in a fraction of a second, while
/// still leaving the reader parked between reads rather than spinning.
const GUARD: Duration = Duration::from_millis(20);

/// Short enough that a stalled write surfaces in a test instead of parking the
/// writer for the production timeout (15 s), while still far longer than any
/// loopback write here needs.
const WRITE_TIMEOUT: Duration = Duration::from_millis(50);

/// What one connection looks like before the loop starts: the page's end, the
/// writer's, the threads feeding it, and the handles the hub and a test use to
/// cut it off.
pub(super) struct Wired {
    pub(super) _dir: tempfile::TempDir,
    pub(super) hub: Arc<TerminalHub>,
    pub(super) served: WebSocket<Duplex>,
    pub(super) link: Link,
    pub(super) page: WebSocket<TcpStream>,
    pub(super) session: Arc<TerminalSession>,
    pub(super) cut_off: TcpStream,
}

pub(super) fn wired() -> Wired {
    wired_with(GUARD)
}

/// The same connection, with a reader guard the caller picks: the tests use
/// [`GUARD`], while the idle measurement uses the production value so the
/// number it reports is the one a real socket costs.
pub(super) fn wired_with(guard: Duration) -> Wired {
    let dir = tempfile::TempDir::new().expect("a temporary directory");
    let hub = TerminalHub::spawn(
        &dir.path().to_string_lossy(),
        Vec::new(),
        Vec::new(),
        false,
        crate::config::ShellConfig::default(),
        Arc::default(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let page_stream = TcpStream::connect(listener.local_addr().unwrap()).expect("a dial");
    let (served_stream, _) = listener.accept().expect("an inbound connection");
    served_stream
        .set_write_timeout(Some(WRITE_TIMEOUT))
        .expect("a write timeout");
    page_stream
        .set_read_timeout(Some(Duration::from_millis(20)))
        .expect("a read timeout");
    let cut_off = served_stream.try_clone().expect("a socket clone");
    let session = Arc::new(hub.connect(
        ViewerId::Browser("liveness-test".to_string()),
        true,
        Some(served_stream.try_clone().expect("a socket clone")),
    ));
    let link = Link::open(
        served_stream.try_clone().expect("a socket clone"),
        &session,
        guard,
    )
    .expect("the reader and frame threads");
    Wired {
        _dir: dir,
        hub,
        served: WebSocket::from_raw_socket(Duplex::new(served_stream), Role::Server, None),
        link,
        page: WebSocket::from_raw_socket(page_stream, Role::Client, None),
        session,
        cut_off,
    }
}

/// A whole liveness cycle in a fraction of a second, so a test does not sleep
/// for thirty.
pub(super) fn short() -> Timing {
    Timing {
        ping_after: Duration::from_millis(100),
        silence_limit: Duration::from_millis(400),
    }
}
