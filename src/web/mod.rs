//! Web mirror: serve a live, controllable view of this nightcrow over HTTP so a
//! browser and the local terminal drive the same session.
//!
//! - [`protocol`] encodes screen frames (ratatui `Buffer` → ANSI) and decodes
//!   browser input (JSON → crossterm events).
//! - `server` runs a synchronous WebSocket/HTTP server on background threads and
//!   exchanges frames/input with the main loop over channels — the `App` itself
//!   is never shared across threads.
//! - `auth` provides Argon2 password verification, session tokens, and login
//!   rate limiting; `http` parses requests and builds responses; `frontend`
//!   holds the embedded page assets.

pub mod protocol;

mod auth;
mod frontend;
mod http;
mod server;

pub use server::WebServer;

/// Escape the five HTML-significant characters for safe interpolation into page
/// text (used for the login error banner).
pub(crate) fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::html_escape;

    #[test]
    fn html_escape_neutralizes_markup() {
        assert_eq!(
            html_escape(r#"<script>&"'"#),
            "&lt;script&gt;&amp;&quot;&#39;"
        );
        assert_eq!(html_escape("plain text"), "plain text");
    }
}
