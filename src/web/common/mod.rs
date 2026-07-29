//! Primitives shared by nightcrow's web servers.
//!
//! Everything here is independent of what a given server actually serves: it
//! knows about passwords, sessions, HTTP framing, and connection accounting,
//! but nothing about screen frames, git data, or terminals.

pub mod auth;
pub mod conn;
pub mod http;
pub mod sse;
