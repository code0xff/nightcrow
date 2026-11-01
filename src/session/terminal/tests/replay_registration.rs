//! A connection that unwinds between registering its client and returning its
//! session: what it has to take with it. `connect` spans the whole replay between
//! those two moments, so the undo cannot live in a session that never exists.

use crate::config::ShellConfig;
use crate::session::size_owner::{RELEASE_GRACE, SizeOwnership, ViewerId};
use crate::session::terminal::frame::TerminalFrame;
use crate::session::terminal::hub_connect_guard::RegistrationGuard;
use crate::session::terminal::session::Client;
use crate::session::terminal::{CLIENT_QUEUE_DEPTH, TerminalHub};
use std::sync::Arc;
use std::sync::mpsc;
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
    hub.state
        .lock()
        .expect("terminal state poisoned")
        .clients
        .push(Client {
            id,
            tx: tx.clone(),
            replay_held: Some(Vec::new()),
            socket: None,
            connection: None,
        });
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
        0,
        "an unwound connection must leave the broadcast list"
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
