//! Web mirror: serve a live, controllable view of this nightcrow over HTTP so a
//! browser and the local terminal drive the same session.

pub mod protocol;

pub(crate) mod common;
mod frontend;
mod server;
pub(crate) mod viewer;

pub use server::WebServer;
