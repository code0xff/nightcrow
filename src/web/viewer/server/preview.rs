//! The HTML preview document: the one API response that is a repository file
//! served as itself.
//!
//! A `srcdoc` frame inherits the embedder's CSP, whose `script-src 'self'`
//! refuses the inline scripts a self-contained page is made of — an HTML slide
//! deck rendered but never ran. Only a network response carries a policy of
//! its own, which is what this endpoint exists to attach.
//!
//! The policy's shape: `sandbox allow-scripts` gives the document an opaque
//! origin (no cookies, no app DOM/storage; its requests arrive unauthenticated
//! with `Origin: null`, which `origin_allowed` refuses before auth is even
//! consulted). `script-src 'unsafe-inline'` is the point of the endpoint, with
//! no host source beside it; `connect-src 'none'` closes fetch and WebSocket
//! outright; `frame-ancestors 'self'` keeps other origins from embedding it.
//! The iframe's own `sandbox="allow-scripts"` attribute intersects with the
//! header, so either one failing still leaves the other standing.
//!
//! A *top-level* navigation to this URL — a pasted link — is served the file
//! as inert `text/plain`, signalled by the unforgeable `Sec-Fetch-Dest:
//! document`. Otherwise a browser ignoring CSP `sandbox` would execute a
//! repository file as a *first-party* document with the session cookie. It
//! fails *open*: browsers send `Sec-Fetch` only from a potentially-trustworthy
//! origin, so every plain-HTTP origin omits it and the viewer reached over a
//! LAN or Tailscale address is exactly that — failing closed there broke the
//! whole mobile path. On that path the CSP `sandbox` header stands alone, as
//! it already does everywhere.
//!
//! What no policy here closes: a script may navigate its own frame away — to
//! an external URL or a phishing page in the pane. That is inherent to
//! allowing scripts, is recorded as an accepted residual in
//! `docs/architecture/web.md`, and is why the boundary this file defends is
//! "the frame cannot reach the *session*", not "the frame cannot emit anything".

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

/// The file as `text/plain` for an explicit top-level navigation. Nothing
/// executes: no first-party page running with the session's cookie.
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
