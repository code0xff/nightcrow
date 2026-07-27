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
    /// Register a new running job and return its id.
    pub fn start(&self) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let mut jobs = self.lock();
        // Evict finished jobs before inserting so a long-lived server does not
        // accumulate them. Running jobs are never evicted: their thread still
        // holds the id and will write a result to it.
        if jobs.len() >= MAX_RETAINED_JOBS {
            let finished: Vec<u64> = jobs
                .iter()
                .filter(|(_, state)| !matches!(state, CloneState::Running))
                .map(|(id, _)| *id)
                .collect();
            for id in finished {
                jobs.remove(&id);
            }
        }
        jobs.insert(id, CloneState::Running);
        id
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

    /// Whether a clone is already running. One at a time keeps a client from
    /// starting a fleet of network transfers on the server's disk.
    pub fn any_running(&self) -> bool {
        self.lock()
            .values()
            .any(|state| matches!(state, CloneState::Running))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, CloneState>> {
        // A poisoned lock means a panic while holding it. The map is plain data
        // with no invariant spanning the critical sections, so recovering keeps
        // clone tracking usable instead of taking the server down with it.
        self.jobs.lock().unwrap_or_else(|err| err.into_inner())
    }
}

#[cfg(test)]
mod tests;
