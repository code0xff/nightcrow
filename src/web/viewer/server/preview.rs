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

/// A repository file served as an HTML document under the preview policy. The
/// `ETag` is the file's blob oid, which the editor reads back and hands to the
/// save and edit-preview routes as the version its edits began from.
fn preview_response(html: &str) -> Vec<u8> {
    let etag = format!("\"{}\"", blob_oid(html.as_bytes()));
    crate::web::common::http::response(
        "200 OK",
        "text/html; charset=utf-8",
        &[
            ("Content-Security-Policy", PREVIEW_CSP),
            ("X-Content-Type-Options", "nosniff"),
            ("Referrer-Policy", "no-referrer"),
            ("ETag", &etag),
            // Session-gated content; no shared cache may keep it.
            ("Cache-Control", "no-store"),
        ],
        html.as_bytes(),
    )
}

/// The git blob oid of some bytes — the version identity the editor compares by,
/// the same one `POST /api/file` uses.
fn blob_oid(bytes: &[u8]) -> String {
    git2::Oid::hash_object(git2::ObjectType::Blob, bytes)
        .map(|oid| oid.to_string())
        .unwrap_or_default()
}

/// `POST /api/preview/edit?repo&path[&oid]` — assemble the editable preview.
///
/// The editor sends the small insert list (a marker per block, plus the head
/// payload carrying the agent), each at a UTF-8 byte offset, and the blob oid
/// its parse began from. The server re-reads the file, refuses `409` if it no
/// longer hashes to that oid (so a marker cannot land on shifted bytes), splices
/// the inserts in, and stashes the result under a one-time token for the frame
/// to load. The document is preview only — nothing here is written to disk.
pub(super) fn stash_edit(
    head: &crate::web::common::http::RequestHead,
    body: &str,
    state: &super::ViewerState,
) -> Vec<u8> {
    use super::handlers::{open_repo, optional_oid, required_path, with_repo};
    use super::http_util::{json_error, json_response};

    #[derive(serde::Deserialize)]
    struct Request {
        inserts: Vec<Insert>,
        base_hash: String,
    }
    #[derive(serde::Deserialize)]
    struct Insert {
        at: usize,
        text: String,
    }

    let request: Request = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => return json_error("400 Bad Request", "expected inserts and baseHash"),
    };
    with_repo(head, state, |entry| {
        let path = required_path(head)?;
        let repo = open_repo(&entry.path)?;
        let content = match optional_oid(head, "oid")? {
            Some(oid) => crate::git::diff::load_commit_file(&repo, oid, &path)?,
            None => crate::git::diff::load_workdir_file(&repo, &path)?,
        };
        let current = blob_oid(content.as_bytes());
        if current != request.base_hash {
            return Ok(json_response(
                "409 Conflict",
                &format!("{{\"error\":\"stale\",\"currentHash\":\"{current}\"}}"),
                &[],
            ));
        }
        let points: Vec<(usize, String)> = request
            .inserts
            .into_iter()
            .map(|i| (i.at, i.text))
            .collect();
        let assembled = match apply_inserts(&content, points) {
            Ok(html) => html,
            Err(message) => return Ok(json_error("400 Bad Request", message)),
        };
        match state.edit_previews.stash(assembled) {
            Ok(token) => {
                let payload = serde_json::json!({ "token": token });
                match super::http_util::encode(&payload) {
                    Ok(json) => Ok(json_response("200 OK", &json, &[])),
                    Err(_) => Ok(json_error(
                        "500 Internal Server Error",
                        "could not encode the token",
                    )),
                }
            }
            Err(_) => Ok(json_error(
                "500 Internal Server Error",
                "could not start a preview",
            )),
        }
    })
}

/// Splice each `(byte offset, text)` insertion into `source`. Applied from the
/// back so earlier offsets do not shift, and every offset must land on a UTF-8
/// character boundary within the source — a guarantee the blob-oid check above
/// already gives, checked again here so a bad offset is a clean error, never a
/// panic.
fn apply_inserts(source: &str, mut points: Vec<(usize, String)>) -> Result<String, &'static str> {
    points.sort_by_key(|point| std::cmp::Reverse(point.0));
    let mut out = source.to_string();
    for (at, text) in points {
        if at > out.len() || !out.is_char_boundary(at) {
            return Err("an insertion offset is out of range");
        }
        out.insert_str(at, &text);
    }
    Ok(out)
}

/// `GET /api/preview/edit?token=…` — serve a stashed editable preview once,
/// under the same policy as a plain preview. A missing token (used, expired, or
/// never issued) is a 404, not an error.
pub(super) fn serve_edit(
    head: &crate::web::common::http::RequestHead,
    state: &super::ViewerState,
) -> Vec<u8> {
    // An assembled preview is only ever meant for the editor's frame. A
    // top-level navigation to it — a pasted link — is refused for the same
    // reason a plain preview turns inert there: a browser ignoring the CSP
    // sandbox would otherwise run it as a first-party document. Refused rather
    // than served inert, since raw markers and an agent are of no use to read,
    // and the token would be spent showing them.
    if head.header("sec-fetch-dest") == Some("document") {
        return super::http_util::json_error("403 Forbidden", "not from this context");
    }
    let Some(token) = head.query_param("token").filter(|t| !t.is_empty()) else {
        return super::http_util::json_error("400 Bad Request", "missing token parameter");
    };
    match state.edit_previews.take(&token) {
        Some(html) => preview_response(&html),
        None => super::http_util::json_error("404 Not Found", "no such preview"),
    }
}
