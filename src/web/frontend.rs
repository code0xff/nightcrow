//! Embedded frontend assets. Bundled into the binary so the server is
//! self-contained and works offline. The terminal page and its vendored
//! xterm.js renderer are fleshed out in the frontend step; the login page is
//! self-contained here.

use crate::web::common::html_escape;

/// The login page, with `{error}` substituted for an optional message.
const LOGIN_TEMPLATE: &str = include_str!("frontend/login.html");
/// The terminal mirror page.
pub const APP_HTML: &str = include_str!("frontend/app.html");
/// Vendored xterm.js 5.5.0 (MIT) — the browser terminal renderer.
pub const XTERM_JS: &str = include_str!("frontend/vendor/xterm.js");
/// Vendored xterm.js 5.5.0 stylesheet (MIT).
pub const XTERM_CSS: &str = include_str!("frontend/vendor/xterm.css");
/// The crow favicon, shared with the web viewer (`viewer-ui/public/crow.svg`)
/// so both services show the same mark. Referenced from the viewer's source
/// SVG rather than a local copy, so the two never drift apart.
pub const CROW_SVG: &str = include_str!("../../viewer-ui/public/crow.svg");
/// The header/login mark: the same crow with a transparent background and no
/// tile, so the page draws the rounded accent tile behind it in CSS. Shared
/// with the viewer's `Mark`; referenced from the viewer's source so they never
/// drift apart.
pub const CROW_MONO_SVG: &str = include_str!("../../viewer-ui/public/crow-mono.svg");

/// Render the login page, injecting an escaped error banner when present.
pub fn login_page(error: Option<&str>) -> String {
    let banner = match error {
        Some(msg) => format!("<p class=\"error\" role=\"alert\">{}</p>", html_escape(msg)),
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

    #[test]
    fn hiding_the_composition_view_keeps_it_laid_out_so_the_ime_textarea_has_size() {
        // xterm's `updateCompositionElements` measures `.composition-view` with
        // getBoundingClientRect and sizes the IME textarea from the result. Taking
        // the element out of layout zeroes that box, collapsing the textarea to
        // 1x1 and stopping composition entirely — measured against stock xterm,
        // which sizes it 12.12x16 for the same input.
        let start = APP_HTML
            .find(".composition-view")
            .expect("app.html must style .composition-view to keep it off the grid");
        let rest = &APP_HTML[start..];
        let end = rest
            .find('}')
            .expect(".composition-view rule must be closed");
        let rule = &rest[..end];
        assert!(
            !rule.contains("display:"),
            "the .composition-view rule must not touch `display`; removing it from layout \
             collapses the IME textarea and breaks composition. Suppress the paint instead \
             (opacity). Rule was: {rule}"
        );
    }
}
