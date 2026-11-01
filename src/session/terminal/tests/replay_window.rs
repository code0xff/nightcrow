//! The window between taking a replay snapshot and its completion marker: what
//! the hub holds back, what it lets through, and what it does to a client that
//! cannot take the replay at all.

use super::{attach, spawn_hub_with_auto_open};
use crate::session::terminal::ConcurrencyTestPoint;
use std::sync::Arc;
use std::thread;

/// A connection that unwinds between registering its client and returning the
/// session must take the registration with it. Otherwise the client sits in the
/// broadcast list behind a replay gate nothing will ever open: every later frame
/// is held until it overflows, and no session exists to release anything.
#[test]
fn a_connect_that_unwinds_leaves_no_client_behind() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub =
        spawn_hub_with_auto_open(&dir.path().to_string_lossy(), Vec::new(), Vec::new(), false);
    hub.set_concurrency_test_hook(|point| {
        if point == ConcurrencyTestPoint::ReplayFraming {
            panic!("a connect that dies mid-replay");
        }
    });

    let connecting = {
        let hub = Arc::clone(&hub);
        thread::spawn(move || {
            let died = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| attach(&hub)));
            assert!(died.is_err(), "the connection was meant to unwind");
        })
    };
    connecting
        .join()
        .expect("the panic must not escape its thread");

    assert_eq!(
        hub.client_count(),
        0,
        "an unwound connect must unregister the client it registered"
    );
    hub.stop();
}
