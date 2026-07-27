use super::*;

#[test]
fn a_started_job_reads_as_running() {
    let jobs = CloneJobs::default();

    let id = jobs.try_start().expect("admitted");

    assert_eq!(jobs.get(id), Some(CloneState::Running));
    assert_eq!(jobs.try_start(), None, "a running job holds the only slot");
}

#[test]
fn ids_are_distinct_and_never_zero() {
    let jobs = CloneJobs::default();

    let first = jobs.try_start().expect("admitted");
    jobs.finish(first, CloneState::Done("/tmp/x".to_string()));
    let second = jobs.try_start().expect("admitted");

    assert_ne!(first, second);
    assert_ne!(first, 0, "0 is reserved so a missing id cannot look valid");
}

#[test]
fn only_one_job_runs_at_a_time() {
    // Admission is the whole protection against a client spawning a fleet of
    // clones, so it must be refused while one is running.
    let jobs = CloneJobs::default();
    let first = jobs.try_start().expect("admitted");

    assert_eq!(jobs.try_start(), None, "a second clone must be refused");

    jobs.finish(first, CloneState::Done("/tmp/x".to_string()));
    assert!(
        jobs.try_start().is_some(),
        "the slot frees when one finishes"
    );
}

#[test]
fn finishing_replaces_the_state() {
    let jobs = CloneJobs::default();
    let id = jobs.try_start().expect("admitted");

    jobs.finish(id, CloneState::Done("/tmp/thing".to_string()));

    assert_eq!(
        jobs.get(id),
        Some(CloneState::Done("/tmp/thing".to_string()))
    );
    assert!(jobs.try_start().is_some(), "the slot is free again");
}

#[test]
fn a_failure_carries_its_message() {
    let jobs = CloneJobs::default();
    let id = jobs.try_start().expect("admitted");

    jobs.finish(id, CloneState::Failed("repository not found".to_string()));

    assert_eq!(
        jobs.get(id),
        Some(CloneState::Failed("repository not found".to_string()))
    );
    assert!(jobs.try_start().is_some(), "the slot is free again");
}

#[test]
fn the_running_job_is_reported_until_it_finishes() {
    // A client that lost its job id — a reloaded page — attaches through this,
    // so it must name the running job and go quiet once nothing is running.
    let jobs = CloneJobs::default();
    assert_eq!(jobs.running(), None, "nothing runs on a fresh registry");
    let id = jobs.try_start().expect("admitted");

    assert_eq!(jobs.running(), Some(id));

    jobs.finish(id, CloneState::Done("/tmp/x".to_string()));
    assert_eq!(
        jobs.running(),
        None,
        "a finished job is no longer something to attach to"
    );
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
fn eviction_bounds_the_map_without_dropping_the_job_it_just_admitted() {
    let jobs = CloneJobs::default();
    // Fill past the retention cap. Admission allows this because each job
    // finishes before the next starts.
    for _ in 0..MAX_RETAINED_JOBS + 4 {
        let id = jobs.try_start().expect("admitted");
        jobs.finish(id, CloneState::Done("/tmp/x".to_string()));
    }

    let latest = jobs.try_start().expect("admitted");

    assert_eq!(
        jobs.get(latest),
        Some(CloneState::Running),
        "the job admitted alongside an eviction must survive it"
    );
}
