use std::collections::{HashMap, VecDeque};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(super) const MAX_RETIRED_WORKERS: usize = 16;
pub(super) const MAX_IN_FLIGHT: usize = 8;
const MAX_IN_FLIGHT_PER_REPO: usize = 1;
const JOIN_GRACE: Duration = Duration::from_millis(5);

struct RetiredRegistry {
    handles: Mutex<VecDeque<JoinHandle<()>>>,
}

fn retired_registry() -> &'static RetiredRegistry {
    static REGISTRY: OnceLock<RetiredRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| RetiredRegistry {
        handles: Mutex::new(VecDeque::new()),
    })
}

pub(super) fn finish_or_retire(handle: JoinHandle<()>) {
    let deadline = Instant::now() + JOIN_GRACE;
    while !handle.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_micros(200));
    }
    if handle.is_finished() {
        join(handle);
        return;
    }

    let registry = retired_registry();
    let mut handles = registry.handles.lock().unwrap_or_else(|e| e.into_inner());
    reap_finished(&mut handles);
    if handles.len() >= MAX_RETIRED_WORKERS
        && let Some(oldest) = handles.pop_front()
    {
        // Pathological churn is back-pressured here instead of detaching an
        // unbounded number of threads and libgit2 file descriptors.
        join(oldest);
        reap_finished(&mut handles);
    }
    handles.push_back(handle);
    #[cfg(test)]
    TEST_PEAK_RETIRED.fetch_max(handles.len(), std::sync::atomic::Ordering::SeqCst);
}

pub(super) fn reap_retired() {
    let mut handles = retired_registry()
        .handles
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    reap_finished(&mut handles);
}

fn reap_finished(handles: &mut VecDeque<JoinHandle<()>>) {
    let mut index = 0;
    while index < handles.len() {
        if handles[index].is_finished() {
            join(handles.remove(index).expect("finished handle exists"));
        } else {
            index += 1;
        }
    }
}

fn join(handle: JoinHandle<()>) {
    if let Err(error) = handle.join() {
        tracing::warn!(?error, "git load worker panicked");
    }
}

#[derive(Default)]
struct InFlightState {
    total: usize,
    repos: HashMap<String, usize>,
}

fn in_flight() -> &'static (Mutex<InFlightState>, Condvar) {
    static LIMITER: OnceLock<(Mutex<InFlightState>, Condvar)> = OnceLock::new();
    LIMITER.get_or_init(|| (Mutex::new(InFlightState::default()), Condvar::new()))
}

pub(super) struct InFlightPermit {
    repo: String,
}

impl InFlightPermit {
    pub(super) fn acquire(repo: &str, stopped: impl Fn() -> bool) -> Option<Self> {
        let (lock, wake) = in_flight();
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        while state.total >= MAX_IN_FLIGHT
            || state.repos.get(repo).copied().unwrap_or(0) >= MAX_IN_FLIGHT_PER_REPO
        {
            if stopped() {
                return None;
            }
            state = wake
                .wait_timeout(state, JOIN_GRACE)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        if stopped() {
            return None;
        }
        state.total += 1;
        *state.repos.entry(repo.to_string()).or_default() += 1;
        #[cfg(test)]
        {
            use std::sync::atomic::Ordering;
            TEST_PEAK_PROCESS.fetch_max(state.total, Ordering::SeqCst);
            TEST_PEAK_REPO.fetch_max(state.repos[repo], Ordering::SeqCst);
        }
        Some(Self {
            repo: repo.to_string(),
        })
    }
}

impl Drop for InFlightPermit {
    fn drop(&mut self) {
        let (lock, wake) = in_flight();
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        state.total = state.total.saturating_sub(1);
        if let Some(count) = state.repos.get_mut(&self.repo) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                state.repos.remove(&self.repo);
            }
        }
        wake.notify_all();
    }
}

#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(test)]
static TEST_PEAK_RETIRED: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static TEST_PEAK_PROCESS: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static TEST_PEAK_REPO: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
struct ChurnObservation {
    peak_retired: usize,
    peak_process_io: usize,
    peak_repo_io: usize,
}

#[cfg(test)]
fn exercise_slow_worker_churn_for_test(count: usize) -> ChurnObservation {
    use std::sync::Arc;

    TEST_PEAK_RETIRED.store(0, Ordering::SeqCst);
    TEST_PEAK_PROCESS.store(0, Ordering::SeqCst);
    TEST_PEAK_REPO.store(0, Ordering::SeqCst);
    let release = Arc::new(AtomicBool::new(false));
    let release_later = Arc::clone(&release);
    let releaser = thread::spawn(move || {
        thread::sleep(Duration::from_millis(80));
        release_later.store(true, Ordering::SeqCst);
    });

    for _ in 0..count {
        let release = Arc::clone(&release);
        let handle = thread::spawn(move || {
            let Some(_permit) = InFlightPermit::acquire("same-repo", || false) else {
                return;
            };
            while !release.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(1));
            }
        });
        finish_or_retire(handle);
    }
    releaser.join().unwrap();
    thread::sleep(Duration::from_millis(10));
    reap_retired();

    ChurnObservation {
        peak_retired: TEST_PEAK_RETIRED.load(Ordering::SeqCst),
        peak_process_io: TEST_PEAK_PROCESS.load(Ordering::SeqCst),
        peak_repo_io: TEST_PEAK_REPO.load(Ordering::SeqCst),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn repeated_slow_worker_churn_keeps_threads_and_same_repo_io_bounded() {
        let observation = super::exercise_slow_worker_churn_for_test(32);

        assert!(observation.peak_retired <= super::MAX_RETIRED_WORKERS);
        assert!(observation.peak_process_io <= super::MAX_IN_FLIGHT);
        assert_eq!(observation.peak_repo_io, 1);
    }
}
