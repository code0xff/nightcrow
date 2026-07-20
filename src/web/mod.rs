//! Web mirror: serve a live, controllable view of this nightcrow over HTTP so a
//! browser and the local terminal drive the same session.
//!
//! - [`common`] holds the server-agnostic primitives: password verification,
//!   session tokens, login rate limiting, HTTP request/response framing, and
//!   connection accounting.
//! - [`protocol`] encodes screen frames (ratatui `Buffer` → ANSI) and decodes
//!   browser input (JSON → crossterm events).
//! - `server` runs a synchronous WebSocket/HTTP server on background threads and
//!   exchanges frames/input with the main loop over channels — the `App` itself
//!   is never shared across threads.
//! - `frontend` holds the embedded page assets.

pub mod protocol;

pub(crate) mod common;
mod frontend;
mod server;
pub(crate) mod viewer;

pub use server::WebServer;
