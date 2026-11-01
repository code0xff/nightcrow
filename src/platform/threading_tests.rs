use super::*;
use std::sync::mpsc;

#[test]
fn a_reaper_runs_its_work_without_holding_the_caller() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();

    let start = Instant::now();
    spawn_reaper("test-reaper", move || {
        let _ = started_tx.send(());
        // Runs inline only when the thread could not start; the elapsed check
        // below is what tells the two apart.
        let _ = release_rx.recv_timeout(Duration::from_secs(5));
        let _ = done_tx.send(());
    });
    let handed_off = start.elapsed();
    assert!(
        handed_off < Duration::from_secs(1),
        "spawn_reaper waited {handed_off:?} for its own job"
    );

    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the reaper never started");
    release_tx.send(()).expect("the reaper went away");
    done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the reaper did not finish its job");
}

#[test]
fn a_signal_wakes_a_parked_wait_without_a_deadline() {
    let wake = WakeHandle::default();
    let signal = Arc::clone(&wake);
    let started = Instant::now();
    let producer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        signal.wake();
    });
    wake.wait(None, Duration::from_secs(5));
    let waited = started.elapsed();
    producer.join().expect("producer");
    assert!(
        waited < Duration::from_secs(1),
        "the wait ignored its signal for {waited:?}"
    );
}

#[test]
fn a_signal_that_arrives_before_the_wait_is_not_lost() {
    let wake = WakeHandle::default();
    wake.wake();
    let started = Instant::now();
    wake.wait(None, Duration::from_secs(5));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "a signal queued before the wait must return it at once"
    );

    // The pass that signal bought is all it buys: the next wait has nothing
    // pending and sits out its own deadline.
    let started = Instant::now();
    wake.wait(
        Some(Instant::now() + Duration::from_millis(30)),
        Duration::from_secs(5),
    );
    let waited = started.elapsed();
    // Not the full 30 ms: Windows timer granularity can end a timed wait early.
    // Returning at once would still be well under this.
    assert!(
        waited >= Duration::from_millis(10),
        "a spent signal must not satisfy a second wait, waited {waited:?}"
    );
}

#[test]
fn an_expired_deadline_returns_at_once_and_the_cap_bounds_every_wait() {
    let wake = WakeHandle::default();
    let started = Instant::now();
    wake.wait(
        Some(Instant::now() - Duration::from_secs(1)),
        Duration::from_secs(5),
    );
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "a deadline in the past must not park the caller"
    );

    let started = Instant::now();
    wake.wait(
        Some(Instant::now() + Duration::from_secs(30)),
        Duration::from_millis(20),
    );
    let waited = started.elapsed();
    // The lower bound is loose because Windows timer granularity can end a timed
    // condvar wait a few milliseconds early.
    assert!(
        (Duration::from_millis(5)..Duration::from_secs(1)).contains(&waited),
        "the cap must shorten a later deadline, waited {waited:?}"
    );
}
