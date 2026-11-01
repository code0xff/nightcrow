use super::*;
use std::sync::mpsc;

/// A worker that cannot finish before the test releases it.
fn blocked_worker() -> (thread::JoinHandle<()>, mpsc::Sender<()>) {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let _ = started_tx.send(());
        let _ = release_rx.recv();
    });
    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the blocked worker did not start");
    (handle, release_tx)
}

#[test]
fn dropping_a_pane_does_not_wait_out_its_workers_reap_window() {
    let (writer, release_writer) = blocked_worker();
    let (reader, release_reader) = blocked_worker();
    let (waiter, release_waiter) = blocked_worker();
    let pane = PtyPane {
        master: None,
        input: None,
        killer: None,
        process_tree: None,
        rx: None,
        reader_handle: Some(reader),
        writer_handle: Some(writer),
        wait_handle: Some(waiter),
        output_backpressured: Arc::new(AtomicBool::new(false)),
        exit: ExitPhase::Running,
        window: None,
    };

    let start = Instant::now();
    drop(pane);
    let elapsed = start.elapsed();
    let _ = release_writer.send(());
    let _ = release_reader.send(());
    let _ = release_waiter.send(());

    // Three blocked joins at the full window would be three times this bound.
    assert!(
        elapsed < PTY_REAP_TIMEOUT,
        "drop spent {elapsed:?} waiting on its workers instead of handing them off"
    );
}
