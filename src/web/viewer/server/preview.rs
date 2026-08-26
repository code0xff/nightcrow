//! The HTML preview document: the one API response that is a repository file
//! served as itself.
//!
//! The file pane used to inline the file into a `srcdoc` frame, but a
//! local-scheme document inherits the embedder's CSP, whose `script-src
//! 'self'` refuses the inline scripts a self-contained page is made of — an
//! HTML slide deck rendered but never ran. Only a network response carries a
//! policy of its own, which is what this endpoint exists to attach.
//!
//! What that policy opens, and what it keeps shut:
//!
//! - **`sandbox allow-scripts`** gives the document an opaque origin even
//!   though its URL is this server's. Scripts run, but the document is
//!   nobody: no cookie jar, nothing of the app's DOM or storage, and every
//!   request it makes arrives unauthenticated (`SameSite=Strict`) with
//!   `Origin: null` — which `origin_allowed` refuses before auth is even
//!   consulted, the terminal WebSocket included.
//! - **`script-src 'unsafe-inline'`** is the point of the endpoint: inline
//!   scripts run. No host source stands beside it, so no script is fetched
//!   from anywhere to run.
//! - **`connect-src 'none'`** closes fetch and WebSocket outright, so the
//!   frame cannot phone any host — this server included. Subresources are
//!   `data:` or refused (`default-src 'none'`), keeping the standing rule
//!   that a preview never loads from another host.
//! - **`frame-ancestors 'self'`** keeps other origins from embedding it.
//!
//! The iframe that loads this keeps its own `sandbox="allow-scripts"`
//! attribute too: header and attribute intersect, so either one failing an
//! old browser or a future edit still leaves the other standing.
//!
//! One more belt for one more brace: a *top-level* navigation to this URL — a
//! pasted link, not an embed — is served the file as inert `text/plain`. This
//! closes the case a browser that ignored the CSP `sandbox` (none in a decade,
//! but the header is our only wall against it) would otherwise open: a
//! repository file executed as a *first-party* document with the session
//! cookie. The signal is `Sec-Fetch-Dest: document`, set by the browser on a
//! top-level navigation and unforgeable from script.
//!
//! It fails *open*: a request that carries no Fetch metadata is treated as an
//! embed and gets the executable document. That is deliberate, because browsers
//! send `Sec-Fetch` only from a potentially-trustworthy origin (HTTPS or
//! localhost) — so every plain-HTTP origin omits it, and the viewer reached
//! over a LAN or Tailscale address is exactly that. Failing closed there served
//! the raw source instead of the page on the whole mobile path. On that path
//! the CSP `sandbox` header stands alone — as it already does everywhere; this
//! gate only ever added a second wall where the metadata exists to raise it.
//!
//! What no policy here closes: a script may navigate its own frame away — to
//! an external URL (carrying its own source, which its author already has) or
//! to a phishing page in the pane. That is inherent to allowing scripts, is
//! recorded as an accepted residual in `docs/architecture/web.md`, and is why
//! the boundary this file defends is "the frame cannot reach the *session*",
//! not "the frame cannot emit anything".

/// See the module doc for why each directive is what it is.
const PREVIEW_CSP: &str = "sandbox allow-scripts; \
default-src 'none'; \
script-src 'unsafe-inline'; \
style-src 'unsafe-inline'; \
img-src data:; \
font-src data:; \
media-src data:; \
connect-src 'none'; \
form-action 'none'; \
base-uri 'none'; \
frame-ancestors 'self'";

/// `GET /api/preview?repo&path[&oid]` — the file as the working tree holds it,
/// or as the named commit does. The same loaders as `/api/file` and
/// `/api/commit/file`, so it passes exactly the path gates they do.
pub(super) fn route(
    head: &crate::web::common::http::RequestHead,
    state: &super::ViewerState,
) -> Vec<u8> {
    use super::handlers::{open_repo, optional_oid, required_path, with_repo};
    // Only an explicit top-level navigation gets the inert view; a frame embed
    // — or a client that sends no Fetch metadata at all, as every plain-HTTP
    // origin does — gets the executable document. See the module doc.
    let top_level_nav = head.header("sec-fetch-dest") == Some("document");
    with_repo(head, state, |entry| {
        let path = required_path(head)?;
        let repo = open_repo(&entry.path)?;
        let content = match optional_oid(head, "oid")? {
            Some(oid) => crate::git::diff::load_commit_file(&repo, oid, &path)?,
            None => crate::git::diff::load_workdir_file(&repo, &path)?,
        };
        Ok(if top_level_nav {
            inert_response(&content)
        } else {
            preview_response(&content)
        })
    })
}

/// The file as `text/plain`, for an explicit top-level navigation. Nothing
/// executes: a browser that reached this by a top-level navigation sees the
/// source, not a first-party page running with the session's cookie.
fn inert_response(source: &str) -> Vec<u8> {
    crate::web::common::http::response(
        "200 OK",
        "text/plain; charset=utf-8",
        &[
            ("X-Content-Type-Options", "nosniff"),
            ("Referrer-Policy", "no-referrer"),
            ("Cache-Control", "no-store"),
        ],
        source.as_bytes(),
    )
}

/// A repository file served as an HTML document under the preview policy.
fn preview_response(html: &str) -> Vec<u8> {
    crate::web::common::http::response(
        "200 OK",
        "text/html; charset=utf-8",
        &[
            ("Content-Security-Policy", PREVIEW_CSP),
            ("X-Content-Type-Options", "nosniff"),
            ("Referrer-Policy", "no-referrer"),
            // Session-gated content; no shared cache may keep it.
            ("Cache-Control", "no-store"),
        ],
        html.as_bytes(),
    )
}
