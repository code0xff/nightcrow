//! Process-level orchestration for the native TUI.
//!
//! This layer owns startup and event routing. Domain state lives in `app`,
//! while rendering, runtime services, and persistence stay in their own
//! top-level modules.

pub(crate) mod bootstrap;
pub(crate) mod event_loop;
pub(crate) mod input;
pub(crate) mod splash;
pub(crate) mod terminal_guard;
