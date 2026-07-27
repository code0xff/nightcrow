use super::*;

#[test]
fn a_started_job_reads_as_running() {
    let jobs = CloneJobs::default();

    let id = jobs.start();

    assert_eq!(jobs.get(id), Some(CloneState::Running));
    assert!(jobs.any_running());
}

#[test]
fn ids_are_distinct_and_never_zero() {
    let jobs = CloneJobs::default();

    let first = jobs.start();
    let second = jobs.start();

    assert_ne!(first, second);
    assert_ne!(first, 0, "0 is reserved so a missing id cannot look valid");
}

#[test]
fn finishing_replaces_the_state() {
    let jobs = CloneJobs::default();
    let id = jobs.start();

    jobs.finish(id, CloneState::Done("/tmp/thing".to_string()));

    assert_eq!(
        jobs.get(id),
        Some(CloneState::Done("/tmp/thing".to_string()))
    );
    assert!(!jobs.any_running());
}

#[test]
fn a_failure_carries_its_message() {
    let jobs = CloneJobs::default();
    let id = jobs.start();

    jobs.finish(id, CloneState::Failed("repository not found".to_string()));

    assert_eq!(
        jobs.get(id),
        Some(CloneState::Failed("repository not found".to_string()))
    );
    assert!(!jobs.any_running());
}

#[test]
fn an_unknown_id_reads_as_missing() {
    let jobs = CloneJobs::default();

    assert_eq!(jobs.get(999), None);
}

#[test]
fn finishing_an_unknown_id_does_not_create_it() {
    let jobs = CloneJobs::default();

    jobs.finish(999, CloneState::Done("/tmp/x".to_string()));

    assert_eq!(jobs.get(999), None);
}

#[test]
fn finished_jobs_are_evicted_but_running_ones_survive() {
    let jobs = CloneJobs::default();
    let running = jobs.start();
    // Fill past the retention cap with jobs that have all finished.
    for _ in 0..MAX_RETAINED_JOBS + 4 {
        let id = jobs.start();
        jobs.finish(id, CloneState::Done("/tmp/x".to_string()));
    }

    // The running job still owns its id, so its thread can still report into it.
    assert_eq!(jobs.get(running), Some(CloneState::Running));
}
