use super::AttachedClients;
use crate::daemon::frame::Frame;
use std::io::Read;
use std::os::unix::net::UnixStream;

/// Attach a client, keeping the far end of its socket so a test can watch what
/// the daemon does to the connection.
fn attach(clients: &AttachedClients) -> (u64, std::sync::mpsc::Receiver<Frame>, UnixStream) {
    let (near, far) = UnixStream::pair().expect("a socket pair");
    // Bounded here rather than where it is read: on macOS the option cannot be
    // set once the peer has shut the connection down, which is the very state
    // these tests go on to check for.
    far.set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .expect("sets a timeout");
    let (id, rx) = clients.connect(near);
    (id, rx, far)
}

/// Whether the daemon has closed its end of `socket`. A live connection with
/// nothing on it reads as a timeout instead.
fn is_closed(socket: &mut UnixStream) -> bool {
    matches!(socket.read(&mut [0u8; 1]), Ok(0))
}

fn payloads(rx: &std::sync::mpsc::Receiver<Frame>) -> Vec<Vec<u8>> {
    rx.try_iter().map(|frame| frame.payload).collect()
}

#[test]
fn a_broadcast_reaches_every_attached_client() {
    // The reason the daemon speaks first: a change one client makes is news
    // for the others.
    let clients = AttachedClients::default();
    let (_a, rx_a, _sock_a) = attach(&clients);
    let (_b, rx_b, _sock_b) = attach(&clients);

    clients.broadcast(Frame::control(b"repos".to_vec()));

    assert_eq!(payloads(&rx_a), vec![b"repos".to_vec()]);
    assert_eq!(payloads(&rx_b), vec![b"repos".to_vec()]);
}

#[test]
fn a_refusal_goes_only_to_the_client_that_asked() {
    // "No such directory" is an answer for one client and noise for the rest.
    let clients = AttachedClients::default();
    let (asker, rx_asker, _sock_asker) = attach(&clients);
    let (_other, rx_other, _sock_other) = attach(&clients);

    clients.send_to(asker, Frame::control(b"error".to_vec()));

    assert_eq!(payloads(&rx_asker), vec![b"error".to_vec()]);
    assert!(payloads(&rx_other).is_empty());
}

#[test]
fn a_disconnected_client_stops_receiving() {
    let clients = AttachedClients::default();
    let (id, rx, _sock_id) = attach(&clients);

    clients.disconnect(id);
    clients.broadcast(Frame::control(b"repos".to_vec()));

    assert!(payloads(&rx).is_empty());
    assert_eq!(clients.len(), 0);
}

#[test]
fn addressing_a_client_that_has_gone_is_not_an_error() {
    // The client can detach between a request being read and its refusal being
    // written; that must not take the daemon down.
    let clients = AttachedClients::default();
    let (id, rx, _sock_id) = attach(&clients);
    clients.disconnect(id);
    drop(rx);

    clients.send_to(id, Frame::control(b"error".to_vec()));
}

#[test]
fn one_stalled_client_does_not_hold_up_the_others() {
    // The whole reason sends never block. A client that stops draining is cut
    // off; the rest keep receiving through it.
    let clients = AttachedClients::default();
    let (_stalled, rx_stalled, mut sock_stalled) = attach(&clients);
    let (_healthy, rx_healthy, _sock_healthy) = attach(&clients);

    for i in 0..(super::CLIENT_QUEUE_DEPTH as u32 + 10) {
        clients.broadcast(Frame::control(i.to_be_bytes().to_vec()));
        // The healthy client drains as it goes; the stalled one never does.
        let drained = payloads(&rx_healthy);
        assert_eq!(
            drained.len(),
            1,
            "the healthy client keeps receiving at broadcast {i}"
        );
    }

    assert_eq!(clients.len(), 1, "the stalled client is gone");
    assert!(
        is_closed(&mut sock_stalled),
        "and its connection was closed rather than left silently short of frames"
    );
    // Bounded rather than grown to hold everything that was sent.
    assert!(payloads(&rx_stalled).len() <= super::CLIENT_QUEUE_DEPTH);
}

#[test]
fn a_client_that_falls_behind_on_a_refusal_is_cut_off_too() {
    // Every path that queues has to make the same call: pane output shares
    // these queues, so a frame skipped anywhere leaves the client rendering a
    // stream with a hole in it.
    let clients = AttachedClients::default();
    let (id, _rx, mut sock) = attach(&clients);

    for _ in 0..(super::CLIENT_QUEUE_DEPTH + 1) {
        clients.send_to(id, Frame::control(b"error".to_vec()));
    }

    assert_eq!(clients.len(), 0);
    assert!(is_closed(&mut sock));
}

#[test]
fn client_ids_are_not_reused_after_a_disconnect() {
    // `send_to` addresses by id, so a reused id would deliver one client's
    // refusal to whoever took its place.
    let clients = AttachedClients::default();
    let (first, _rx_first, _sock_first) = attach(&clients);
    clients.disconnect(first);
    let (second, _rx_second, _sock_second) = attach(&clients);

    assert_ne!(first, second);
}
