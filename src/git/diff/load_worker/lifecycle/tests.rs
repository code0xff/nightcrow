use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use super::{AdmissionLimiter, MAX_IN_FLIGHT, MAX_WORKER_THREADS, WorkerSlots, finish_or_detach};

#[test]
fn retiring_a_slow_worker_never_waits_for_its_completion() {
    let slots = Arc::new(WorkerSlots::new());
    let release = Arc::new(AtomicBool::new(false));
    let release_later = Arc::clone(&release);
    let releaser = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        release_later.store(true, Ordering::SeqCst);
    });

    let mut capped_retire_elapsed = Duration::ZERO;
    for index in 0..MAX_WORKER_THREADS {
        let slots = Arc::clone(&slots);
        let release = Arc::clone(&release);
        let (ready_tx, ready_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let _permit = slots.try_acquire().expect("worker slot available");
            ready_tx.send(()).unwrap();
            while !release.load(Ordering::SeqCst) {
                thread::yield_now();
            }
        });
        ready_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let started = Instant::now();
        finish_or_detach(handle);
        if index == 16 {
            capped_retire_elapsed = started.elapsed();
        }
    }

    assert!(capped_retire_elapsed < Duration::from_millis(50));
    assert!(slots.try_acquire().is_none());
    assert_eq!(slots.state.lock().unwrap().peak, MAX_WORKER_THREADS);
    releaser.join().unwrap();
    wait_until_worker_slots_are_free(&slots);
}

#[test]
fn ninth_repo_enters_before_eight_repos_can_refill() {
    let limiter = Arc::new(AdmissionLimiter::new());
    let start = Arc::new(Barrier::new(MAX_IN_FLIGHT + 1));
    let release = Arc::new(Barrier::new(MAX_IN_FLIGHT + 1));
    let (initial_tx, initial_rx) = mpsc::channel();
    let (refill_tx, refill_rx) = mpsc::channel();
    let mut handles = Vec::new();

    for index in 0..MAX_IN_FLIGHT {
        let limiter = Arc::clone(&limiter);
        let start = Arc::clone(&start);
        let release = Arc::clone(&release);
        let initial_tx = initial_tx.clone();
        let refill_tx = refill_tx.clone();
        handles.push(thread::spawn(move || {
            let repo = format!("repo-{index}");
            start.wait();
            let permit = limiter.acquire(&repo, || false).unwrap();
            initial_tx.send(permit.admission_for_test()).unwrap();
            release.wait();
            drop(permit);
            let refill = limiter.acquire(&repo, || false).unwrap();
            refill_tx.send(refill.admission_for_test()).unwrap();
        }));
    }
    start.wait();
    let mut initial: Vec<_> = (0..MAX_IN_FLIGHT)
        .map(|_| initial_rx.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect();
    initial.sort_unstable();
    assert_eq!(initial, (1..=MAX_IN_FLIGHT as u64).collect::<Vec<_>>());

    let ninth_limiter = Arc::clone(&limiter);
    let (ninth_tx, ninth_rx) = mpsc::channel();
    let ninth = thread::spawn(move || {
        let permit = ninth_limiter.acquire("repo-8", || false).unwrap();
        ninth_tx.send(permit.admission_for_test()).unwrap();
    });
    wait_until_queued(&limiter, "repo-8");
    release.wait();

    assert_eq!(ninth_rx.recv_timeout(Duration::from_secs(1)).unwrap(), 9);
    let refills: Vec<_> = (0..MAX_IN_FLIGHT)
        .map(|_| refill_rx.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect();
    assert!(refills.into_iter().all(|admission| admission > 9));
    ninth.join().unwrap();
    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn cancelled_ticket_leaves_the_admission_queue() {
    let limiter = Arc::new(AdmissionLimiter::new());
    let mut holders = Vec::new();
    for index in 0..MAX_IN_FLIGHT {
        holders.push(
            limiter
                .acquire(&format!("holder-{index}"), || false)
                .unwrap(),
        );
    }
    let stopped = Arc::new(AtomicBool::new(false));
    let waiter_limiter = Arc::clone(&limiter);
    let waiter_stopped = Arc::clone(&stopped);
    let waiter = thread::spawn(move || {
        waiter_limiter
            .acquire("cancelled", || waiter_stopped.load(Ordering::SeqCst))
            .is_none()
    });
    wait_until_queued(&limiter, "cancelled");
    stopped.store(true, Ordering::SeqCst);

    assert!(waiter.join().unwrap());
    assert!(limiter.state.lock().unwrap().waiting.is_empty());
    drop(holders);
}

fn wait_until_queued(limiter: &AdmissionLimiter, repo: &str) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if limiter
            .state
            .lock()
            .unwrap()
            .waiting
            .iter()
            .any(|waiter| waiter.repo == repo)
        {
            return;
        }
        thread::yield_now();
    }
    panic!("{repo} was not queued before the deadline");
}

fn wait_until_worker_slots_are_free(slots: &WorkerSlots) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if slots.state.lock().unwrap().active == 0 {
            return;
        }
        thread::yield_now();
    }
    panic!("detached workers did not release their slots");
}
