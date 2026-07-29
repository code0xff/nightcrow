//! Web viewer: a native browser UI for the git panel and terminals, served as
//! its own HTTP server, independent of the TUI. Nothing here touches `App`,
//! `ui`, or `input`, which lets the server run headless (`nightcrow serve`).

#![allow(dead_code)] // Wired up at step 6; see the module docs above.

pub mod assets;
pub mod catalog;
pub mod clone_jobs;
pub mod dto;
pub mod highlight;
pub mod limits;
pub mod prefs;
pub mod runtime;
pub mod server;
pub mod session;
pub mod terminal;
