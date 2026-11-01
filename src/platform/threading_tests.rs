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
