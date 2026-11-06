//! Transport-neutral state shared by every nightcrow session surface.
//!
//! The daemon owns this state. Browser HTTP and attached-terminal transports
//! translate their own requests into the operations exposed here; neither owns
//! the repositories, terminal hubs, preferences, or PTY size arbitration.

pub mod catalog;
pub mod clone;
pub mod limits;
pub mod notice;
pub mod prefs;
pub mod reload;
pub mod runtime;
pub mod size_owner;
pub mod terminal;

mod operations;
mod state;

pub use clone::{CloneJobState, CloneLookup, CloneStartError, CloneStarted};
pub use operations::{
    CloseError, NotifyError, OpenError, SessionRepo, accent, active_repo, can_clone, clone_status,
    close_repo, focus_repo, list_repos, list_session_repos, notify_pane, open_repo, reorder_repos,
    set_accent, start_clone,
};
#[cfg(test)]
pub use state::test_status_encoder;
pub use state::{RepositoryStatusSnapshot, SessionOptions, SessionState, StatusEncoder};
