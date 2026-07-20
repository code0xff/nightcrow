//! The viewer's built frontend, embedded in the binary.
//!
//! `viewer-ui/dist` is committed to the repository, so `cargo install
//! nightcrow` needs no Node toolchain — the alternative, invoking npm from a
//! build script, breaks every install that lacks one. Rebuild it with
//! `npm --prefix viewer-ui run build` after changing anything under
//! `viewer-ui/src`.
//!
//! Every asset is served with a strict CSP and `nosniff`. The bundle is
//! entirely self-contained (no CDN, no external fonts), so `default-src
//! 'self'` costs nothing and removes a whole class of injection.

use crate::web::common::http;
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "viewer-ui/dist"]
struct Assets;

/// `connect-src` must allow the same origin for fetch, SSE, and the terminal
/// WebSocket; `ws:`/`wss:` are needed because a WebSocket URL is not covered
/// by `'self'` in every browser.
const CSP: &str = "default-src 'self'; \
img-src 'self' data:; \
style-src 'self' 'unsafe-inline'; \
script-src 'self'; \
connect-src 'self' ws: wss:; \
frame-ancestors 'none'; \
base-uri 'none'; \
form-action 'self'";

/// Serve a built asset, falling back to `index.html` so client-side routes and
/// a bare `/` both load the app.
pub fn serve(path: &str) -> Option<Vec<u8>> {
    let trimmed = path.trim_start_matches('/');
    let candidate = if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    };

    // `rust_embed` resolves names against the embedded map, so a `..` in the
    // request simply misses; there is no filesystem lookup to escape.
    // rust-embed carries the guessed type alongside the bytes, so the content
    // type comes from the same lookup that found the file — they cannot
    // disagree, which matters when the CSP refuses a mistyped script.
    let file = Assets::get(candidate).or_else(|| Assets::get("index.html"))?;

    Some(http::response(
        "200 OK",
        file.metadata.mimetype(),
        &[
            ("Content-Security-Policy", CSP),
            ("X-Content-Type-Options", "nosniff"),
            ("Referrer-Policy", "no-referrer"),
        ],
        file.data.as_ref(),
    ))
}

/// Whether the frontend was built into this binary. A source checkout with no
/// `dist` still compiles; the server then says so rather than 404ing blankly.
pub fn is_present() -> bool {
    Assets::get("index.html").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(response: &[u8]) -> String {
        String::from_utf8_lossy(response).into_owned()
    }

    #[test]
    fn the_frontend_is_embedded() {
        assert!(
            is_present(),
            "viewer-ui/dist must be committed and built; run `npm --prefix viewer-ui run build`"
        );
    }

    #[test]
    fn the_root_serves_the_app_shell() {
        let response = serve("/").expect("index.html");
        let text = text(&response);

        assert!(text.starts_with("HTTP/1.1 200 OK"));
        assert!(text.contains("text/html"));
        assert!(text.contains("<div id=\"root\">"), "not the app shell");
    }

    #[test]
    fn assets_carry_a_strict_csp_and_nosniff() {
        let text = text(&serve("/").unwrap());

        assert!(text.contains("Content-Security-Policy: default-src 'self'"));
        assert!(text.contains("frame-ancestors 'none'"));
        assert!(text.contains("X-Content-Type-Options: nosniff"));
    }

    #[test]
    fn a_traversal_request_cannot_escape_the_embedded_map() {
        // There is no filesystem lookup here, so a `..` simply misses and the
        // app shell is served instead of anything outside the bundle.
        let text = text(&serve("/../../etc/passwd").unwrap());

        assert!(text.contains("<div id=\"root\">"), "expected the app shell");
        assert!(!text.contains("root:x:"), "a system file leaked");
    }

    #[test]
    fn a_javascript_bundle_is_served_with_a_script_mime() {
        // The build hashes the filename, so find it rather than hard-coding.
        let name = Assets::iter()
            .find(|f| f.ends_with(".js"))
            .expect("a built bundle");
        let text = text(&serve(&format!("/{name}")).unwrap());

        assert!(
            text.contains("javascript"),
            "a script served as the wrong type will be refused by the CSP"
        );
    }
}
