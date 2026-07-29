//! The client's half of the shared tab list.
//!
//! The daemon owns which repositories are open and in what order. This client
//! asks for a change and adopts whatever comes back — including changes another
//! client made, which arrive with nothing having asked.
//!
//! Which tab is in front is the daemon's too, so switching is a request and
//! every client follows the answer. What stays local is everything *inside* a
//! project — the view mode, the cursor, the scroll — which is what makes two
//! clients on one session more than two copies of one screen.

use crate::application::bootstrap::init_app;
use crate::application::input::dispatch::{ProjectContext, ProjectRequest};
use crate::daemon::client::DaemonClient;
use crate::daemon::protocol::{RepoSummary, ServerMessage};
use crate::web::viewer::terminal::frame::ServerMessage as HubServerMessage;
use crate::workspace::Workspace;

pub(crate) struct SessionLink {
    client: DaemonClient,
}

impl SessionLink {
    pub(crate) fn new(client: DaemonClient) -> Self {
        Self { client }
    }

    /// Take in everything the daemon has said since the last tick.
    pub(crate) fn sync(&mut self, ws: &mut Workspace, ctx: &ProjectContext) {
        for message in self.client.drain() {
            match message {
                ServerMessage::Repos { repos, active } => {
                    adopt(ws, ctx, &repos, &self.client);
                    // After the set has settled, because the tab to put in front
                    // may be one this very message created — which is the usual
                    // case, since opening a repository focuses it.
                    if let Some(active) = active {
                        focus_repo(ws, &active);
                    }
                }
                // A refusal this client asked for — a path that is not a
                // directory, or one repository too many. Shown where every
                // other refusal is shown.
                ServerMessage::Error { message } => {
                    ws.raise_notice(crate::app::NoticeKind::Project, message);
                }
                // Answered during the handshake; a later one would mean the
                // daemon restarted under this client, which the connection loss
                // reports on its own.
                ServerMessage::Hello { .. } => {}
                // Only a refusal reaches here. A pane created, exited, or
                // reordered goes to that repository's backend, which is what
                // renders it; a refusal is about a request rather than a pane,
                // so it belongs on the tab that shows notices.
                ServerMessage::Terminal { repo, event } => {
                    if let HubServerMessage::Error { message } = event {
                        notify_repo(ws, &repo, message);
                    }
                }
            }
        }
    }

    /// Carry out a tab request locally, or send it to the daemon.
    pub(crate) fn request(&mut self, ws: &mut Workspace, request: ProjectRequest) {
        let sent = match request {
            // Which project is in front is the session's, so this asks. Nothing
            // moves locally in the meantime: switching optimistically and then
            // being corrected would show a tab flicking past on every switch.
            ProjectRequest::Switch(index) => {
                match ws.projects().get(index).and_then(|app| app.repo_id.clone()) {
                    Some(id) => self.client.focus_repo(&id),
                    // A tab the daemon has not named yet — it is a beat from
                    // arriving, and there is nothing to ask about.
                    None => return,
                }
            }
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
            ProjectRequest::Close => match ws.active().and_then(|app| app.repo_id.clone()) {
                Some(id) => self.client.close_repo(&id),
                None => return,
            },
        };
        if let Err(err) = sent {
            ws.raise_notice(
                crate::app::NoticeKind::Project,
                format!("daemon request failed: {err}"),
            );
        }
    }

    /// Whether the daemon is still there.
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
        .position(|app| app.repo_id.as_deref() == Some(repo))
    {
        Some(index) => {
            ws.switch(index);
            true
        }
        None => false,
    }
}

/// Raise a terminal refusal on the tab it came from.
///
/// By repository, not on the active tab: the client subscribes to every open
/// repository's terminals, so a refusal can be about one the user is not looking
/// at, and putting it on whatever tab is in front would name the wrong project.
/// A repository with no tab yet falls back to the active one rather than losing
/// the message.
fn notify_repo(ws: &mut Workspace, repo: &str, message: String) {
    match ws
        .projects_mut()
        .iter_mut()
        .find(|project| project.repo_id.as_deref() == Some(repo))
    {
        Some(project) => project.raise_notice(crate::app::NoticeKind::Terminal, message),
        None => ws.raise_notice(crate::app::NoticeKind::Terminal, message),
    }
}

/// Make the workspace match the set the daemon reports.
///
/// Membership first, then order, then the ids — a tab that stays open keeps its
/// terminals, scroll, and selection, so reconciling in place matters more than
/// it would if this rebuilt from scratch.
fn adopt(ws: &mut Workspace, ctx: &ProjectContext, repos: &[RepoSummary], client: &DaemonClient) {
    // Closing first frees room under `MAX_PROJECTS` for what is being opened,
    // so a set that swaps one repository for another fits in a single pass.
    let wanted: Vec<&str> = repos.iter().map(|repo| repo.path.as_str()).collect();
    let doomed: Vec<String> = ws
        .projects()
        .iter()
        .map(|project| project.repo_path.clone())
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
