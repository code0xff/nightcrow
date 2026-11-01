//! Which failures end a terminal socket, and which are only a stall.

use super::stalled_not_gone;

fn io(kind: std::io::ErrorKind) -> tungstenite::Error {
    tungstenite::Error::Io(std::io::Error::from(kind))
}

/// The write timeout expiring on a page that stopped reading. Ending the
/// connection there costs it every pane, replayed from scratch, for a stall
/// it would have ridden out. macOS reports one kind, Linux the other.
#[test]
fn an_operation_that_timed_out_leaves_the_connection_usable() {
    for kind in [std::io::ErrorKind::WouldBlock, std::io::ErrorKind::TimedOut] {
        assert!(
            stalled_not_gone(&io(kind)),
            "{kind:?} is a stall, not a end"
        );
    }
}

#[test]
fn a_socket_that_actually_failed_ends_the_connection() {
    for err in [
        io(std::io::ErrorKind::BrokenPipe),
        io(std::io::ErrorKind::ConnectionReset),
        tungstenite::Error::ConnectionClosed,
        tungstenite::Error::AlreadyClosed,
    ] {
        assert!(
            !stalled_not_gone(&err),
            "{err} is not something to wait out"
        );
    }
}
