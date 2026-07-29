//! Where the tab list comes from.
//!
//! Two answers today. Run on its own, the TUI owns its tabs and mirrors them to
//! a viewer beside it. Attached, the daemon owns them: this client asks for a
//! change and adopts whatever comes back, including changes another client
//! made. The second is where this is going; the first goes away with the
//! single-process mode.

use crate::application::bootstrap::init_app;
use crate::application::input::dispatch::{ProjectContext, ProjectRequest};
use crate::daemon::client::DaemonClient;
use crate::daemon::protocol::{RepoSummary, ServerMessage};
use crate::workspace::Workspace;

pub(crate) enum SessionLink {
    /// The TUI owns its tabs. A viewer beside it follows them.
    Local {
        viewer: Option<crate::web::viewer::server::ViewerServer>,
        /// The set last handed to the viewer; the catalog only needs updating
        /// when a tab opens or closes, not every frame.
        served: Vec<String>,
    },
    /// The daemon owns the tabs. Requests go out, and the set comes back.
    Daemon(Box<DaemonClient>),
}

impl SessionLink {
    /// Bring the workspace and the far side into agreement for this tick.
    pub(crate) fn sync(&mut self, ws: &mut Workspace, ctx: &ProjectContext) {
        match self {
            SessionLink::Local { viewer, served } => {
                let Some(viewer) = viewer.as_ref() else {
                    return;
                };
                let current: Vec<String> =
                    ws.projects().iter().map(|p| p.repo_path.clone()).collect();
                if &current != served {
                    viewer.set_repos(&current);
                    *served = current;
                }
            }
            SessionLink::Daemon(client) => {
                for message in client.drain() {
                    match message {
                        ServerMessage::Repos { repos } => adopt(ws, ctx, &repos),
                        // A refusal this client asked for — a path that is not a
                        // directory, or one repository too many. Shown where
                        // every other refusal is shown.
                        ServerMessage::Error { message } => {
                            ws.raise_notice(crate::app::NoticeKind::Project, message);
                        }
                        // Answered during the handshake; a later one would mean
                        // the daemon restarted under this client, which the
                        // connection loss reports on its own.
                        ServerMessage::Hello { .. } => {}
                    }
                }
            }
        }
    }

    /// Carry out a tab request, or send it to whoever owns the tabs.
    pub(crate) fn request(
        &mut self,
        ws: &mut Workspace,
        ctx: &ProjectContext,
        request: ProjectRequest,
    ) {
        let SessionLink::Daemon(client) = self else {
            crate::application::event_loop::apply_project_request(ws, ctx, request);
            return;
        };
        // Switching tabs and opening the dialog change nothing the daemon owns:
        // which tab this client looks at is its own business, and every client
        // may look at a different one.
        let sent = match request {
            ProjectRequest::Switch(index) => {
                ws.switch(index);
                return;
            }
            ProjectRequest::OpenDialog => {
                ws.start_repo_input();
                return;
            }
            ProjectRequest::Open(path) => client.open_repo(&path),
            // Closing is by id, so a tab with no id is not one the daemon knows
            // about and closing it locally would only hide it until the next
            // broadcast put it back.
            ProjectRequest::Close => match ws.active().and_then(|app| app.repo_id.clone()) {
                Some(id) => client.close_repo(&id),
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

    /// Whether the far side is still there. Always true when there is no far
    /// side to lose.
    pub(crate) fn is_connected(&self) -> bool {
        match self {
            SessionLink::Local { .. } => true,
            SessionLink::Daemon(client) => client.is_connected(),
        }
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
