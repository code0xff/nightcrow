use crate::app::App;
use crate::workspace::persistence::SessionState;

pub(crate) fn init_app(
    repo_path: &str,
    cfg: &crate::config::Config,
    leader: crossterm::event::KeyEvent,
    saved_session: Option<SessionState>,
    backend: Box<dyn crate::backend::TerminalBackend>,
) -> App {
    let mut app = App::new(repo_path.to_string(), cfg.log.prompt_log, leader, backend);
    app.cfg_agent_indicator = cfg.agent_indicator.clone();
    app.cfg_tree = cfg.tree.clone();
    app.interaction.mouse_enabled = cfg.mouse.enabled;
    if cfg.tree.live_watch {
        app.tree_watch = crate::runtime::tree_watch::TreeWatcher::new();
    }
    app.configure_commit_log(
        cfg.log.commit_log_page_size,
        cfg.log.commit_log_prefetch_threshold,
    );
    if let Some(state) = saved_session {
        // Applied up front rather than on the first snapshot: only the Status
        // selection needs the changed-file list, and it waits in
        // `pending_selection` (see `App::restore_session`). The terminal half
        // waits for the panes to arrive from the session, which replaces the
        // fresh-launch default rather than fighting it.
        app.restore_session(&state);
    }
    app
}
