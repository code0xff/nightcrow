//! Track in-flight clones so the request that starts one can return at once.
//!
//! A clone runs for as long as the remote takes, which is far past what a
//! browser will hold a request open for — and on a phone the tab may be
//! suspended mid-transfer. So `POST /api/clone` starts a thread and answers
//! with an id, and the client polls `GET /api/clone?job=<id>` until the job
//! reaches a terminal state. The thread outlives the request that spawned it:
//! nothing is cancelled by a client that walks away, matching how the
//! terminal hub keeps PTYs alive across disconnects.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Finished jobs kept for polling. Small because a job is only read until the
/// client sees a terminal state; the cap only bounds a client that starts
/// clones and never reads the results.
const MAX_RETAINED_JOBS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloneState {
    Running,
    /// Cloned into this absolute path.
    Done(String),
    /// Failed with a message safe to show the client (git's own last line).
    Failed(String),
}

#[derive(Default)]
pub struct CloneJobs {
    next_id: AtomicU64,
    jobs: Mutex<HashMap<u64, CloneState>>,
}

impl CloneJobs {
    /// Admit a new job and return its id, or `None` when one is already
    /// running.
    ///
    /// Admission and insertion happen under one lock on purpose: checking
    /// "is anything running?" from the caller and inserting afterwards is a
    /// check-then-act race that lets parallel requests each see an idle
    /// registry and every one of them spawn a clone.
    pub fn try_start(&self) -> Option<u64> {
        let mut jobs = self.lock();
        if jobs
            .values()
            .any(|state| matches!(state, CloneState::Running))
        {
            return None;
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        // Evict the *oldest finished* jobs first — dropping all at once could
        // take one a client had not read yet, which reads as "your clone is
        // gone" even though it succeeded. Running jobs are never evicted:
        // their thread still holds the id and will write a result to it.
        if jobs.len() >= MAX_RETAINED_JOBS {
            let mut finished: Vec<u64> = jobs
                .iter()
                .filter(|(_, state)| !matches!(state, CloneState::Running))
                .map(|(id, _)| *id)
                .collect();
            finished.sort_unstable();
            let keep = MAX_RETAINED_JOBS / 2;
            let drop_count = finished.len().saturating_sub(keep);
            for id in finished.into_iter().take(drop_count) {
                jobs.remove(&id);
            }
        }
        jobs.insert(id, CloneState::Running);
        Some(id)
    }

    pub fn finish(&self, id: u64, state: CloneState) {
        // Only update an id we handed out; a finish for an evicted job is a
        // no-op rather than a resurrection.
        if let Some(slot) = self.lock().get_mut(&id) {
            *slot = state;
        }
    }

    pub fn get(&self, id: u64) -> Option<CloneState> {
        self.lock().get(&id).cloned()
    }

    /// The job currently running, if any — at most one exists by admission,
    /// so a client that lost track of its id can ask what to follow.
    pub fn running(&self) -> Option<u64> {
        self.lock()
            .iter()
            .find(|(_, state)| matches!(state, CloneState::Running))
            .map(|(id, _)| *id)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, CloneState>> {
        // A poisoned lock means a panic while holding it, but the map is plain
        // data with no invariant spanning critical sections — recovering keeps
        // clone tracking usable instead of taking the server down with it.
        self.jobs.lock().unwrap_or_else(|err| err.into_inner())
    }
}

#[cfg(test)]
mod tests;
