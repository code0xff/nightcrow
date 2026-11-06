use super::{
    CloneJobState, CloneJobs, CloneLookup, CloneService, CloneStartError, MAX_RETAINED_JOBS,
};
use std::path::Path;
use std::time::{Duration, Instant};

fn succeeds(_url: &str, dest: &Path) -> Result<(), String> {
    std::fs::write(dest.join("README.md"), "cloned").map_err(|error| error.to_string())
}

fn await_status(service: &CloneService, job: u64) -> CloneLookup {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = service.lookup(Some(job));
        if !matches!(
            status,
            CloneLookup::Found {
                state: CloneJobState::Running,
                ..
            }
        ) {
            return status;
        }
        assert!(Instant::now() < deadline, "clone worker did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn clone_worker_records_success_at_the_canonical_destination() {
    let service = CloneService::with_runner(succeeds);
    let parent = tempfile::TempDir::new().unwrap();

    let started = service
        .start(
            parent.path().to_str().unwrap(),
            "https://example.com/team/thing.git",
        )
        .expect("valid clone starts");

    assert_eq!(started.name, "thing");
    let expected_path =
        crate::platform::paths::for_display(&parent.path().canonicalize().unwrap().join("thing"))
            .into_owned();
    assert_eq!(
        await_status(&service, started.job),
        CloneLookup::Found {
            job: started.job,
            state: CloneJobState::Done {
                path: expected_path
            },
        }
    );
    assert_eq!(
        std::fs::read_to_string(parent.path().join("thing/README.md")).unwrap(),
        "cloned"
    );
}

#[test]
fn bad_urls_missing_parents_and_existing_destinations_are_refused() {
    let service = CloneService::with_runner(succeeds);
    let parent = tempfile::TempDir::new().unwrap();

    assert!(matches!(
        service.start(parent.path().to_str().unwrap(), "ext::echo bad"),
        Err(CloneStartError::InvalidUrl(_))
    ));
    assert_eq!(
        service.start(
            "/this/path/does/not/exist",
            "https://example.com/team/repo.git"
        ),
        Err(CloneStartError::MissingParent)
    );
    std::fs::create_dir(parent.path().join("repo")).unwrap();
    assert_eq!(
        service.start(
            parent.path().to_str().unwrap(),
            "https://example.com/team/repo.git"
        ),
        Err(CloneStartError::ExistingDestination)
    );
}

#[test]
fn the_running_slot_is_shared_and_completed_jobs_are_bounded() {
    let jobs = CloneJobs::default();
    let running = jobs.try_start().unwrap();
    assert_eq!(
        jobs.lookup(None),
        CloneLookup::Found {
            job: running,
            state: CloneJobState::Running,
        }
    );
    assert!(jobs.try_start().is_none());
    jobs.finish(
        running,
        CloneJobState::Failed {
            message: "retry".into(),
        },
    );

    let mut ids = vec![running];
    for _ in 0..MAX_RETAINED_JOBS + 4 {
        let id = jobs.try_start().unwrap();
        jobs.finish(
            id,
            CloneJobState::Done {
                path: "/tmp/repo".into(),
            },
        );
        ids.push(id);
    }
    let latest = *ids.last().unwrap();
    assert!(matches!(
        jobs.lookup(Some(latest)),
        CloneLookup::Found { .. }
    ));
    assert!(matches!(
        jobs.lookup(Some(ids[0])),
        CloneLookup::Unknown { .. }
    ));
}

#[test]
fn an_unknown_job_stays_distinct_from_an_idle_session() {
    let jobs = CloneJobs::default();

    assert_eq!(jobs.lookup(None), CloneLookup::Idle);
    assert_eq!(jobs.lookup(Some(42)), CloneLookup::Unknown { job: 42 });
}

#[test]
fn running_discovery_cannot_recover_a_completed_job_without_its_id() {
    let jobs = CloneJobs::default();
    let id = jobs.try_start().unwrap();

    assert!(
        matches!(jobs.lookup(None), CloneLookup::Found { job, state: CloneJobState::Running } if job == id)
    );
    jobs.finish(
        id,
        CloneJobState::Done {
            path: "/tmp/repo".into(),
        },
    );

    assert_eq!(jobs.lookup(None), CloneLookup::Idle);
    assert_eq!(
        jobs.lookup(Some(id)),
        CloneLookup::Found {
            job: id,
            state: CloneJobState::Done {
                path: "/tmp/repo".into(),
            },
        }
    );
}
