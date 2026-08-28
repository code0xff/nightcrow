//! What the sizing did, written down. It changes what *every* attached client
//! renders, it moves for reasons no client can observe (a last connection
//! going, a grace expiring on a worker tick), and a client that loses it stays
//! wrong until a person presses the fit button — so with nothing recorded there
//! is only the symptom to read afterwards. INFO rather than DEBUG: these happen
//! per page load, not per frame, and the moment they are wanted is a report
//! about something that already happened — too late to raise the level.

use super::ViewerId;

/// How a viewer appears in a line. The browser half is client-supplied, so it
/// is tagged rather than printed bare.
fn label(viewer: &ViewerId) -> String {
    match viewer {
        ViewerId::Browser(id) => format!("browser:{id}"),
        ViewerId::Attached(id) => format!("tui:{id}"),
    }
}

fn or_nobody(viewer: Option<&ViewerId>) -> String {
    viewer.map_or_else(|| "nobody".to_string(), label)
}

/// `arriving` is the client's own word for a person sitting down; it is the
/// difference between a page opening and a socket reconnecting, and only the
/// client can tell them apart.
pub(super) fn joined(viewer: &ViewerId, connection: u64, arriving: bool) {
    tracing::info!(
        viewer = %label(viewer),
        connection,
        arriving,
        "viewer connection joined"
    );
}

/// `last` says this was the viewer's only remaining connection — the one case
/// that starts the release grace.
pub(super) fn left(viewer: &ViewerId, connection: u64, last: bool) {
    tracing::info!(
        viewer = %label(viewer),
        connection,
        last,
        "viewer connection left"
    );
}

pub(super) fn moved(from: Option<&ViewerId>, to: Option<&ViewerId>, reason: &'static str) {
    tracing::info!(
        from = %or_nobody(from),
        to = %or_nobody(to),
        reason,
        "terminal sizing moved"
    );
}
