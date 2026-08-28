use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::super::runtime::{TestHooks, WorkerTask, WorkerThread, spawn_task};
use super::super::*;
use super::{ManualClock, assert_file_reply, request, successful_executor, wait_for_reply};

#[test]
fn transient_spawn_failures_recover_after_bounded_backoff() {
    let clock = Arc::new(ManualClock::new());
    let spawn_calls = Arc::new(AtomicUsize::new(0));
    let worker = controlled_worker(
        Arc::clone(&clock),
        {
            let spawn_calls = Arc::clone(&spawn_calls);
            move |task| {
                if spawn_calls.fetch_add(1, Ordering::SeqCst) < 2 {
                    return Err(io::Error::other("injected spawn failure"));
                }
                spawn_task(task)
            }
        },
        |_| {},
    );

    worker.submit(request(
        1,
        GitLoadOperation::WorkdirFile("after-backoff.rs".into()),
    ));
    assert_eq!(spawn_calls.load(Ordering::SeqCst), 1);

    clock.advance(Duration::from_millis(15));
    assert!(matches!(worker.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert_eq!(spawn_calls.load(Ordering::SeqCst), 1);

    clock.advance(Duration::from_millis(1));
    assert!(matches!(worker.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert_eq!(spawn_calls.load(Ordering::SeqCst), 2);

    clock.advance(Duration::from_millis(31));
    assert!(matches!(worker.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert_eq!(spawn_calls.load(Ordering::SeqCst), 2);

    clock.advance(Duration::from_millis(1));
    assert_file_reply(wait_for_reply(&worker), "after-backoff.rs");
    assert_eq!(spawn_calls.load(Ordering::SeqCst), 3);
}

#[test]
fn sustained_spawn_failure_bounds_retries_and_warnings() {
    let clock = Arc::new(ManualClock::new());
    let spawn_calls = Arc::new(AtomicUsize::new(0));
    let warning_calls = Arc::new(AtomicUsize::new(0));
    let worker = controlled_worker(
        Arc::clone(&clock),
        {
            let spawn_calls = Arc::clone(&spawn_calls);
            move |_| {
                spawn_calls.fetch_add(1, Ordering::SeqCst);
                Err(io::Error::other("persistent spawn failure"))
            }
        },
        {
            let warning_calls = Arc::clone(&warning_calls);
            move |_| {
                warning_calls.fetch_add(1, Ordering::SeqCst);
            }
        },
    );

    worker.submit(request(
        1,
        GitLoadOperation::WorkdirFile("still-pending.rs".into()),
    ));
    for _ in 0..3_750 {
        clock.advance(Duration::from_millis(16));
        assert!(matches!(worker.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }

    assert_eq!(spawn_calls.load(Ordering::SeqCst), 65);
    assert_eq!(warning_calls.load(Ordering::SeqCst), 2);
}

#[test]
fn successful_spawn_resets_backoff_and_warning_window() {
    let clock = Arc::new(ManualClock::new());
    let spawn_calls = Arc::new(AtomicUsize::new(0));
    let warning_calls = Arc::new(AtomicUsize::new(0));
    let successful_worker_exited = Arc::new(AtomicBool::new(false));
    let worker = controlled_worker(
        Arc::clone(&clock),
        {
            let spawn_calls = Arc::clone(&spawn_calls);
            let successful_worker_exited = Arc::clone(&successful_worker_exited);
            move |task| match spawn_calls.fetch_add(1, Ordering::SeqCst) {
                0 | 2 => Err(io::Error::other("injected spawn failure")),
                1 => {
                    let successful_worker_exited = Arc::clone(&successful_worker_exited);
                    thread::Builder::new().spawn(move || {
                        drop(task);
                        successful_worker_exited.store(true, Ordering::SeqCst);
                    })
                }
                _ => spawn_task(task),
            }
        },
        {
            let warning_calls = Arc::clone(&warning_calls);
            move |_| {
                warning_calls.fetch_add(1, Ordering::SeqCst);
            }
        },
    );

    worker.submit(request(
        1,
        GitLoadOperation::WorkdirFile("after-reset.rs".into()),
    ));
    clock.advance(Duration::from_millis(16));
    assert!(matches!(worker.try_recv(), Err(mpsc::TryRecvError::Empty)));
    while !successful_worker_exited.load(Ordering::SeqCst) {
        thread::yield_now();
    }

    let deadline = Instant::now() + Duration::from_secs(1);
    while spawn_calls.load(Ordering::SeqCst) < 3 && Instant::now() < deadline {
        assert!(matches!(worker.try_recv(), Err(mpsc::TryRecvError::Empty)));
        thread::yield_now();
    }
    assert_eq!(spawn_calls.load(Ordering::SeqCst), 3);
    assert_eq!(warning_calls.load(Ordering::SeqCst), 2);

    clock.advance(Duration::from_millis(15));
    assert!(matches!(worker.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert_eq!(spawn_calls.load(Ordering::SeqCst), 3);

    clock.advance(Duration::from_millis(1));
    assert_file_reply(wait_for_reply(&worker), "after-reset.rs");
    assert_eq!(spawn_calls.load(Ordering::SeqCst), 4);
}

fn controlled_worker(
    clock: Arc<ManualClock>,
    spawner: impl Fn(WorkerTask) -> io::Result<thread::JoinHandle<()>> + Send + Sync + 'static,
    on_warning: impl Fn(&io::Error) + Send + Sync + 'static,
) -> GitLoadWorker {
    let hooks = Arc::new(TestHooks {
        spawner: Arc::new(spawner),
        executor: Arc::new(successful_executor),
        now: Arc::new(move || clock.now()),
        on_warning: Arc::new(on_warning),
    });
    GitLoadWorker::new(move |reply_tx| WorkerThread::with_hooks(reply_tx, hooks))
}
