//! The half of a pane's teardown that waits.
//!
//! Dropping a pane happens on the hub's worker thread, which also carries every
//! other pane's output. Terminating a process tree and giving three workers
//! their reap window there stalled them; the handles move here instead.

use super::PTY_REAP_TIMEOUT;
use crate::platform::process_tree::ProcessTree;
use crate::platform::threading::try_timed_join;
use portable_pty::{ChildKiller, MasterPty};
use std::thread;

pub(super) struct PaneReaper {
    pub(super) master: Option<Box<dyn MasterPty>>,
    pub(super) killer: Option<Box<dyn ChildKiller + Send + Sync>>,
    pub(super) process_tree: Option<ProcessTree>,
    pub(super) writer: Option<thread::JoinHandle<()>>,
    pub(super) reader: Option<thread::JoinHandle<()>>,
    pub(super) waiter: Option<thread::JoinHandle<()>>,
}

impl PaneReaper {
    pub(super) fn reap(mut self) {
        // Terminate the pane's process boundary before the PTY handles and
        // waiter are reaped, so child jobs cannot outlive an explicit close.
        if let Some(tree) = self.process_tree.as_ref()
            && let Err(error) = tree.terminate()
        {
            tracing::warn!(%error, "could not terminate pane process tree");
        }
        // Best-effort direct kill covers a child that exited before its tree
        // termination raced with teardown.
        if let Some(killer) = self.killer.as_mut() {
            let _ = killer.kill();
        }
        // Close the PTY boundary before joining; platform teardown normally
        // releases blocked system calls, and every join remains bounded.
        self.master.take();
        if let Some(h) = self.writer.take() {
            try_timed_join(h, PTY_REAP_TIMEOUT);
        }
        // Bounded joins: a daemonized grandchild may keep the reader blocked
        // on Unix, so detach rather than freezing pane teardown.
        if let Some(h) = self.reader.take() {
            try_timed_join(h, PTY_REAP_TIMEOUT);
        }
        if let Some(h) = self.waiter.take() {
            try_timed_join(h, PTY_REAP_TIMEOUT);
        }
    }
}
