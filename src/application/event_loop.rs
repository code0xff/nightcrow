pub(crate) use crate::application::input::dispatch::ProjectContext;
use crate::application::input::dispatch::{KeyOutcome, dispatch_key};
use crate::application::input::mouse::dispatch_mouse;
use crate::application::input::paste::dispatch_paste;
use crate::application::session_link::SessionLink;
use crate::workspace::Workspace;
use crossterm::event::{self, Event};
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::time::Duration;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

pub(crate) fn main_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ws: &mut Workspace,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    cfg: &crate::config::Config,
    ctx: &ProjectContext,
    mut link: SessionLink,
) -> anyhow::Result<()> {
    loop {
        // Whoever owns the tab list gets the first word each tick: attached,
        // the set may have changed under this client since the last frame, and
        // rendering a stale one would show a tab the session no longer has.
        link.sync(ws, ctx);
        if !link.is_connected() {
            tracing::info!("daemon connection lost");
            return Ok(());
        }
        // Every project drains its queues, not just the visible one: the
        // snapshot worker and PTY reader produce into unbounded channels
        // regardless of which tab is on screen, so skipping the background
        // ones would let them grow until the user switched back.
        //
        // Only the active project *applies* its snapshot, though. That runs a
        // full `refresh_diff`, and doing it for every open project would put
        // several repositories' git diffs on the UI thread every tick. A
        // background snapshot waits in `pending_snapshot` until its tab is
        // shown (see `App::drain_snapshot`).
        let active = ws.active_index();
        for (i, project) in ws.projects_mut().iter_mut().enumerate() {
            if i == active {
                project.poll_snapshot();
                // Applying a commit-log page can trigger a further prefetch and
                // load a commit diff synchronously, so it stays with the
                // snapshot as active-only work. A hidden project's in-flight
                // fetch is capped at one by `CommitLogPagination`, so its reply
                // can wait in the channel without growing.
                project.poll_commit_log_page_fetch();
            } else {
                project.drain_snapshot();
            }
            // Both are cheap drains that must run everywhere: the tree watcher
            // to keep OS filesystem events from piling up, the terminal to
            // consume PTY output before the pipe fills and blocks the child.
            // Acting on a watcher event rereads directories and previews a
            // file, so like the snapshot that is active-only; a hidden project
            // records the event and refreshes when its tab comes forward.
            if i == active {
                project.poll_tree_watcher();
            } else {
                project.drain_tree_watcher();
            }
            project.poll_terminal();
        }

        let size = terminal.size()?;
        let screen = Rect::new(0, 0, size.width, size.height);
        if let Some(app) = ws.active() {
            let layouts: Vec<(crate::backend::PaneId, u16, u16)> =
                crate::ui::terminal_content_areas(app, screen, &cfg.layout)
                    .into_iter()
                    .map(|(id, area)| (id, area.height, area.width))
                    .collect();
            let app = ws.active_mut().expect("active project checked above");
            app.terminal.resize_visible_panes(&layouts);
            app.terminal.sync_scroll();
        }

        // Collected before the mutable borrow of the active project, since the
        // tab row names every project while the body renders only one. Bounded
        // by `MAX_PROJECTS`, so the per-frame clone is a handful of short
        // strings.
        let tab_paths: Vec<String> = ws.projects().iter().map(|p| p.repo_path.clone()).collect();
        let active_tab = ws.active_index();
        let empty_notice = ws.empty_notice().cloned();
        let prefix_armed = ws.prefix_armed();
        let fallback_accent = crate::config::Accent::from_index(cfg.theme.preset_index()).color();

        let (app_opt, repo_input) = ws.render_parts();
        let tabs = crate::ui::Chrome {
            repo_paths: &tab_paths,
            active: active_tab,
            repo_input,
        };
        let accent = app_opt
            .as_ref()
            .map(|app| app.current_accent())
            .unwrap_or(fallback_accent);
        terminal.draw(|frame| match app_opt {
            Some(app) => {
                crate::ui::draw(frame, app, tabs, ss, ts, &cfg.layout, accent);
            }
            None => crate::ui::draw_empty(
                frame,
                tabs,
                empty_notice.as_ref(),
                ctx.leader,
                prefix_armed,
                cfg.mouse.enabled,
                accent,
            ),
        })?;

        // `tabs` above borrows the workspace for the draw; input needs it
        // mutably, so rebuild the same view over a snapshot of the dialog.
        // Only the buffer is copied, and only on frames that see an event.
        let repo_input = ws.repo_input.clone();
        let tabs = crate::ui::Chrome {
            repo_paths: &tab_paths,
            active: active_tab,
            repo_input: &repo_input,
        };

        // 16 ms ≈ 60 fps. The previous 50 ms tick noticeably lagged PTY echo
        // on every keystroke (typing felt sticky). `event::poll` performs an
        // OS-level wait when nothing is happening, so the higher cap doesn't
        // burn CPU at idle.
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                // Ratatui's next draw will pick up the new size from
                // `Frame::area()`. An explicit clear() here only adds a
                // visible flash on resize without improving correctness.
                Event::Resize(_, _) => {}
                Event::Key(key) => {
                    let outcome = dispatch_key(ws, key);
                    if apply_outcome(terminal, ws, &mut link, outcome)? {
                        return Ok(());
                    }
                }
                Event::Paste(text) => dispatch_paste(ws, &text),
                Event::Mouse(mouse) => {
                    let screen = Rect::new(0, 0, size.width, size.height);
                    let outcome =
                        dispatch_mouse(ws, tabs, mouse, screen, &cfg.layout, cfg.mouse.enabled);
                    if apply_outcome(terminal, ws, &mut link, outcome)? {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }
}

/// Carry out a handler's outcome. Returns `true` when the app should quit.
pub(crate) fn apply_outcome(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ws: &mut Workspace,
    link: &mut SessionLink,
    outcome: KeyOutcome,
) -> anyhow::Result<bool> {
    match outcome {
        KeyOutcome::Quit => return Ok(true),
        KeyOutcome::Redraw => terminal.clear()?,
        KeyOutcome::Continue => {}
        // Through the link: attached, opening and closing a tab is a request to
        // whoever owns the tab list, not a local edit.
        KeyOutcome::Project(request) => link.request(ws, request),
    }
    Ok(false)
}
