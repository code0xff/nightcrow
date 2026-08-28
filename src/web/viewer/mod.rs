//! Web viewer: a browser UI for the git panel and terminals, served as its
//! own HTTP server. Nothing here touches `App`, `ui`, or `input`, which lets
//! the server run headless (`nightcrow serve`).

pub mod assets;
pub mod clone_jobs;
pub mod dto;
pub mod highlight;
pub mod limits;
pub mod server;
pub(crate) mod status_payload;
