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
///
/// A miss is split by whether the request names a file. An extensionless path is
/// a client-side route and gets the app shell; a path that names a file (has an
/// extension) is a real asset miss and gets a 404. The shell fallback must not
/// cover the second case: handing `index.html` back for a missing `.svg`/`.js`
/// serves HTML under an image or module request, which then fails silently —
/// a stale embedded build made the header/splash crow render as a blank accent
/// tile exactly this way (`/crow-mono.svg` missing → HTML → the `<img>` shows
/// nothing). A loud 404 surfaces the missing asset instead.
pub fn serve(path: &str) -> Option<Vec<u8>> {
    let headers = [
        ("Content-Security-Policy", CSP),
        ("X-Content-Type-Options", "nosniff"),
        ("Referrer-Policy", "no-referrer"),
    ];
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
    if let Some(file) = Assets::get(candidate) {
        return Some(http::response(
            "200 OK",
            file.metadata.mimetype(),
            &headers,
            file.data.as_ref(),
        ));
    }

    // The last path segment names a file when it contains a dot — the extension
    // that marks it an asset rather than a route.
    let names_a_file = candidate
        .rsplit('/')
        .next()
        .is_some_and(|name| name.contains('.'));
    if names_a_file {
        return Some(http::response(
            "404 Not Found",
            "text/plain; charset=utf-8",
            &headers,
            b"not found",
        ));
    }

    let shell = Assets::get("index.html")?;
    Some(http::response(
        "200 OK",
        shell.metadata.mimetype(),
        &headers,
        shell.data.as_ref(),
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
    fn a_missing_asset_is_a_404_not_the_app_shell() {
        // A named file that is not embedded must 404, not fall back to
        // index.html: serving HTML under an <img>/module request fails silently
        // (the mark rendered as a blank accent tile when a stale build lacked
        // crow-mono.svg). A loud 404 surfaces the missing asset instead.
        let text = text(&serve("/crow-mono-does-not-exist.svg").unwrap());

        assert!(text.starts_with("HTTP/1.1 404"), "got: {text}");
        assert!(
            !text.contains("<div id=\"root\">"),
            "must not serve the shell"
        );
    }

    #[test]
    fn an_embedded_svg_asset_is_served_as_an_image() {
        // The crow mark's source: present in the bundle and served with an image
        // type, so the <img> actually renders it.
        let text = text(&serve("/crow-mono.svg").unwrap());

        assert!(text.starts_with("HTTP/1.1 200"), "got: {text}");
        assert!(text.contains("image/svg+xml"), "wrong content type");
    }

    #[test]
    fn an_extensionless_route_falls_back_to_the_shell() {
        // A client-side route (no file extension) still gets the app shell so
        // the SPA loads; only named-file misses 404.
        let text = text(&serve("/some/route").unwrap());

        assert!(text.contains("<div id=\"root\">"), "expected the app shell");
    }

    #[test]
    fn the_pwa_manifest_is_served_as_a_manifest() {
        // The install manifest must be reachable and typed as JSON so the
        // browser parses it rather than downloading it as an opaque blob.
        let text = text(&serve("/manifest.webmanifest").unwrap());

        assert!(text.starts_with("HTTP/1.1 200"), "got: {text}");
        assert!(
            text.contains("json"),
            "manifest served with a non-JSON type"
        );
    }

    #[test]
    fn a_pwa_icon_is_served_as_a_png() {
        // Home-screen install needs raster icons the launcher can render.
        let text = text(&serve("/icon-512.png").unwrap());

        assert!(text.starts_with("HTTP/1.1 200"), "got: {text}");
        assert!(text.contains("image/png"), "wrong content type");
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
