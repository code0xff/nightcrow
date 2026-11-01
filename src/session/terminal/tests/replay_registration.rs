//! A connection that unwinds between registering its client and returning its
//! session: what it has to take with it. `connect` spans the whole replay between
//! those two moments, so the undo cannot live in a session that never exists.

use super::{attach, spawn_hub_with_auto_open};
use crate::config::ShellConfig;
use crate::session::size_owner::{RELEASE_GRACE, SizeOwnership, ViewerId};
use crate::session::terminal::frame::TerminalFrame;
use crate::session::terminal::hub_connect_guard::RegistrationGuard;
use crate::session::terminal::session::Client;
use crate::session::terminal::{CLIENT_QUEUE_DEPTH, ConcurrencyTestPoint, TerminalHub};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

/// A connection that unwinds after it registered with the session must give the
/// sizing back as well: the session that would have released it never exists, so
/// nothing else can. A viewer left holding the sizing with no connection behind it
/// keeps every other screen passive until the grace expires.
#[test]
fn a_guard_that_fires_after_registration_releases_the_sizing() {
    let dir = tempfile::TempDir::new().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let ownership: Arc<SizeOwnership> = Default::default();
    let hub = TerminalHub::spawn(
        &cwd,
        Vec::new(),
        Vec::new(),
        false,
        ShellConfig::default(),
        Arc::clone(&ownership),
    );
    let id = 42;
    let (tx, _rx): (mpsc::SyncSender<TerminalFrame>, _) =
        mpsc::sync_channel(CLIENT_QUEUE_DEPTH + 1);
    let (bystander, _bystander_rx): (mpsc::SyncSender<TerminalFrame>, _) =
        mpsc::sync_channel(CLIENT_QUEUE_DEPTH + 1);
    {
        let mut state = hub.state.lock().expect("terminal state poisoned");
        for (client, tx) in [(id, tx.clone()), (id + 1, bystander)] {
            state.clients.push(Client {
                id: client,
                tx,
                replay_held: Some(Vec::new()),
                socket: None,
                connection: None,
            });
        }
    }
    let registration = ownership.join(
        ViewerId::Browser("unwound".to_string()),
        true,
        tx,
        Instant::now(),
    );
    let mut guard = RegistrationGuard::new(&hub, id);
    guard.registered(registration.connection);
    assert!(
        ownership.owns(registration.connection),
        "the fixture must register a connection that owns the sizing"
    );

    drop(guard);

    assert_eq!(
        hub.client_count(),
        1,
        "only the connection that unwound must leave the broadcast list"
    );
    assert_eq!(
        hub.state.lock().expect("terminal state poisoned").clients[0].id,
        id + 1,
        "a connection that did not unwind must be left alone"
    );
    assert!(
        !ownership.owns(registration.connection),
        "and it must give its sizing back"
    );
    ownership.settle(Instant::now() + RELEASE_GRACE);
    assert!(
        ownership.owner().is_none(),
        "no viewer is left to own the sizing"
    );
    hub.stop();
}

/// A page that reloads into a hub with a large replay fails this way on every
/// attempt, not once. Twenty unwinds must leave the broadcast list as they found it:
/// one dead record per failure would accumulate for the life of the hub, and each
/// would be held behind a gate nothing will open.
#[test]
fn repeated_unwinds_leave_no_client_behind() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub =
        spawn_hub_with_auto_open(&dir.path().to_string_lossy(), Vec::new(), Vec::new(), false);
    for attempt in 0..20 {
        hub.set_concurrency_test_hook(|point| {
            if point == ConcurrencyTestPoint::ReplayFraming {
                panic!("a connect that dies mid-replay");
            }
        });
        let connecting = {
            let hub = Arc::clone(&hub);
            thread::spawn(move || {
                let died = std::panic::catch_unwind(AssertUnwindSafe(|| attach(&hub)));
                assert!(died.is_err(), "the connection was meant to unwind");
            })
        };
        connecting
            .join()
            .expect("the panic must not escape its thread");
        assert_eq!(
            hub.client_count(),
            0,
            "attempt {attempt} left a client behind"
        );
    }
    hub.stop();
}

/// The guard runs while unwinding, so the state lock a panicking thread held may
/// already be poisoned. It has to take the lock anyway: refusing would leave the
/// record behind and abort the unwind it is part of. Nothing else is left to clean
/// up after `connect` dies there.
#[test]
fn a_guard_still_removes_its_client_through_a_poisoned_lock() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub =
        spawn_hub_with_auto_open(&dir.path().to_string_lossy(), Vec::new(), Vec::new(), false);
    let id = 42;
    let (tx, _rx): (mpsc::SyncSender<TerminalFrame>, _) =
        mpsc::sync_channel(CLIENT_QUEUE_DEPTH + 1);
    hub.state
        .lock()
        .expect("the fixture must acquire the state")
        .clients
        .push(Client {
            id,
            tx,
            replay_held: Some(Vec::new()),
            socket: None,
            connection: None,
        });
    let died = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _held = hub
            .state
            .lock()
            .expect("the fixture must acquire the state");
        panic!("a thread that dies holding the terminal state");
    }));
    assert!(died.is_err(), "the fixture must poison the state lock");
    assert!(hub.state.is_poisoned(), "the lock must be poisoned");

    drop(RegistrationGuard::new(&hub, id));

    let removed = hub
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clients
        .is_empty();
    assert!(removed, "the guard must unregister through the poison");
    hub.state.clear_poison();
    hub.stop();
}
