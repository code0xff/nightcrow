use super::{SHELL_TEST_DEADLINE, next_matching, resized_size, spawn_hub};
use crate::backend::PtyBackend;
use crate::config::ShellConfig;
use crate::session::size_owner::ViewerId;
use crate::session::terminal::ConcurrencyTestPoint;
use crate::session::terminal::frame::ClientMessage;
use crate::session::terminal::hub_modes::PaneModeTracker;
use std::sync::{Arc, Barrier, mpsc};

const QUIET: std::time::Duration = std::time::Duration::from_millis(100);

fn owned(frame: &crate::session::terminal::TerminalFrame) -> Option<bool> {
    let crate::session::terminal::TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    (value["type"] == "size_owner")
        .then(|| value["owned"].as_bool())
        .flatten()
}

#[test]
fn a_taken_resize_linearizes_before_a_racing_disconnect() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    hub.stop();
    let viewer = ViewerId::Browser("resize-race".to_string());
    let old_owner = hub.connect(viewer.clone(), true, None);
    let observer = hub.connect(viewer, false, None);
    hub.register_pane(7, 24, 80, None, None);

    old_owner.dispatch(ClientMessage::Resize {
        pane: 7,
        rows: 24,
        cols: 80,
    });
    let resize = hub
        .take_pending_resizes()
        .pop()
        .expect("the worker must have taken the old request");

    let (events_tx, events_rx) = mpsc::channel();
    let release_resize = Arc::new(Barrier::new(2));
    let hook_release = Arc::clone(&release_resize);
    hub.set_concurrency_test_hook(move |point| {
        let _ = events_tx.send(point);
        if point == ConcurrencyTestPoint::BeforeResizeValidation {
            hook_release.wait();
        }
    });

    let resizing_hub = Arc::clone(&hub);
    let backend_dir = dir.path().to_path_buf();
    let resizing = std::thread::spawn(move || {
        let mut backend = PtyBackend::new(&backend_dir, ShellConfig::default());
        let mut modes = PaneModeTracker::default();
        resizing_hub.resize_pane(&mut backend, &mut modes, resize);
    });
    assert_eq!(
        events_rx.recv_timeout(SHELL_TEST_DEADLINE).unwrap(),
        ConcurrencyTestPoint::BeforeResizeValidation
    );

    let disconnecting = std::thread::spawn(move || drop(old_owner));
    assert_eq!(
        events_rx.recv_timeout(SHELL_TEST_DEADLINE).unwrap(),
        ConcurrencyTestPoint::DisconnectStateContended,
        "disconnect must wait behind the resize's registration and ownership check"
    );
    release_resize.wait();
    resizing.join().expect("resize thread panicked");
    disconnecting.join().expect("disconnect thread panicked");

    let applied = next_matching(&observer, |frame| resized_size(frame).is_some())
        .and_then(|frame| resized_size(&frame));
    assert_eq!(applied, Some((24, 80)));
}

#[test]
fn an_old_resize_is_rejected_after_ownership_returns_to_the_same_viewer() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    hub.stop();
    let a = hub.connect(ViewerId::Browser("a".into()), true, None);
    let b = hub.connect(ViewerId::Browser("b".into()), true, None);
    hub.register_pane(7, 24, 80, None, None);
    a.dispatch(ClientMessage::Resize {
        pane: 7,
        rows: 24,
        cols: 80,
    });
    let stale = hub.take_pending_resizes().pop().unwrap();

    b.dispatch(ClientMessage::ClaimSize);
    a.dispatch(ClientMessage::ClaimSize);
    let mut backend = PtyBackend::new(dir.path(), ShellConfig::default());
    hub.resize_pane(&mut backend, &mut PaneModeTracker::default(), stale);

    while let Some(frame) = a.next_frame(QUIET) {
        assert!(
            resized_size(&frame).is_none(),
            "A → B → A must not make A's old queued resize current again"
        );
    }
}

#[test]
fn a_claim_waits_until_an_admitted_resize_finishes_applying_and_broadcasting() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    hub.stop();
    let a = hub.connect(ViewerId::Browser("a".into()), true, None);
    let b_viewer = ViewerId::Browser("b".into());
    let b = hub.connect(b_viewer.clone(), true, None);
    let b_claim = hub.connect(b_viewer, false, None);
    hub.register_pane(7, 24, 80, None, None);
    a.dispatch(ClientMessage::Resize {
        pane: 7,
        rows: 24,
        cols: 80,
    });
    let resize = hub.take_pending_resizes().pop().unwrap();

    let (events_tx, events_rx) = mpsc::channel();
    let release_resize = Arc::new(Barrier::new(2));
    let hook_release = Arc::clone(&release_resize);
    let release_broadcast = Arc::new(Barrier::new(2));
    let hook_broadcast_release = Arc::clone(&release_broadcast);
    hub.set_concurrency_test_hook(move |point| match point {
        ConcurrencyTestPoint::ResizeOwnershipLocked => {
            let _ = events_tx.send(point);
            hook_release.wait();
        }
        ConcurrencyTestPoint::ResizeBroadcastComplete => {
            let _ = events_tx.send(point);
            hook_broadcast_release.wait();
        }
        _ => {}
    });
    let resizing_hub = Arc::clone(&hub);
    let backend_dir = dir.path().to_path_buf();
    let resizing = std::thread::spawn(move || {
        let mut backend = PtyBackend::new(&backend_dir, ShellConfig::default());
        resizing_hub.resize_pane(&mut backend, &mut PaneModeTracker::default(), resize);
    });
    assert_eq!(
        events_rx.recv_timeout(SHELL_TEST_DEADLINE).unwrap(),
        ConcurrencyTestPoint::ResizeOwnershipLocked
    );
    assert!(hub.ownership.test_lock_is_held());

    let (started_tx, started_rx) = mpsc::channel();
    let (claim_done_tx, claim_done_rx) = mpsc::channel();
    let claiming = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        b_claim.dispatch(ClientMessage::ClaimSize);
        claim_done_tx.send(()).unwrap();
    });
    started_rx.recv_timeout(SHELL_TEST_DEADLINE).unwrap();
    assert!(
        claim_done_rx.recv_timeout(QUIET).is_err(),
        "the claim must wait while the admitted resize still owns the lock"
    );
    release_resize.wait();
    assert_eq!(
        events_rx.recv_timeout(SHELL_TEST_DEADLINE).unwrap(),
        ConcurrencyTestPoint::ResizeBroadcastComplete
    );
    assert!(hub.ownership.test_lock_is_held());
    assert!(claim_done_rx.try_recv().is_err());
    assert!(
        next_matching(&b, |frame| resized_size(frame).is_some()).is_some(),
        "the resize reaches clients before the waiting claim proceeds"
    );
    release_broadcast.wait();
    resizing.join().unwrap();
    claim_done_rx.recv_timeout(SHELL_TEST_DEADLINE).unwrap();
    claiming.join().unwrap();

    assert!(
        next_matching(&b, |frame| owned(frame) == Some(true)).is_some(),
        "the waiting claim is eventually acknowledged"
    );
}
