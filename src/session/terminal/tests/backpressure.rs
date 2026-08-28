//! What happens to a client that stops draining its queue.

use super::{attach, attach_over_socket, created_pane, next_matching, spawn_hub};
use crate::session::terminal::CLIENT_QUEUE_DEPTH;
use crate::session::terminal::frame::ClientMessage;
use crate::session::terminal::hub_helpers::Command;
use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
    let ownership: std::sync::Arc<crate::session::size_owner::SizeOwnership> = Default::default();
    let hub = crate::session::terminal::TerminalHub::spawn(
        &cwd,
        Vec::new(),
        Vec::new(),
        crate::config::ShellConfig::default(),
        ownership.clone(),
    );

    let evicted = crate::session::size_owner::ViewerId::Browser("evicted".to_string());
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
    let waiting = crate::session::size_owner::ViewerId::Browser("waiting".to_string());
    let _other = hub.connect(waiting.clone(), false, None);
    drop(session);
    ownership.settle(std::time::Instant::now() + crate::session::size_owner::RELEASE_GRACE * 2);

    assert_eq!(
        ownership.owner(),
        Some(waiting),
        "an evicted client must not hold the sizing after its session ends"
    );
    hub.stop();
}

#[test]
fn the_final_resize_survives_a_full_command_queue() {
    // Stop the worker so the ordinary command queue remains deterministically
    // full. Resize has latest-value semantics and must stay independently
    // writable even then; every intermediate drag position may collapse.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    hub.stop();
    let session = attach(&hub);
    // The worker cleared its real panes while stopping; install one record so
    // dispatch still exercises the production liveness boundary.
    hub.register_pane(7, 24, 80, None, None);
    for _ in 0..CLIENT_QUEUE_DEPTH + 8 {
        session.dispatch(ClientMessage::Input {
            pane: 7,
            data: "x".to_string(),
        });
    }

    for cols in 81..=120 {
        session.dispatch(ClientMessage::Resize {
            pane: 7,
            rows: 30,
            cols,
        });
    }

    let pending = hub.take_pending_resizes();
    assert_eq!(pending.len(), 1, "intermediate sizes must be coalesced");
    assert_eq!((pending[0].rows, pending[0].cols), (30, 120));
}

#[test]
fn resize_progresses_while_command_producers_stay_busy() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let session = Arc::new(attach(&hub));
    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    let pane = next_matching(&session, |frame| created_pane(frame).is_some())
        .and_then(|frame| created_pane(&frame))
        .expect("no created message");

    let running = Arc::new(AtomicBool::new(true));
    let accepted = Arc::new(AtomicUsize::new(0));
    let producers: Vec<_> = (0..4)
        .map(|_| {
            let commands = hub.commands.clone();
            let running = Arc::clone(&running);
            let accepted = Arc::clone(&accepted);
            std::thread::spawn(move || {
                while running.load(Ordering::Acquire) {
                    if commands
                        .try_send(Command::Reorder { order: vec![pane] })
                        .is_ok()
                    {
                        accepted.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        })
        .collect();
    let busy = super::wait_for(|| {
        (accepted.load(Ordering::Relaxed) >= CLIENT_QUEUE_DEPTH * 4).then_some(())
    })
    .is_some();

    session.dispatch(ClientMessage::Resize {
        pane,
        rows: 30,
        cols: 120,
    });
    let resized = super::wait_for(|| {
        session
            .next_frame(std::time::Duration::from_millis(20))
            .and_then(|frame| super::resized_size(&frame))
            .filter(|size| *size == (30, 120))
    })
    .is_some();

    running.store(false, Ordering::Release);
    for producer in producers {
        producer.join().expect("command producer panicked");
    }
    hub.stop();
    assert!(
        busy,
        "the producer must exercise a sustained command stream"
    );
    assert!(
        resized,
        "continuous commands must not starve a pending resize"
    );
}

#[test]
fn unknown_panes_do_not_grow_the_resize_queue() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    hub.stop();
    let session = attach(&hub);

    for pane in 1..=1_000 {
        session.dispatch(ClientMessage::Resize {
            pane,
            rows: 30,
            cols: 100,
        });
    }

    assert!(hub.take_pending_resizes().is_empty());
}

#[test]
fn disconnected_connections_leave_no_pending_resizes() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    hub.stop();
    hub.register_pane(7, 24, 80, None, None);

    for cols in 81..=1_080 {
        let session = attach(&hub);
        session.dispatch(ClientMessage::Resize {
            pane: 7,
            rows: 30,
            cols,
        });
        drop(session);
    }

    assert!(
        hub.take_pending_resizes().is_empty(),
        "reconnect churn must not retain entries for dead connections"
    );
}
