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
