//! The keepalive option, checked against a real loopback socket.

use super::{enable_keepalive, keepalive_enabled};

/// A connected pair: the accepted end is the one the viewer would hold.
fn connected_pair() -> (std::net::TcpStream, std::net::TcpStream) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("no loopback listener");
    let peer = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (accepted, _) = listener.accept().unwrap();
    (accepted, peer)
}

#[test]
fn an_accepted_socket_then_has_keepalive_on() {
    let (accepted, _peer) = connected_pair();
    assert!(
        !keepalive_enabled(&accepted).unwrap(),
        "the option must start off, or this proves nothing"
    );

    enable_keepalive(&accepted).expect("the OS refused keepalive on a plain TCP socket");

    assert!(
        keepalive_enabled(&accepted).unwrap(),
        "keepalive was requested but the socket does not have it"
    );
}

/// Enabling it twice is what happens when a route runs the seam without
/// knowing whether the accept loop already did; it must stay harmless.
#[test]
fn keepalive_can_be_enabled_again() {
    let (accepted, _peer) = connected_pair();
    enable_keepalive(&accepted).unwrap();
    enable_keepalive(&accepted).unwrap();
    assert!(keepalive_enabled(&accepted).unwrap());
}
