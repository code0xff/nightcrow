use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};

use git2::Repository;

use super::execute::execute;
use super::lifecycle::{InFlightPermit, WorkerPermit, finish_or_detach, join_finished};
use super::{GitLoadPayload, GitLoadReply, GitLoadRequest, Pending};

const TASK_PANIC_ERROR: &str = "background git load panicked";

type Shared = Arc<(Mutex<Pending>, Condvar)>;

pub(super) struct WorkerThread {
    reply_tx: mpsc::Sender<GitLoadReply>,
    handle: Option<JoinHandle<()>>,
    #[cfg(test)]
    hooks: Option<Arc<TestHooks>>,
}

impl WorkerThread {
    pub(super) fn new(reply_tx: mpsc::Sender<GitLoadReply>) -> Self {
        Self {
            reply_tx,
            handle: None,
            #[cfg(test)]
            hooks: None,
        }
    }

    pub(super) fn ensure_started(&mut self, shared: Shared) {
        if self.handle.as_ref().is_some_and(JoinHandle::is_finished) {
            join_finished(self.handle.take().expect("finished worker handle exists"));
        }
        if self.handle.is_some() {
            return;
        }
        let Some(permit) = WorkerPermit::try_acquire() else {
            return;
        };
        let task = WorkerTask {
            shared,
            replies: self.reply_tx.clone(),
            _worker_permit: permit,
            #[cfg(test)]
            executor: self.hooks.as_ref().map(|hooks| Arc::clone(&hooks.executor)),
        };
        match self.spawn_task(task) {
            Ok(handle) => self.handle = Some(handle),
            Err(error) => tracing::warn!(?error, "failed to spawn git load worker"),
        }
    }

    pub(super) fn finish(&mut self) {
        if let Some(handle) = self.handle.take() {
            finish_or_detach(handle);
        }
    }

    fn spawn_task(&self, task: WorkerTask) -> io::Result<JoinHandle<()>> {
        #[cfg(test)]
        if let Some(hooks) = &self.hooks {
            return (hooks.spawner)(task);
        }
        spawn_task(task)
    }

    #[cfg(test)]
    pub(super) fn with_hooks(reply_tx: mpsc::Sender<GitLoadReply>, hooks: Arc<TestHooks>) -> Self {
        Self {
            reply_tx,
            handle: None,
            hooks: Some(hooks),
        }
    }
}

pub(super) struct WorkerTask {
    shared: Shared,
    replies: mpsc::Sender<GitLoadReply>,
    _worker_permit: WorkerPermit<'static>,
    #[cfg(test)]
    executor: Option<Arc<TestExecutor>>,
}

pub(super) fn spawn_task(task: WorkerTask) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("git-load-worker".into())
        .spawn(move || task.run())
}

impl WorkerTask {
    fn run(self) {
        worker_loop(
            self.shared,
            self.replies,
            self._worker_permit,
            #[cfg(test)]
            self.executor,
        );
    }
}

fn worker_loop(
    shared: Shared,
    replies: mpsc::Sender<GitLoadReply>,
    _worker_permit: WorkerPermit<'static>,
    #[cfg(test)] executor: Option<Arc<TestExecutor>>,
) {
    let mut cached: Option<(String, Repository)> = None;
    loop {
        let request = {
            let (lock, wake) = &*shared;
            let mut pending = lock.lock().unwrap_or_else(|e| e.into_inner());
            while !pending.stopped && pending.requests.iter().all(Option::is_none) {
                pending = wake.wait(pending).unwrap_or_else(|e| e.into_inner());
            }
            if pending.stopped {
                return;
            }
            pending.take_next().expect("a pending request was observed")
        };

        if !shared
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_latest(&request)
        {
            continue;
        }
        let Some(_permit) = InFlightPermit::acquire(&request.repo, || {
            shared.0.lock().unwrap_or_else(|e| e.into_inner()).stopped
        }) else {
            return;
        };
        let result = execute_safely(
            &request,
            &mut cached,
            #[cfg(test)]
            executor.as_deref(),
        );
        if replies.send(GitLoadReply { request, result }).is_err() {
            return;
        }
    }
}

fn execute_safely(
    request: &GitLoadRequest,
    cached: &mut Option<(String, Repository)>,
    #[cfg(test)] executor: Option<&TestExecutor>,
) -> Result<GitLoadPayload, String> {
    let result = catch_unwind(AssertUnwindSafe(|| {
        #[cfg(test)]
        if let Some(executor) = executor {
            return executor(request, cached);
        }
        execute(request, cached)
    }));
    match result {
        Ok(result) => result.map_err(|error| error.to_string()),
        Err(_) => {
            *cached = None;
            Err(TASK_PANIC_ERROR.into())
        }
    }
}

#[cfg(test)]
pub(super) type TestExecutor = dyn Fn(&GitLoadRequest, &mut Option<(String, Repository)>) -> anyhow::Result<GitLoadPayload>
    + Send
    + Sync;

#[cfg(test)]
pub(super) type TestSpawner = dyn Fn(WorkerTask) -> io::Result<JoinHandle<()>> + Send + Sync;

#[cfg(test)]
pub(super) struct TestHooks {
    pub(super) spawner: Arc<TestSpawner>,
    pub(super) executor: Arc<TestExecutor>,
}
