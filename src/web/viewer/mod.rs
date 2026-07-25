//! Web viewer: a native browser UI for the git panel and terminals, served as
//! a second HTTP server independent of the mirror. Nothing here touches `App`,
//! `ui`, or `input`, which lets the server run headless (`nightcrow serve`).

#![allow(dead_code)] // Wired up at step 6; see the module docs above.

pub mod assets;
pub mod catalog;
pub mod dto;
pub mod highlight;
pub mod limits;
pub mod prefs;
pub mod runtime;
pub mod server;
pub mod terminal;
