use super::*;
use crate::config::ShellConfig;
use std::sync::mpsc;

/// More than one pass's budget of events, then the end of the stream. Nothing
/// follows them: the queue is the test's own, and its sender is dropped, so no
/// reader thread is left to signal the owner.
#[test]
fn a_drain_that_spends_its_budget_asks_to_be_run_again() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let pane = 7;
    let (tx, rx) = mpsc::sync_channel(super::super::io::OUTPUT_EVENT_CAPACITY);
    for _ in 0..(PER_PANE_DRAIN_BUDGET + 3) {
        tx.send(PtyEvent::Output(vec![b'x'])).expect("queue output");
    }
    tx.send(PtyEvent::Exited).expect("queue exit");
    drop(tx);
    backend.panes.insert(pane, queued_pane(rx));

    let first = backend.drain_events();
    assert_eq!(
        outputs_for(&first, pane),
        PER_PANE_DRAIN_BUDGET,
        "the fixture must spend exactly one pass's budget"
    );
    let deadline = backend
        .next_deadline()
        .expect("a spent budget must ask for the next pass at once");
    assert!(
        deadline <= Instant::now(),
        "the backlog is due immediately, got {deadline:?}"
    );

    let rest = backend.drain_events();
    assert_eq!(
        outputs_for(&first, pane) + outputs_for(&rest, pane),
        PER_PANE_DRAIN_BUDGET + 3,
        "the events behind the budget must all arrive"
    );
    assert!(
        rest.iter()
            .any(|event| matches!(event, BackendEvent::Exited { pane: exited } if *exited == pane)),
        "the exit queued behind the backlog must arrive on the next pass"
    );
    assert!(
        backend.next_deadline().is_none(),
        "a drain that finished its queue must not ask for another pass"
    );
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
    }
}
