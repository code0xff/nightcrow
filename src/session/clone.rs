//! A session-wide clone slot shared by the browser and attached clients.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

const MAX_RETAINED_JOBS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CloneJobState {
    Running,
    Done { path: String },
    Failed { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum CloneLookup {
    Idle,
    Found { job: u64, state: CloneJobState },
    Unknown { job: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneStarted {
    pub job: u64,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloneStartError {
    Unavailable,
    InvalidUrl(crate::git::clone::CloneUrlError),
    MissingParent,
    ExistingDestination,
    CreateDestination,
    AlreadyRunning,
    StartWorker,
}

impl CloneStartError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Unavailable => "git is not installed or not on the daemon's PATH",
            Self::InvalidUrl(error) => error.message(),
            Self::MissingParent => "no such directory",
            Self::ExistingDestination => "a folder with that repository's name already exists here",
            Self::CreateDestination => "could not create the destination",
            Self::AlreadyRunning => "a clone is already running",
            Self::StartWorker => "could not start the clone",
        }
    }
}

#[derive(Default)]
struct CloneJobs {
    next_id: AtomicU64,
    jobs: Mutex<HashMap<u64, CloneJobState>>,
}

impl CloneJobs {
    fn try_start(&self) -> Option<u64> {
        let mut jobs = self.lock();
        if jobs
            .values()
            .any(|state| matches!(state, CloneJobState::Running))
        {
            return None;
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        if jobs.len() >= MAX_RETAINED_JOBS {
            let mut finished: Vec<u64> = jobs
                .iter()
                .filter(|(_, state)| !matches!(state, CloneJobState::Running))
                .map(|(id, _)| *id)
                .collect();
            finished.sort_unstable();
            for id in finished
                .into_iter()
                .take(jobs.len().saturating_sub(MAX_RETAINED_JOBS / 2))
            {
                jobs.remove(&id);
            }
        }
        jobs.insert(id, CloneJobState::Running);
        Some(id)
    }

    fn finish(&self, id: u64, state: CloneJobState) {
        if let Some(slot) = self.lock().get_mut(&id) {
            *slot = state;
        }
    }

    fn lookup(&self, job: Option<u64>) -> CloneLookup {
        let jobs = self.lock();
        match job {
            Some(id) => jobs
                .get(&id)
                .cloned()
                .map(|state| CloneLookup::Found { job: id, state })
                .unwrap_or(CloneLookup::Unknown { job: id }),
            None => jobs
                .iter()
                .find(|(_, state)| matches!(state, CloneJobState::Running))
                .map(|(id, state)| CloneLookup::Found {
                    job: *id,
                    state: state.clone(),
                })
                .unwrap_or(CloneLookup::Idle),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<u64, CloneJobState>> {
        self.jobs.lock().unwrap_or_else(|error| error.into_inner())
    }
}

pub(super) struct CloneService {
    available: std::sync::atomic::AtomicBool,
    jobs: Arc<CloneJobs>,
    runner: Mutex<CloneRunner>,
}

pub(crate) type CloneRunner = fn(&str, &Path) -> Result<(), String>;

fn run_git_clone(url: &str, dest: &Path) -> Result<(), String> {
    crate::git::clone::run_clone(url, dest).map_err(|error| error.to_string())
}

impl CloneService {
    pub(super) fn new() -> Self {
        Self {
            available: std::sync::atomic::AtomicBool::new(crate::git::clone::git_available()),
            jobs: Arc::new(CloneJobs::default()),
            runner: Mutex::new(run_git_clone),
        }
    }

    #[cfg(test)]
    fn with_runner(runner: CloneRunner) -> Self {
        Self {
            available: std::sync::atomic::AtomicBool::new(true),
            jobs: Arc::new(CloneJobs::default()),
            runner: Mutex::new(runner),
        }
    }

    #[cfg(test)]
    pub(super) fn set_runner_for_test(&self, runner: CloneRunner) {
        *self
            .runner
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = runner;
        self.available.store(true, Ordering::Release);
    }

    pub(super) fn available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }

    pub(super) fn start(
        &self,
        parent: &str,
        raw_url: &str,
    ) -> Result<CloneStarted, CloneStartError> {
        if !self.available() {
            return Err(CloneStartError::Unavailable);
        }
        let url = raw_url.trim();
        let name =
            crate::git::clone::validate_clone_url(url).map_err(CloneStartError::InvalidUrl)?;
        let parent = crate::platform::paths::expand_tilde(parent.trim());
        if !parent.is_dir() {
            return Err(CloneStartError::MissingParent);
        }
        let base = parent
            .canonicalize()
            .map_err(|_| CloneStartError::MissingParent)?;
        let dest = base.join(&name);
        let Some(id) = self.jobs.try_start() else {
            return Err(CloneStartError::AlreadyRunning);
        };
        if let Err(error) = std::fs::create_dir(&dest) {
            self.jobs.finish(
                id,
                CloneJobState::Failed {
                    message: CloneStartError::CreateDestination.message().to_string(),
                },
            );
            return Err(if error.kind() == std::io::ErrorKind::AlreadyExists {
                CloneStartError::ExistingDestination
            } else {
                CloneStartError::CreateDestination
            });
        }
        let claimed = dest.clone();
        let jobs = Arc::clone(&self.jobs);
        let runner = *self
            .runner
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let url = url.to_string();
        if let Err(error) = std::thread::Builder::new()
            .name("nightcrow-clone".into())
            .spawn(move || run_and_record(jobs, id, url, dest, runner))
        {
            let _ = std::fs::remove_dir(&claimed);
            self.jobs.finish(
                id,
                CloneJobState::Failed {
                    message: CloneStartError::StartWorker.message().to_string(),
                },
            );
            tracing::warn!(error = %error, "clone thread failed to start");
            return Err(CloneStartError::StartWorker);
        }
        Ok(CloneStarted { job: id, name })
    }

    pub(super) fn lookup(&self, job: Option<u64>) -> CloneLookup {
        self.jobs.lookup(job)
    }
}

fn run_and_record(jobs: Arc<CloneJobs>, id: u64, url: String, dest: PathBuf, runner: CloneRunner) {
    let outcome = match runner(&url, &dest) {
        Ok(()) => CloneJobState::Done {
            path: crate::platform::paths::for_display(&dest).into_owned(),
        },
        Err(message) => {
            let _ = std::fs::remove_dir(&dest);
            tracing::info!(error = %message, "clone failed");
            CloneJobState::Failed { message }
        }
    };
    jobs.finish(id, outcome);
}

#[cfg(test)]
#[path = "clone_tests.rs"]
mod tests;
