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

#[test]
fn an_evicted_client_still_releases_the_sizing_when_its_session_ends() {
    // Its record is gone from the broadcast list by the time the session drops,
    // so reading the size-ownership registration back out of that list found
    // nothing and released nothing. The viewer then stayed present for good: it
    // held the sizing after its page was closed, and no other screen could take
    // it back, because the grace that hands it on only starts once its last
    // connection leaves.
    let dir = tempfile::TempDir::new().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let ownership: std::sync::Arc<crate::web::viewer::size_owner::SizeOwnership> =
        Default::default();
    let hub = crate::web::viewer::terminal::TerminalHub::spawn(
        &cwd,
        Vec::new(),
        Vec::new(),
        crate::config::ShellConfig::default(),
        ownership.clone(),
    );

    let evicted = crate::web::viewer::size_owner::ViewerId::Browser("evicted".to_string());
    let session = hub.connect(evicted.clone(), true, None);
    assert_eq!(ownership.owner(), Some(evicted.clone()), "it arrived last");

    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    let pane = next_matching(&session, |f| created_pane(f).is_some())
        .and_then(|f| created_pane(&f))
        .expect("no created message");
    for i in 0..CLIENT_QUEUE_DEPTH + 8 {
        let target = if i % 2 == 0 { Some(pane) } else { None };
        session.dispatch(ClientMessage::Zoom { pane: target });
    }

    // Whoever is left takes the sizing once the departed owner's grace runs out.
    let waiting = crate::web::viewer::size_owner::ViewerId::Browser("waiting".to_string());
    let _other = hub.connect(waiting.clone(), false, None);
    drop(session);
    ownership.settle(std::time::Instant::now() + crate::web::viewer::size_owner::RELEASE_GRACE * 2);

    assert_eq!(
        ownership.owner(),
        Some(waiting),
        "an evicted client must not hold the sizing after its session ends"
    );
    hub.stop();
}
