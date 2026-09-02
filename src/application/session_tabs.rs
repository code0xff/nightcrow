//! The pure tab-order and tab-focus decisions.
//!
//! Kept apart from the connection so each choice — which tab comes forward,
//! which id a step lands on, what new order a move asks for — can be driven by
//! tests without a daemon, which is what the tests attached below do.

use crate::workspace::Workspace;

/// Put the tab for `repo` in front, by catalog id. Reports whether there was one.
///
/// A miss is normal rather than an error: the session can name a repository this
/// client has not built a tab for yet, in the beat between the two.
pub(super) fn focus_repo(ws: &mut Workspace, repo: &str) -> bool {
    match ws
        .projects()
        .iter()
        .position(|app| app.repository_id() == Some(repo))
    {
        Some(index) => {
            ws.switch(index);
            true
        }
        None => false,
    }
}

/// The catalog id of the tab one step from the front, wrapping over tab order.
///
/// `None` for every case with nothing to ask about: no tabs, a single tab —
/// where either direction lands back on the one already in front — and a tab
/// the daemon has not named yet, the same early-out closing has. Takes
/// `&Workspace` because stepping is a request; the tab moves when the daemon
/// rebroadcasts the set, never here.
pub(super) fn cycle_target(ws: &Workspace, forward: bool) -> Option<String> {
    let len = ws.projects().len();
    if len <= 1 {
        return None;
    }
    // Backward as a forward step of `len - 1` so the arithmetic stays in
    // unsigned space and wrapping past zero needs no special case.
    let step = if forward { 1 } else { len - 1 };
    let target = (ws.active_index() + step) % len;
    ws.projects()[target].repository_id().map(str::to_string)
}

/// The full tab order with the front tab swapped past its neighbour, by catalog
/// id, or `None` when there is nothing to ask for.
///
/// `None` at every boundary: fewer than two tabs, the front tab already at the
/// end it is pushed towards — no wrap, because wrapping is the stepping chord's
/// meaning and a held key would otherwise shuffle the strip — and any tab the
/// daemon has not named yet, since the session appends whatever an order leaves
/// out (`CatalogMembership::reorder`), so a partial order would send that
/// repository to the back of the strip. Takes `&Workspace` because reordering is
/// a request; the tabs move when the daemon rebroadcasts the set, never here.
pub(super) fn moved_order(ws: &Workspace, forward: bool) -> Option<Vec<String>> {
    let mut order: Vec<String> = ws
        .projects()
        .iter()
        .map(|project| project.repository_id().map(str::to_string))
        .collect::<Option<Vec<String>>>()?;
    // Both ends bounded by the order just built, stated once: the forward step
    // needs the length anyway, and reading it from the same place keeps the
    // backward step from being the one that can address past the end, where
    // `swap` would panic in the event loop rather than answer nothing.
    let last = order.len().checked_sub(1)?;
    let active = ws.active_index();
    let neighbour = if forward {
        (active < last).then(|| active + 1)
    } else {
        active.checked_sub(1).filter(|_| active <= last)
    }?;
    order.swap(active, neighbour);
    Some(order)
}

/// Raise a terminal refusal on the tab it came from, not the active one: the
/// client subscribes to every open repository, so the refusal may be about a
/// tab the user is not looking at. A repository with no tab yet falls back to
/// the active one rather than losing the message.
pub(super) fn notify_repo(ws: &mut Workspace, repo: &str, message: String) {
    match ws
        .projects_mut()
        .iter_mut()
        .find(|project| project.repository_id() == Some(repo))
    {
        Some(project) => project.raise_notice(crate::app::NoticeKind::Terminal, message),
        None => ws.raise_notice(crate::app::NoticeKind::Terminal, message),
    }
}

#[cfg(test)]
#[path = "session_link_tests.rs"]
mod tests;
