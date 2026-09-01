//! The client's half of the shared tab list.
//!
//! The daemon owns the tab list — which repositories are open, their order,
//! which is in front. This client asks and adopts whatever comes back,
//! including changes another client made; what stays local is everything
//! *inside* a project (view mode, cursor, scroll).

use crate::application::bootstrap::init_app;
use crate::application::input::dispatch::{ProjectContext, ProjectRequest};
use crate::daemon::client::DaemonClient;
use crate::daemon::protocol::{RepoSummary, ServerMessage};
use crate::session::terminal::frame::ServerMessage as HubServerMessage;
use crate::workspace::Workspace;

pub(crate) struct SessionLink {
    client: DaemonClient,
}

impl SessionLink {
    pub(crate) fn new(client: DaemonClient) -> Self {
        Self { client }
    }

    pub(crate) fn sync(&mut self, ws: &mut Workspace, ctx: &ProjectContext) -> bool {
        let mut changed = false;
        for message in self.client.drain() {
            match message {
                ServerMessage::Repos {
                    repos,
                    active,
                    accent,
                } => {
                    adopt(ws, ctx, &repos, &self.client);
                    // After the set has settled, because the tab to put in front
                    // may be one this very message created — which is the usual
                    // case, since opening a repository focuses it.
                    if let Some(active) = active {
                        focus_repo(ws, &active);
                    }
                    // Adopted whether or not this client asked: the colour may
                    // have been picked in a browser, or in another terminal.
                    ws.set_accent_index(accent);
                    changed = true;
                }
                // A refusal this client asked for — a path that is not a
                // directory, or one repository too many.
                ServerMessage::Error { message } => {
                    ws.raise_notice(crate::app::NoticeKind::Project, message);
                    changed = true;
                }
                // Shown where the refusal above is shown, because the two are the
                // same answer to the same request.
                ServerMessage::Reloaded { summary } => {
                    ws.raise_notice(crate::app::NoticeKind::Session, summary);
                    changed = true;
                }
                // Answered during the handshake; a later one would mean the
                // daemon restarted under this client.
                ServerMessage::Hello { .. } => {}
                // A status connection closes before attachment, so this is not
                // a response the long-lived session client can request.
                ServerMessage::Status { .. } => {}
                // Only a refusal reaches here. A pane created, exited, or
                // reordered goes to that repository's backend.
                ServerMessage::Terminal { repo, event } => {
                    if let HubServerMessage::Error { message } = event {
                        notify_repo(ws, &repo, message);
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    pub(crate) fn request(&mut self, ws: &mut Workspace, request: ProjectRequest) {
        let sent = match request {
            // Which project is in front is the session's, so this asks. Nothing
            // moves locally in the meantime: switching optimistically and then
            // being corrected would show a tab flicking past on every switch.
            ProjectRequest::Switch(index) => {
                match ws
                    .projects()
                    .get(index)
                    .and_then(|app| app.repository_id().map(str::to_string))
                {
                    Some(id) => self.client.focus_repo(&id),
                    // A tab the daemon has not named yet — it is a beat from
                    // arriving, and there is nothing to ask about.
                    None => return,
                }
            }
            // Resolved to an id here rather than to an index sent onward,
            // because the wrap and the tab it lands on are the same question.
            ProjectRequest::Cycle { forward } => match cycle_target(ws, forward) {
                Some(id) => self.client.focus_repo(&id),
                None => return,
            },
            // Tab order is the session's too, so the whole new order is asked
            // for and nothing is rearranged here. Adopting the broadcast is
            // what moves the tab, exactly as switching does.
            ProjectRequest::Move { forward } => match moved_order(ws, forward) {
                Some(order) => self.client.reorder_repos(&order),
                None => return,
            },
            // The dialog is this client's own; only what it confirms is a
            // request.
            ProjectRequest::OpenDialog => {
                ws.start_repo_input();
                return;
            }
            // Opening focuses in the daemon, so the tab comes forward with the
            // set rather than needing to be chased here.
            ProjectRequest::Open(path) => self.client.open_repo(&path),
            // Closing is by id, so a tab with no id is not one the daemon knows
            // about and closing it locally would only hide it until the next
            // broadcast put it back.
            ProjectRequest::Close => match ws
                .active()
                .and_then(|app| app.repository_id().map(str::to_string))
            {
                Some(id) => self.client.close_repo(&id),
                None => return,
            },
            // The step is resolved here into the colour it lands on, so two
            // clients cycling at once agree on the result instead of each
            // advancing the session from what it last showed them.
            ProjectRequest::CycleAccent => self.client.set_accent(ws.next_accent_index()),
            // Nothing is shown optimistically. Unlike the accent this is not a
            // wait to avoid a flicker — there is simply nothing to show until the
            // session says what it read, and guessing would mean claiming a file
            // applied before anything had parsed it.
            ProjectRequest::ReloadConfig => self.client.reload_config(),
        };
        if let Err(err) = sent {
            ws.raise_notice(
                crate::app::NoticeKind::Project,
                format!("daemon request failed: {err}"),
            );
        }
    }

    pub(crate) fn is_connected(&self) -> bool {
        self.client.is_connected()
    }
}

/// Put the tab for `repo` in front, by catalog id. Reports whether there was one.
///
/// A miss is normal rather than an error: the session can name a repository this
/// client has not built a tab for yet, in the beat between the two.
fn focus_repo(ws: &mut Workspace, repo: &str) -> bool {
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
fn cycle_target(ws: &Workspace, forward: bool) -> Option<String> {
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
fn moved_order(ws: &Workspace, forward: bool) -> Option<Vec<String>> {
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
fn notify_repo(ws: &mut Workspace, repo: &str, message: String) {
    match ws
        .projects_mut()
        .iter_mut()
        .find(|project| project.repository_id() == Some(repo))
    {
        Some(project) => project.raise_notice(crate::app::NoticeKind::Terminal, message),
        None => ws.raise_notice(crate::app::NoticeKind::Terminal, message),
    }
}

/// Make the workspace match the set the daemon reports.
///
/// Membership first, then order, then the ids — a tab that stays open keeps its
/// terminals, scroll, and selection, so reconciling in place matters more than
/// rebuilding from scratch would.
fn adopt(ws: &mut Workspace, ctx: &ProjectContext, repos: &[RepoSummary], client: &DaemonClient) {
    // Closing first frees room under `MAX_PROJECTS` for what is being opened.
    let wanted: Vec<&str> = repos.iter().map(|repo| repo.path.as_str()).collect();
    let doomed: Vec<String> = ws
        .projects()
        .iter()
        .map(|project| project.repository_path().to_string())
        .filter(|path| !wanted.contains(&path.as_str()))
        .collect();
    for path in doomed {
        ws.close_repo(&path);
    }
    for repo in repos {
        if ws.index_of_repo(&repo.path).is_some() {
            continue;
        }
        if ws.is_full() {
            // The daemon's cap is the same number, so this means the two
            // disagree about what is open — worth saying rather than dropping
            // the tab silently.
            ws.raise_notice(
                crate::app::NoticeKind::Project,
                format!(
                    "cannot open more than {} projects",
                    crate::workspace::MAX_PROJECTS
                ),
            );
            break;
        }
        let saved = ws.session_for(&repo.path).cloned();
        ws.add(init_app(
            &repo.path,
            ctx.cfg,
            ctx.leader,
            saved,
            // The tab's panes are the session's. Built with the repository's own
            // end of the connection, so the terminals it shows are the ones the
            // daemon is running and the browser is looking at.
            Box::new(crate::backend::HubBackend::new(
                client.terminal_link(&repo.id),
            )),
        ));
    }
    ws.reorder_to(&wanted);
    // Recorded after the set settles: the id is how this client names a
    // repository back to the daemon, and a tab carrying a stale one would ask
    // to close something else.
    for repo in repos {
        ws.set_repo_id(&repo.path, &repo.id);
    }
    // Whatever the daemon has been streaming for a repository that is not in
    // the set has no tab to reach; its inbox goes with the tab.
    let ids: Vec<String> = repos.iter().map(|repo| repo.id.clone()).collect();
    client.retain_repos(&ids);
}

#[cfg(test)]
#[path = "session_link_tests.rs"]
mod tests;
