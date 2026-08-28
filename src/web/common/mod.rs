//! Primitives shared by nightcrow's web servers — passwords, sessions, HTTP
//! framing, connection accounting. Nothing here knows what a server actually
//! serves: no screen frames, git data, or terminals.

pub mod auth;
pub mod conn;
pub mod http;
pub mod sessions;
pub mod sse;
