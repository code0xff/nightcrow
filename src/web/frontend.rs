//! Embedded frontend assets. Bundled into the binary so the server is
//! self-contained and works offline. The terminal page and its vendored
//! xterm.js renderer are fleshed out in the frontend step; the login page is
//! self-contained here.

use crate::web::html_escape;

/// The login page, with `{error}` substituted for an optional message.
const LOGIN_TEMPLATE: &str = include_str!("frontend/login.html");
/// The terminal mirror page.
pub const APP_HTML: &str = include_str!("frontend/app.html");
/// Vendored xterm.js 5.5.0 (MIT) — the browser terminal renderer.
pub const XTERM_JS: &str = include_str!("frontend/vendor/xterm.js");
/// Vendored xterm.js 5.5.0 stylesheet (MIT).
pub const XTERM_CSS: &str = include_str!("frontend/vendor/xterm.css");

/// Render the login page, injecting an escaped error banner when present.
pub fn login_page(error: Option<&str>) -> String {
    let banner = match error {
        Some(msg) => format!(
            "<p class=\"error\" role=\"alert\">{}</p>",
            html_escape(msg)
        ),
        None => String::new(),
    };
    LOGIN_TEMPLATE.replace("<!--ERROR-->", &banner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_page_leaves_xterm_stdin_enabled_so_ime_and_paste_reach_ondata() {
        // xterm's `triggerDataEvent` — the sole source of the `onData` events the
        // page forwards as `{t:"paste"}` — returns early when `disableStdin` is
        // set. Turning it on silently drops IME-composed text (Hangul, kana,
        // pinyin) and clipboard pastes, with no error anywhere to trace it to.
        assert!(
            !APP_HTML.contains("disableStdin:"),
            "app.html must not set the disableStdin option; it would break IME input and paste"
        );
        assert!(
            APP_HTML.contains("term.onData("),
            "app.html must keep the onData handler that forwards composed text and pastes"
        );
    }
}
