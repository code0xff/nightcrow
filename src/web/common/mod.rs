//! Primitives shared by nightcrow's web servers.
//!
//! Everything here is independent of what a given server actually serves: it
//! knows about passwords, sessions, HTTP framing, and connection accounting,
//! but nothing about screen frames, git data, or terminals. The mirror is the
//! only consumer today; the planned viewer (`docs/web-viewer-plan.md`) is a
//! second server that shares exactly this layer and nothing above it.

pub mod auth;
pub mod conn;
pub mod http;
pub mod sse;

/// Escape the five HTML-significant characters for safe interpolation into page
/// text (used for the login error banner).
pub fn html_escape(s: &str) -> String {
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
