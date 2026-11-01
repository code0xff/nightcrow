use super::*;
use crate::config::ShellConfig;
use crate::platform::threading::WakeHandle;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// More than one pass's budget of events, then the end of the stream, queued for
/// a pane whose sender is already dropped: no reader thread is left to signal the
/// owner, so only the backend's own deadline can bring it back.
fn backlogged(backend: &mut PtyBackend, pane: PaneId, extra: usize) {
    let (tx, rx) = mpsc::sync_channel(super::super::io::OUTPUT_EVENT_CAPACITY);
    for _ in 0..(PER_PANE_DRAIN_BUDGET + extra) {
        tx.send(PtyEvent::Output(vec![b'x'])).expect("queue output");
    }
    tx.send(PtyEvent::Exited).expect("queue exit");
    drop(tx);
    backend.panes.insert(pane, queued_pane(rx));
}

fn exited(events: &[BackendEvent], pane: PaneId) -> bool {
    events
        .iter()
        .any(|event| matches!(event, BackendEvent::Exited { pane: exited } if *exited == pane))
}

#[test]
fn a_woken_owner_drains_a_spent_pane_again_only_after_its_window() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    backend.set_waker(WakeHandle::default());
    let pane = 7;
    backlogged(&mut backend, pane, 3);

    let started = Instant::now();
    let first = backend.drain_events();
    assert_eq!(
        outputs_for(&first, pane),
        PER_PANE_DRAIN_BUDGET,
        "the fixture must spend exactly one pass's budget"
    );
    let deadline = backend
        .next_deadline()
        .expect("a spent budget must name when the pane is drained again");
    assert!(
        deadline >= started + DRAIN_WINDOW && deadline <= Instant::now() + DRAIN_WINDOW,
        "the backlog is due at the end of the drain window, got {:?} after the drain",
        deadline.saturating_duration_since(started)
    );

    // A pass inside the window, as a wake from another pane would cause, leaves
    // the spent pane alone: that is what bounds a burst to one budget per window.
    if Instant::now() < deadline {
        let early = backend.drain_events();
        assert_eq!(
            outputs_for(&early, pane),
            0,
            "a pane must not be drained again inside its window"
        );
    }

    while Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    let rest = backend.drain_events();
    assert_eq!(
        outputs_for(&first, pane) + outputs_for(&rest, pane),
        PER_PANE_DRAIN_BUDGET + 3,
        "the events behind the budget must all arrive"
    );
    assert!(
        exited(&rest, pane),
        "the exit queued behind the backlog must arrive once the window ends"
    );
    assert!(
        backend.next_deadline().is_none(),
        "a drain that finished its queue must not ask for another pass"
    );
}

#[test]
fn a_woken_owner_counts_the_budget_across_passes_in_a_window() {
    // A woken owner runs a pass for every event the reader queues, so the queue
    // never builds up to a pass's budget. The budget has to bind across passes,
    // or a burst reaches the clients as fast as the PTY can produce it.
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    backend.set_waker(WakeHandle::default());
    let pane = 7;
    let (tx, rx) = mpsc::sync_channel(super::super::io::OUTPUT_EVENT_CAPACITY);
    backend.panes.insert(pane, queued_pane(rx));

    let started = Instant::now();
    let mut drained = 0;
    for _ in 0..(PER_PANE_DRAIN_BUDGET * 2) {
        tx.send(PtyEvent::Output(vec![b'x'])).expect("queue output");
        drained += outputs_for(&backend.drain_events(), pane);
    }
    if started.elapsed() < DRAIN_WINDOW {
        assert_eq!(
            drained, PER_PANE_DRAIN_BUDGET,
            "passes inside one window must share one budget"
        );
        let deadline = backend
            .next_deadline()
            .expect("a spent window must name when the pane is drained again");
        assert!(deadline <= started + DRAIN_WINDOW + Duration::from_millis(1));
    }

    while backend
        .next_deadline()
        .is_some_and(|due| Instant::now() < due)
    {
        thread::sleep(Duration::from_millis(1));
    }
    drained += outputs_for(&backend.drain_events(), pane);
    assert_eq!(
        drained,
        PER_PANE_DRAIN_BUDGET * 2,
        "every event arrives once the window has passed"
    );
}

#[test]
fn a_polling_owner_is_not_paced() {
    // Without a waker the owner comes back on its own tick, which already bounds
    // a burst, so a drain right after a spent budget takes the rest at once.
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let pane = 7;
    backlogged(&mut backend, pane, 3);

    let first = backend.drain_events();
    assert_eq!(outputs_for(&first, pane), PER_PANE_DRAIN_BUDGET);
    assert!(
        backend.next_deadline().is_none(),
        "a polling owner needs no deadline for its backlog"
    );
    let rest = backend.drain_events();
    assert_eq!(outputs_for(&rest, pane), 3);
    assert!(exited(&rest, pane));
}

fn outputs_for(events: &[BackendEvent], pane: PaneId) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, BackendEvent::Output { pane: p, .. } if *p == pane))
        .count()
}

/// A pane with a queue and nothing else: no PTY to read from, no process, and no
/// worker threads to hand to a reaper.
fn queued_pane(rx: mpsc::Receiver<PtyEvent>) -> PtyPane {
    PtyPane {
        master: None,
        input: None,
        killer: None,
        process_tree: None,
        rx: Some(rx),
        reader_handle: None,
        writer_handle: None,
        wait_handle: None,
        output_backpressured: Arc::new(AtomicBool::new(false)),
        exit: ExitPhase::Running,
        window: None,
    }
}
