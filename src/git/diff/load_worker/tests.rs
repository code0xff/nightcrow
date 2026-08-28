use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::runtime::{TestHooks, WorkerTask, WorkerThread, spawn_task};
use super::*;

mod retry;

fn request(generation: u64, operation: GitLoadOperation) -> GitLoadRequest {
    GitLoadRequest {
        repo: "repo".into(),
        generation,
        operation,
    }
}

#[test]
fn 같은_lane의_대기_요청은_최신_요청_하나로_합쳐진다() {
    let mut pending = Pending::default();
    for generation in 1..=100_000 {
        pending.replace(request(
            generation,
            GitLoadOperation::StatusDiff(format!("{generation}.rs")),
        ));
    }

    let latest = pending.take_next().unwrap();
    assert_eq!(latest.generation, 100_000);
    assert!(pending.take_next().is_none());
}

#[test]
fn 서로_다른_lane의_요청은_서로를_덮어쓰지_않는다() {
    let mut pending = Pending::default();
    pending.replace(request(1, GitLoadOperation::StatusDiff("a.rs".into())));
    pending.replace(request(2, GitLoadOperation::WorkdirFile("a.rs".into())));

    assert!(pending.take_next().is_some());
    assert!(pending.take_next().is_some());
}

#[test]
fn continuously_refilled_diff_lane_cannot_starve_other_lanes() {
    let mut pending = Pending::default();
    pending.replace(request(1, GitLoadOperation::StatusDiff("a.rs".into())));
    pending.replace(request(2, GitLoadOperation::WorkdirFile("a.rs".into())));
    pending.replace(request(3, GitLoadOperation::CommitFiles(Oid::ZERO_SHA1)));
    pending.replace(request(4, GitLoadOperation::Decorations));

    let mut lanes = Vec::new();
    for generation in 5..9 {
        let next = pending.take_next().unwrap();
        lanes.push(next.operation.lane());
        pending.replace(request(
            generation,
            GitLoadOperation::StatusDiff(format!("{generation}.rs")),
        ));
    }

    assert!(lanes.contains(&LoadLane::File));
    assert!(lanes.contains(&LoadLane::CommitFiles));
    assert!(lanes.contains(&LoadLane::Decorations));
}

#[test]
fn finished_panicked_worker_restarts_and_completes_queued_request() {
    let spawn_calls = Arc::new(AtomicUsize::new(0));
    let worker = worker_with_hooks(
        {
            let spawn_calls = Arc::clone(&spawn_calls);
            move |task| {
                if spawn_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    return thread::Builder::new().spawn(move || {
                        drop(task);
                        panic!("injected worker panic");
                    });
                }
                spawn_task(task)
            }
        },
        successful_executor,
    );

    worker.submit(request(
        1,
        GitLoadOperation::WorkdirFile("after-panic.rs".into()),
    ));

    assert_file_reply(wait_for_reply(&worker), "after-panic.rs");
    assert!(spawn_calls.load(Ordering::SeqCst) >= 2);
}

#[test]
fn queued_request_completes_after_injected_spawn_failure_and_poll_retry() {
    let spawn_calls = Arc::new(AtomicUsize::new(0));
    let worker = worker_with_hooks(
        {
            let spawn_calls = Arc::clone(&spawn_calls);
            move |task| {
                if spawn_calls.fetch_add(1, Ordering::SeqCst) < 2 {
                    return Err(io::Error::other("injected spawn failure"));
                }
                spawn_task(task)
            }
        },
        successful_executor,
    );

    worker.submit(request(
        1,
        GitLoadOperation::WorkdirFile("after-spawn-failure.rs".into()),
    ));

    assert_file_reply(wait_for_reply(&worker), "after-spawn-failure.rs");
    assert!(spawn_calls.load(Ordering::SeqCst) >= 3);
}

#[test]
fn task_panic_replies_with_error_and_worker_completes_future_request() {
    let execute_calls = Arc::new(AtomicUsize::new(0));
    let spawn_calls = Arc::new(AtomicUsize::new(0));
    let worker = worker_with_hooks(
        {
            let spawn_calls = Arc::clone(&spawn_calls);
            move |task| {
                spawn_calls.fetch_add(1, Ordering::SeqCst);
                spawn_task(task)
            }
        },
        {
            let execute_calls = Arc::clone(&execute_calls);
            move |request, cached| {
                if execute_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("injected task panic");
                }
                successful_executor(request, cached)
            }
        },
    );

    worker.submit(request(
        1,
        GitLoadOperation::WorkdirFile("panics.rs".into()),
    ));
    let panic_reply = wait_for_reply(&worker);
    assert_eq!(panic_reply.request.generation, 1);
    assert_eq!(
        panic_reply.result.err().as_deref(),
        Some("background git load panicked")
    );

    worker.submit(request(
        2,
        GitLoadOperation::WorkdirFile("after-task-panic.rs".into()),
    ));
    assert_file_reply(wait_for_reply(&worker), "after-task-panic.rs");
    assert_eq!(spawn_calls.load(Ordering::SeqCst), 1);
}

fn worker_with_hooks(
    spawner: impl Fn(WorkerTask) -> io::Result<thread::JoinHandle<()>> + Send + Sync + 'static,
    executor: impl Fn(
        &GitLoadRequest,
        &mut Option<(String, git2::Repository)>,
    ) -> anyhow::Result<GitLoadPayload>
    + Send
    + Sync
    + 'static,
) -> GitLoadWorker {
    let hooks = Arc::new(TestHooks {
        spawner: Arc::new(spawner),
        executor: Arc::new(executor),
        now: Arc::new(Instant::now),
        on_warning: Arc::new(|_| {}),
    });
    GitLoadWorker::new(move |reply_tx| WorkerThread::with_hooks(reply_tx, hooks))
}

struct ManualClock {
    start: Instant,
    elapsed_ms: AtomicU64,
}

impl ManualClock {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            elapsed_ms: AtomicU64::new(0),
        }
    }

    fn now(&self) -> Instant {
        self.start + Duration::from_millis(self.elapsed_ms.load(Ordering::SeqCst))
    }

    fn advance(&self, duration: Duration) {
        let millis = u64::try_from(duration.as_millis()).expect("test duration fits u64");
        self.elapsed_ms.fetch_add(millis, Ordering::SeqCst);
    }
}

fn successful_executor(
    request: &GitLoadRequest,
    _: &mut Option<(String, git2::Repository)>,
) -> anyhow::Result<GitLoadPayload> {
    let GitLoadOperation::WorkdirFile(path) = &request.operation else {
        panic!("test executor only accepts workdir file loads");
    };
    Ok(GitLoadPayload::File(path.clone()))
}

fn wait_for_reply(worker: &GitLoadWorker) -> GitLoadReply {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match worker.try_recv() {
            Ok(reply) => return reply,
            Err(mpsc::TryRecvError::Empty) if Instant::now() < deadline => thread::yield_now(),
            Err(mpsc::TryRecvError::Empty) => panic!("worker did not reply before the deadline"),
            Err(mpsc::TryRecvError::Disconnected) => panic!("worker reply channel disconnected"),
        }
    }
}

fn assert_file_reply(reply: GitLoadReply, expected: &str) {
    match reply.result {
        Ok(GitLoadPayload::File(content)) => assert_eq!(content, expected),
        Ok(_) => panic!("worker returned the wrong payload kind"),
        Err(error) => panic!("worker returned an error: {error}"),
    }
}
