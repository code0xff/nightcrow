//! Web viewer: a native browser UI for the git panel and terminals, served as
//! a second HTTP server independent of the mirror.
//!
//! Unlike the mirror, this does not reflect the TUI's screen. It reads the same
//! `git`/`runtime`/`backend` layers the TUI reads and renders them as DOM, and
//! it owns its own terminals rather than sharing the TUI's. Nothing here
//! touches `App`, `ui`, or `input`, which is what lets the server run headless
//! (`nightcrow serve`) as well as alongside a running TUI.
//!
//! See `docs/web-viewer-plan.md` for the full design and its rationale.

#![allow(dead_code)] // Wired up at step 6; see the module docs above.

pub mod catalog;
pub mod dto;
pub mod limits;
pub mod runtime;
pub mod server;
