//! Embedded frontend assets. Bundled into the binary so the server is
//! self-contained and works offline. The terminal page and its vendored
//! xterm.js renderer are fleshed out in the frontend step; the login page is
//! self-contained here.

use crate::web::html_escape;

/// The login page, with `{error}` substituted for an optional message.
const LOGIN_TEMPLATE: &str = include_str!("frontend/login.html");
/// The terminal mirror page.
pub const APP_HTML: &str = include_str!("frontend/app.html");

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
