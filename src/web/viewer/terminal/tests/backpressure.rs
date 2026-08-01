//! What happens to a client that stops draining its queue.

use super::{attach_over_socket, created_pane, next_matching, spawn_hub};
use crate::web::viewer::terminal::CLIENT_QUEUE_DEPTH;
use crate::web::viewer::terminal::frame::ClientMessage;
use std::io::Read;

#[test]
fn a_client_that_stops_draining_has_its_connection_ended() {
    // Dropping it from the broadcast list is only half the disconnect the hub
    // claims to make: the connection thread is parked in its read, so without
    // this the page keeps a socket open on a panel that has silently stopped
    // being true — and never fires the `close` its reconnect hangs off.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    // `_served` stands in for the connection thread, which keeps the original
    // while the hub holds a clone: without it the hub's handle would be the only
    // one and dropping the client record alone would close the socket.
    let (session, mut peer, _served) = attach_over_socket(&hub);

    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    let pane = next_matching(&session, |f| created_pane(f).is_some())
        .and_then(|f| created_pane(&f))
        .expect("no created message");

    // Zoom toggles rather than a flood of creates: each is broadcast from this
    // thread as it is dispatched, so the queue fills a known number of frames
    // later instead of whenever a shell happens to print. Alternating, because a
    // zoom that changes nothing is deliberately not announced.
    //
    // Creates would fill it too, but with refusals once past the pane cap — and
    // those are addressed sends, so the test would pass on a path other than the
    // broadcast one it is about.
    for i in 0..CLIENT_QUEUE_DEPTH + 8 {
        let target = if i % 2 == 0 { Some(pane) } else { None };
        session.dispatch(ClientMessage::Zoom { pane: target });
    }

    peer.set_read_timeout(Some(super::SHELL_TEST_DEADLINE))
        .expect("could not bound the read");
    let mut buf = [0u8; 1];
    assert_eq!(
        peer.read(&mut buf).ok(),
        Some(0),
        "an evicted client's socket must be shut down, not merely forgotten"
    );
    hub.stop();
}
