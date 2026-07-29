//! The client's half of the shared tab list.
//!
//! The daemon owns which repositories are open and in what order. This client
//! asks for a change and adopts whatever comes back — including changes another
//! client made, which arrive with nothing having asked.
//!
//! What it does *not* ask about is which tab it is looking at. That is the
//! per-client half of the boundary: two clients on one session may sit on
//! different projects, so switching is local and immediate.

use crate::application::bootstrap::init_app;
use crate::application::input::dispatch::{ProjectContext, ProjectRequest};
use crate::daemon::client::DaemonClient;
use crate::daemon::protocol::{RepoSummary, ServerMessage};
use crate::workspace::Workspace;

pub(crate) struct SessionLink {
    client: DaemonClient,
    /// A repository this client asked to open, waiting to be focused.
    ///
    /// Opening is a request, so the tab does not exist yet when the request is
    /// made — and the answer is a whole set, which says nothing about who asked
    /// for what. Without this, opening a repository from the dialog would leave
    /// the user on the tab they were already on, and asking for one that is
    /// *already* open would look like the key did nothing.
    pending_focus: Option<String>,
}

impl SessionLink {
    pub(crate) fn new(client: DaemonClient) -> Self {
        Self {
            client,
            pending_focus: None,
        }
    }

    /// Take in everything the daemon has said since the last tick.
    pub(crate) fn sync(&mut self, ws: &mut Workspace, ctx: &ProjectContext) {
        for message in self.client.drain() {
            match message {
                ServerMessage::Repos { repos } => {
                    adopt(ws, ctx, &repos);
                    // Only after the set has settled: the tab being focused may
                    // be one this very message created.
                    if let Some(path) = self.pending_focus.take() {
                        focus_if_open(ws, &path);
                    }
                }
                // A refusal this client asked for — a path that is not a
                // directory, or one repository too many. Shown where every
                // other refusal is shown.
                ServerMessage::Error { message } => {
                    self.pending_focus = None;
                    ws.raise_notice(crate::app::NoticeKind::Project, message);
                }
                // Answered during the handshake; a later one would mean the
                // daemon restarted under this client, which the connection loss
                // reports on its own.
                ServerMessage::Hello { .. } => {}
                // Panes are not shared yet — the client still runs its own
                // PTYs, so there is nothing here for these to act on. Dropped
                // rather than treated as a fault so the daemon can start
                // sending them before this side reads them.
                ServerMessage::Terminal { .. } => {}
            }
        }
    }

    /// Carry out a tab request locally, or send it to the daemon.
    pub(crate) fn request(&mut self, ws: &mut Workspace, request: ProjectRequest) {
        let sent = match request {
            // Neither touches anything the daemon owns.
            ProjectRequest::Switch(index) => {
                ws.switch(index);
                return;
            }
            ProjectRequest::OpenDialog => {
                ws.start_repo_input();
                return;
            }
            ProjectRequest::Open(path) => {
                // Recorded before the send, and by the path as typed: the
                // daemon resolves it to a worktree root, so the match is made
                // against the answer rather than assumed here.
                self.pending_focus = Some(path.clone());
                self.client.open_repo(&path)
            }
            // Closing is by id, so a tab with no id is not one the daemon knows
            // about and closing it locally would only hide it until the next
            // broadcast put it back.
            ProjectRequest::Close => match ws.active().and_then(|app| app.repo_id.clone()) {
                Some(id) => self.client.close_repo(&id),
                None => return,
            },
        };
        if let Err(err) = sent {
            self.pending_focus = None;
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

/// Move to the tab on `repo` if there is one. Reports whether there was.
///
/// A miss is normal rather than an error: the daemon resolves a path to a
/// worktree root, so a repository opened as `.` comes back under its real
/// path, and one that failed to open never arrives at all.
fn focus_if_open(ws: &mut Workspace, repo: &str) -> bool {
    match ws.index_of_repo(repo) {
        Some(index) => {
            ws.switch(index);
            true
        }
        None => false,
    }
}

/// Make the workspace match the set the daemon reports.
///
/// Membership first, then order, then the ids — a tab that stays open keeps its
/// terminals, scroll, and selection, so reconciling in place matters more than
/// it would if this rebuilt from scratch.
fn adopt(ws: &mut Workspace, ctx: &ProjectContext, repos: &[RepoSummary]) {
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
            ctx.startup_commands,
            ctx.leader,
            saved,
            Box::new(crate::backend::PtyBackend::new(&repo.path)),
        ));
    }
    ws.reorder_to(&wanted);
    // Recorded after the set settles: the id is how this client names a
    // repository back to the daemon, and a tab carrying a stale one would ask
    // to close something else.
    for repo in repos {
        ws.set_repo_id(&repo.path, &repo.id);
    }
}

#[cfg(test)]
#[path = "session_link_tests.rs"]
mod tests;
