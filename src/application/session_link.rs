//! The client's half of the shared tab list.
//!
//! The daemon owns the tab list — which repositories are open, their order,
//! which is in front. This client asks and adopts whatever comes back,
//! including changes another client made; what stays local is everything
//! *inside* a project (view mode, cursor, scroll).

use crate::application::bootstrap::init_app;
use crate::application::input::dispatch::{ProjectContext, ProjectRequest};
use crate::application::session_tabs::{cycle_target, focus_repo, moved_order, notify_repo};
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
        ws.set_clone_available(self.client.can_clone());
        for message in self.client.drain() {
            match message {
                ServerMessage::Repos {
                    repos,
                    active,
                    accent,
                } => {
                    let repo_paths: Vec<String> =
                        repos.iter().map(|repo| repo.path.clone()).collect();
                    ws.clone_open_confirmed(&repo_paths);
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
                    ws.clone_open_failed();
                    ws.raise_notice(crate::app::NoticeKind::Project, message);
                    changed = true;
                }
                // Shown where the refusal above is shown, because the two are the
                // same answer to the same request.
                ServerMessage::Reloaded { summary } => {
                    ws.raise_notice(crate::app::NoticeKind::Session, summary);
                    changed = true;
                }
                ServerMessage::CloneStarted { job, .. } => {
                    ws.clone_started(job);
                    changed = true;
                }
                ServerMessage::CloneStatus { lookup } => {
                    if let Some(path) = ws.apply_clone_lookup(lookup) {
                        match self.client.open_repo(&path) {
                            Ok(()) => {
                                ws.clone_open_is_present();
                            }
                            Err(err) => {
                                ws.clone_open_failed();
                                ws.raise_notice(
                                    crate::app::NoticeKind::Project,
                                    format!(
                                        "clone finished at {path}, but could not open it: {err}"
                                    ),
                                );
                            }
                        }
                    }
                    changed = true;
                }
                ServerMessage::CloneRejected { message } => {
                    ws.clone_rejected(&message);
                    changed = true;
                }
                // Answered during the handshake; a later one would mean the
                // daemon restarted under this client.
                ServerMessage::Hello { .. } => {}
                // A status connection closes before attachment, so this is not
                // a response the long-lived session client can request.
                ServerMessage::Status { .. } | ServerMessage::Notified => {}
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
        if let Some(job) = ws.take_clone_poll(std::time::Instant::now())
            && let Err(err) = self.client.clone_status(job)
        {
            ws.raise_notice(
                crate::app::NoticeKind::Project,
                format!("could not check clone status: {err}"),
            );
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
            ProjectRequest::StartClone { parent, url } => {
                if let Err(err) = self.client.start_clone(&parent, &url) {
                    // A failed write does not prove the daemon failed to admit
                    // the clone; the status query can recover its running id.
                    ws.raise_notice(
                        crate::app::NoticeKind::Project,
                        format!("could not confirm the clone request: {err}"),
                    );
                    let _ = self.client.clone_status(None);
                }
                return;
            }
            ProjectRequest::CloneStatus { job } => {
                if let Err(err) = self.client.clone_status(job) {
                    ws.raise_notice(
                        crate::app::NoticeKind::Project,
                        format!("could not check clone status: {err}"),
                    );
                }
                return;
            }
            // The overlay is this client's own, like the dialog: nothing about
            // it is the session's business.
            ProjectRequest::ToggleHelp => {
                ws.help.toggle();
                return;
            }
            // Opening focuses in the daemon, so the tab comes forward with the
            // set rather than needing to be chased here.
            ProjectRequest::Open(path) => {
                let clone_result = ws.clone_open_requested(&path);
                match self.client.open_repo(&path) {
                    Ok(()) => {
                        if clone_result {
                            ws.clone_open_is_present();
                        }
                        return;
                    }
                    Err(err) => {
                        if clone_result {
                            ws.clone_open_failed();
                        }
                        Err(err)
                    }
                }
            }
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
