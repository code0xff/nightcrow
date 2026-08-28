use super::{SHELL_TEST_DEADLINE, next_matching, resized_size, spawn_hub};
use crate::backend::PtyBackend;
use crate::config::ShellConfig;
use crate::session::size_owner::ViewerId;
use crate::session::terminal::ConcurrencyTestPoint;
use crate::session::terminal::frame::ClientMessage;
use crate::session::terminal::hub_modes::PaneModeTracker;
use std::sync::{Arc, Barrier, mpsc};

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
