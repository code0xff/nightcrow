//! The seam between the reader's bytes and the WebSocket's parser.

use super::Duplex;
use std::io::{ErrorKind, Read};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

fn socket_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let peer = TcpStream::connect(listener.local_addr().unwrap()).expect("a dial");
    let (served, _) = listener.accept().expect("an inbound connection");
    (served, peer)
}

/// The parser asks for bytes before any have arrived; that is how a chunk
/// boundary looks to it, not an end of the connection.
#[test]
fn an_empty_buffer_is_a_stall_not_a_failure() {
    let (served, _peer) = socket_pair();
    let mut duplex = Duplex::new(served);

    let kind = duplex
        .read(&mut [0u8; 8])
        .expect_err("nothing has been fed")
        .kind();

    assert_eq!(kind, ErrorKind::WouldBlock);
}

/// A frame routinely straddles two socket reads, so what the parser sees must
/// be one continuous stream, not one fragment per read.
#[test]
fn fed_bytes_read_back_in_order_across_chunks() {
    let (served, _peer) = socket_pair();
    let mut duplex = Duplex::new(served);

    duplex.feed(&[1, 2, 3]);
    duplex.feed(&[4, 5]);

    let mut buf = [0u8; 3];
    assert_eq!(duplex.read(&mut buf).unwrap(), 3);
    assert_eq!(buf, [1, 2, 3]);
    assert_eq!(duplex.read(&mut buf).unwrap(), 2);
    assert_eq!(buf[..2], [4, 5]);
    assert_eq!(
        duplex.read(&mut buf).unwrap_err().kind(),
        ErrorKind::WouldBlock,
        "the second chunk is spent, and nothing more has arrived"
    );
}

#[test]
fn a_short_buffer_takes_only_what_it_can_hold() {
    let (served, _peer) = socket_pair();
    let mut duplex = Duplex::new(served);
    duplex.feed(&[7; 32]);

    let mut buf = [0u8; 4];
    assert_eq!(duplex.read(&mut buf).unwrap(), 4);
    assert_eq!(duplex.read(&mut buf).unwrap(), 4);
}

/// The same thing the hub does to a client that stopped keeping up: shutting
/// the socket down must reach the peer, or its read would wait forever.
#[test]
fn cutting_a_socket_off_ends_the_peers_read() {
    let (served, mut peer) = socket_pair();
    peer.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("a read timeout");

    Duplex::new(served).cut_off();

    let mut buf = [0u8; 1];
    assert!(
        matches!(peer.read(&mut buf), Ok(0) | Err(_)),
        "the peer must see the connection end"
    );
}
