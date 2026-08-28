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

/// The policy every asset is served under. `connect-src` must allow the same
/// origin for fetch, SSE, and the terminal WebSocket, and a WebSocket URL is
/// not covered by `'self'` in every browser — but the bare `ws:`/`wss:`
/// schemes admit *any* host, and the HTML preview's `srcdoc` frame inherits
/// this policy, so the socket schemes are pinned to the host the request was
/// addressed to. A request whose `Host` is unusable falls back to the broad
/// schemes: that only ever relaxes to what every response carried before, and
/// never breaks the page that sent it.
///
/// The pin trusts `Host` to be the name the *browser* used. A reverse proxy
/// that rewrites it toward upstream would pin the sockets to a host the page
/// cannot use, and the terminal would fail loudly on connect — no such proxy
/// fronts this server today (`host_allowed` makes the same assumption), and
/// the fix there is the proxy forwarding the original `Host`.
fn csp(host: Option<&str>) -> String {
    let sockets = match host.filter(|h| plain_host(h)) {
        Some(host) => format!("ws://{host} wss://{host}"),
        None => "ws: wss:".to_string(),
    };
    format!(
        "default-src 'self'; \
img-src 'self' data:; \
style-src 'self' 'unsafe-inline'; \
script-src 'self'; \
connect-src 'self' {sockets}; \
frame-ancestors 'none'; \
base-uri 'none'; \
form-action 'self'"
    )
}

/// Only what a `host[:port]` can be made of. This value is printed into a
/// header, so anything looser could splice CSP directives (`;`, whitespace)
/// or new headers into the response.
fn plain_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 255
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']'))
}

/// Serve a built asset, falling back to `index.html` so client-side routes and
/// a bare `/` both load the app.
///
/// A miss is split by whether the request names a file. An extensionless path
/// is a client-side route and gets the app shell; a path naming a file is a
/// real asset miss and gets a 404. Handing `index.html` back for a missing
/// `.svg`/`.js` serves HTML under an image or module request, which fails
/// silently — a stale embedded build once rendered the header/splash as a
/// blank accent tile exactly this way. A loud 404 surfaces it instead.
pub fn serve(path: &str, host: Option<&str>) -> Option<Vec<u8>> {
    let csp = csp(host);
    let headers = [
        ("Content-Security-Policy", csp.as_str()),
        ("X-Content-Type-Options", "nosniff"),
        ("Referrer-Policy", "no-referrer"),
    ];
    let trimmed = path.trim_start_matches('/');
    let candidate = if trimmed.is_empty() { SHELL } else { trimmed };

    // The shell is the one asset that is not served as it is stored; see
    // `shell`. Answered before the lookup below so `/` and `/index.html` are
    // one case.
    if candidate == SHELL {
        return shell(&headers);
    }

    // `rust_embed` resolves names against the embedded map, so a `..` in the
    // request simply misses; there is no filesystem lookup to escape. The
    // guessed mimetype travels with the bytes, so content type and file
    // cannot disagree — which matters when the CSP refuses a mistyped script.
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

    shell(&headers)
}

/// The document every asset above is loaded from.
const SHELL: &str = "index.html";

/// Where the build id is stamped, and the name the page reads it back by.
const BUILD_META: &str = "nightcrow-build";

/// The app shell, carrying the id of the build it is part of.
///
/// Stamped rather than left to the client to work out: what the page needs is
/// the build **it** is running, and the only moment that is certain is the one
/// it is handed over. Inferring it from the first API response is wrong for a
/// tab sitting on the login screen across a rebuild.
///
/// One read, not two: a debug server reads `dist` from disk, so reading the
/// bytes and then asking [`build_id`] again could stamp the build that landed
/// in between onto the document that preceded it.
fn shell(headers: &[(&str, &str)]) -> Option<Vec<u8>> {
    let file = Assets::get(SHELL)?;
    let id = id_of(file.metadata.sha256_hash());
    let stamped = stamp_build(file.data.as_ref(), &id);
    let mimetype = file.metadata.mimetype().to_string();
    Some(http::response("200 OK", &mimetype, headers, &stamped))
}

/// Put the build id in the head of `html`.
///
/// Returns the document untouched when there is no head to put it in, which is
/// a shell this server did not build. The page then has no id to compare and
/// says nothing, rather than being told it is out of date forever.
fn stamp_build(html: &[u8], id: &str) -> Vec<u8> {
    const HEAD: &[u8] = b"<head>";
    let Some(at) = html
        .windows(HEAD.len())
        .position(|window| window == HEAD)
        .map(|start| start + HEAD.len())
    else {
        return html.to_vec();
    };
    let tag = format!("\n    <meta name=\"{BUILD_META}\" content=\"{id}\" />");
    let mut stamped = Vec::with_capacity(html.len() + tag.len());
    stamped.extend_from_slice(&html[..at]);
    stamped.extend_from_slice(tag.as_bytes());
    stamped.extend_from_slice(&html[at..]);
    stamped
}

/// How much of the hash names a build. Long enough that two builds never
/// collide in practice, short enough to read in a log line; this tells builds
/// apart, it does not authenticate one.
const BUILD_ID_BYTES: usize = 4;

/// Names the built frontend this server is serving.
///
/// The hash of `index.html`, because that file names the code: every chunk and
/// stylesheet Vite emits carries a content hash in its filename, so a change
/// to any of them changes a name in the shell. What that leaves out is
/// `public/`, copied under fixed names — deliberately, since a file nothing
/// imports cannot put a page in the replaced-code state this exists to
/// report.
///
/// Read per call rather than held: only a release build embeds `dist`, and a
/// debug server reads it from disk — a rebuild under a running daemon is
/// exactly the case this exists to report.
///
/// `None` when the shell is missing: a build that cannot load at all, where
/// saying nothing beats a placeholder id that never changes.
pub fn build_id() -> Option<String> {
    Some(id_of(Assets::get(SHELL)?.metadata.sha256_hash()))
}

fn id_of(hash: [u8; 32]) -> String {
    hash[..BUILD_ID_BYTES]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Whether the frontend was built into this binary. A source checkout with no
/// `dist` still compiles; the server then says so rather than 404ing blankly.
#[cfg(test)]
pub fn is_present() -> bool {
    Assets::get(SHELL).is_some()
}

#[cfg(test)]
#[path = "assets_tests.rs"]
mod tests;
