use super::AttachedClients;
use crate::daemon::frame::Frame;

fn payloads(rx: &std::sync::mpsc::Receiver<Frame>) -> Vec<Vec<u8>> {
    rx.try_iter().map(|frame| frame.payload).collect()
}

#[test]
fn a_broadcast_reaches_every_attached_client() {
    // The reason the daemon speaks first: a change one client makes is news
    // for the others.
    let clients = AttachedClients::default();
    let (_a, rx_a) = clients.connect();
    let (_b, rx_b) = clients.connect();

    clients.broadcast(Frame::control(b"repos".to_vec()));

    assert_eq!(payloads(&rx_a), vec![b"repos".to_vec()]);
    assert_eq!(payloads(&rx_b), vec![b"repos".to_vec()]);
}

#[test]
fn a_refusal_goes_only_to_the_client_that_asked() {
    // "No such directory" is an answer for one client and noise for the rest.
    let clients = AttachedClients::default();
    let (asker, rx_asker) = clients.connect();
    let (_other, rx_other) = clients.connect();

    clients.send_to(asker, Frame::control(b"error".to_vec()));

    assert_eq!(payloads(&rx_asker), vec![b"error".to_vec()]);
    assert!(payloads(&rx_other).is_empty());
}

#[test]
fn a_disconnected_client_stops_receiving() {
    let clients = AttachedClients::default();
    let (id, rx) = clients.connect();

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
    let (id, rx) = clients.connect();
    clients.disconnect(id);
    drop(rx);

    clients.send_to(id, Frame::control(b"error".to_vec()));
}

#[test]
fn one_stalled_client_does_not_hold_up_the_others() {
    // The whole reason sends never block. A client that stops draining fills
    // its queue; every frame after that is dropped for it alone.
    let clients = AttachedClients::default();
    let (_stalled, rx_stalled) = clients.connect();
    let (_healthy, rx_healthy) = clients.connect();

    for i in 0..200u32 {
        clients.broadcast(Frame::control(i.to_be_bytes().to_vec()));
        // The healthy client drains as it goes; the stalled one never does.
        let drained = payloads(&rx_healthy);
        assert_eq!(
            drained.len(),
            1,
            "the healthy client keeps receiving at broadcast {i}"
        );
    }
    // The stalled client's queue is bounded rather than growing to 200.
    assert!(payloads(&rx_stalled).len() < 200);
}

#[test]
fn client_ids_are_not_reused_after_a_disconnect() {
    // `send_to` addresses by id, so a reused id would deliver one client's
    // refusal to whoever took its place.
    let clients = AttachedClients::default();
    let (first, _rx_first) = clients.connect();
    clients.disconnect(first);
    let (second, _rx_second) = clients.connect();

    assert_ne!(first, second);
}
