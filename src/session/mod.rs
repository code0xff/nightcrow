//! Transport-neutral state shared by every nightcrow session surface.
//!
//! The daemon owns this state. Browser HTTP and attached-terminal transports
//! translate their own requests into the operations exposed here; neither owns
//! the repositories, terminal hubs, preferences, or PTY size arbitration.

pub mod catalog;
pub mod limits;
pub mod prefs;
pub mod reload;
pub mod runtime;
pub mod size_owner;
pub mod terminal;

mod operations;
mod state;

pub use operations::{
    CloseError, OpenError, SessionRepo, accent, active_repo, close_repo, focus_repo, list_repos,
    list_session_repos, open_repo, reorder_repos, set_accent,
};
#[cfg(test)]
pub use state::test_status_encoder;
pub use state::{RepositoryStatusSnapshot, SessionOptions, SessionState, StatusEncoder};
