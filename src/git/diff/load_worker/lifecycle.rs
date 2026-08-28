use std::collections::{HashMap, VecDeque};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(super) const MAX_IN_FLIGHT: usize = 8;
const MAX_IN_FLIGHT_PER_REPO: usize = 1;
pub(super) const MAX_WORKER_THREADS: usize = crate::workspace::MAX_PROJECTS + MAX_IN_FLIGHT;
pub(super) const JOIN_GRACE: Duration = Duration::from_millis(5);

#[derive(Default)]
struct WorkerSlotState {
    active: usize,
    peak: usize,
}

struct WorkerSlots {
    state: Mutex<WorkerSlotState>,
}

impl WorkerSlots {
    fn new() -> Self {
        Self {
            state: Mutex::new(WorkerSlotState::default()),
        }
    }

    fn try_acquire(&self) -> Option<WorkerPermit<'_>> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.active >= MAX_WORKER_THREADS {
            return None;
        }
        state.active += 1;
        state.peak = state.peak.max(state.active);
        Some(WorkerPermit { slots: self })
    }
}

fn worker_slots() -> &'static WorkerSlots {
    static SLOTS: OnceLock<WorkerSlots> = OnceLock::new();
    SLOTS.get_or_init(WorkerSlots::new)
}

pub(super) struct WorkerPermit<'a> {
    slots: &'a WorkerSlots,
}

impl WorkerPermit<'static> {
    pub(super) fn try_acquire() -> Option<Self> {
        worker_slots().try_acquire()
    }
}

impl Drop for WorkerPermit<'_> {
    fn drop(&mut self) {
        let mut state = self.slots.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active = state.active.saturating_sub(1);
    }
}

pub(super) fn finish_or_detach(handle: JoinHandle<()>) {
    let deadline = Instant::now() + JOIN_GRACE;
    while !handle.is_finished() && Instant::now() < deadline {
        thread::yield_now();
    }
    if handle.is_finished() {
        join_finished(handle);
    }
}

pub(super) fn join_finished(handle: JoinHandle<()>) {
    debug_assert!(handle.is_finished());
    if let Err(error) = handle.join() {
        tracing::warn!(?error, "git load worker panicked");
    }
}

struct Waiter {
    ticket: u64,
    repo: String,
}

#[derive(Default)]
struct AdmissionState {
    total: usize,
    repos: HashMap<String, usize>,
    waiting: VecDeque<Waiter>,
    next_ticket: u64,
    next_admission: u64,
    peak_total: usize,
    peak_repo: usize,
}

struct AdmissionLimiter {
    state: Mutex<AdmissionState>,
    wake: Condvar,
}

impl AdmissionLimiter {
    fn new() -> Self {
        Self {
            state: Mutex::new(AdmissionState::default()),
            wake: Condvar::new(),
        }
    }

    fn acquire(&self, repo: &str, stopped: impl Fn() -> bool) -> Option<InFlightPermit<'_>> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let ticket = state.next_ticket;
        state.next_ticket = state.next_ticket.wrapping_add(1);
        state.waiting.push_back(Waiter {
            ticket,
            repo: repo.to_string(),
        });

        loop {
            if stopped() {
                remove_waiter(&mut state.waiting, ticket);
                self.wake.notify_all();
                return None;
            }
            let eligible = state.total < MAX_IN_FLIGHT
                && state
                    .waiting
                    .iter()
                    .find(|waiter| {
                        state.repos.get(&waiter.repo).copied().unwrap_or(0) < MAX_IN_FLIGHT_PER_REPO
                    })
                    .is_some_and(|waiter| waiter.ticket == ticket);
            if eligible {
                remove_waiter(&mut state.waiting, ticket);
                state.total += 1;
                let repo_count = state.repos.entry(repo.to_string()).or_default();
                *repo_count += 1;
                let repo_count = *repo_count;
                state.next_admission = state.next_admission.wrapping_add(1);
                let admission = state.next_admission;
                state.peak_total = state.peak_total.max(state.total);
                state.peak_repo = state.peak_repo.max(repo_count);
                return Some(InFlightPermit {
                    limiter: self,
                    repo: repo.to_string(),
                    _admission: admission,
                });
            }
            state = self
                .wake
                .wait_timeout(state, JOIN_GRACE)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

fn remove_waiter(waiting: &mut VecDeque<Waiter>, ticket: u64) {
    if let Some(index) = waiting.iter().position(|waiter| waiter.ticket == ticket) {
        waiting.remove(index);
    }
}

fn in_flight() -> &'static AdmissionLimiter {
    static LIMITER: OnceLock<AdmissionLimiter> = OnceLock::new();
    LIMITER.get_or_init(AdmissionLimiter::new)
}

pub(super) struct InFlightPermit<'a> {
    limiter: &'a AdmissionLimiter,
    repo: String,
    _admission: u64,
}

impl InFlightPermit<'static> {
    pub(super) fn acquire(repo: &str, stopped: impl Fn() -> bool) -> Option<Self> {
        in_flight().acquire(repo, stopped)
    }
}

impl Drop for InFlightPermit<'_> {
    fn drop(&mut self) {
        let mut state = self.limiter.state.lock().unwrap_or_else(|e| e.into_inner());
        state.total = state.total.saturating_sub(1);
        if let Some(count) = state.repos.get_mut(&self.repo) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                state.repos.remove(&self.repo);
            }
        }
        self.limiter.wake.notify_all();
    }
}

#[cfg(test)]
impl InFlightPermit<'_> {
    fn admission_for_test(&self) -> u64 {
        self._admission
    }
}

#[cfg(test)]
mod tests;
