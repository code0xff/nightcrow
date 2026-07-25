use crate::app::App;
use crate::session::SessionState;

pub(crate) fn init_app(
    repo_path: &str,
    cfg: &crate::config::Config,
    startup_commands: &[crate::config::StartupCommand],
    leader: crossterm::event::KeyEvent,
    saved_session: Option<SessionState>,
) -> App {
    let mut app = App::new(
        repo_path.to_string(),
        cfg.log.prompt_log,
        startup_commands,
        leader,
    );
    app.set_accent_index(cfg.theme.preset_index());
    app.cfg_agent_indicator = cfg.agent_indicator.clone();
    app.cfg_tree = cfg.tree.clone();
    app.mouse_enabled = cfg.mouse.enabled;
    if cfg.tree.live_watch {
        app.tree_watch = crate::runtime::tree_watch::TreeWatcher::new();
    }
    app.pagination.page_size = cfg.log.commit_log_page_size;
    app.pagination.prefetch_threshold = cfg.log.commit_log_prefetch_threshold;
    if let Some(state) = saved_session {
        // Applied up front rather than on the first snapshot: only the Status
        // selection needs the changed-file list, and it waits in
        // `pending_selection` (see `App::restore_session`). Restoring here also
        // keeps the fresh-launch terminal focus set by `ensure_initial_terminal`
        // from drawing — or routing keystrokes — over the saved focus.
        app.restore_session(&state);
    }
    app
}
