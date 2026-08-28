pub(crate) use crate::application::input::dispatch::ProjectContext;
use crate::application::input::dispatch::{KeyOutcome, dispatch_key};
use crate::application::input::mouse::dispatch_mouse;
use crate::application::input::paste::dispatch_paste;
use crate::application::redraw::{RedrawCause, RedrawState};
use crate::application::session_link::SessionLink;
use crate::application::terminal_guard::TuiTerminal;
use crate::workspace::Workspace;
use crossterm::event::{self, Event};
use ratatui::layout::Rect;
use std::time::Duration;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

pub(crate) fn main_loop(
    terminal: &mut TuiTerminal,
    ws: &mut Workspace,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    cfg: &crate::config::Config,
    ctx: &ProjectContext,
    mut link: SessionLink,
) -> anyhow::Result<()> {
    let blink_started = std::time::Instant::now();
    let mut redraw = RedrawState::new();
    loop {
        // Whoever owns the tab list gets the first word each tick: attached,
        // the set may have changed under this client since the last frame.
        if link.sync(ws, ctx) {
            redraw.request(RedrawCause::Session);
        }
        if !link.is_connected() {
            tracing::info!("daemon connection lost");
            // Reported rather than returned quietly. Leaving on a lost
            // connection looks identical to leaving on purpose — the terminal
            // comes back with no explanation. Deliberately not "the session
            // is gone": the daemon may well be running and have dropped only
            // this connection.
            anyhow::bail!(
                "the connection to the session ended. The session may still be running — \
                 reattach with `nightcrow attach`"
            );
        }
        // Every project drains its queues, not just the visible one: the
        // snapshot worker and PTY reader keep producing regardless of which
        // tab is on screen. Only the active project
        // *applies* its snapshot, though — a background one waits in
        // `pending_snapshot` until its tab is shown.
        let active = ws.active_index();
        for (i, project) in ws.projects_mut().iter_mut().enumerate() {
            if project.poll_git_loads() {
                redraw.request(RedrawCause::Git);
            }
            if i == active {
                if project.poll_snapshot() {
                    redraw.request(RedrawCause::Snapshot);
                }
                // Stays with the snapshot as active-only work: applying a
                // commit-log page can trigger a further prefetch and load a
                // commit diff on the git-load worker.
                if project.poll_commit_log_page_fetch() {
                    redraw.request(RedrawCause::Log);
                }
            } else {
                project.drain_snapshot();
            }
            // Cheap drains that must run everywhere: the tree watcher so OS
            // filesystem events do not pile up, the terminal so PTY output is
            // consumed before the pipe fills and blocks the child. Acting on a
            // watcher event is active-only; a hidden project records the event.
            if i == active {
                if project.poll_tree_watcher() {
                    redraw.request(RedrawCause::Tree);
                }
            } else {
                project.drain_tree_watcher();
            }
            if project.poll_terminal() {
                redraw.request(RedrawCause::Terminal);
            }
        }
        // Project-tab attention is client-local and means "not seen on this
        // screen". The project in front has just consumed its terminal events,
        // so everything through this tick is visible and acknowledged.
        ws.acknowledge_active_attention();

        let size = terminal.size()?;
        let screen = Rect::new(0, 0, size.width, size.height);
        redraw.observe_screen(size.width, size.height);
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
        // by `MAX_PROJECTS`, so the per-frame clone is a handful of strings.
        let tab_paths: Vec<String> = ws
            .projects()
            .iter()
            .map(|p| p.repository_path().to_string())
            .collect();
        let tab_attention: Vec<bool> = ws
            .projects()
            .iter()
            .map(|project| project.terminal.has_unread_attention())
            .collect();
        let has_attention = tab_attention.iter().any(|attention| *attention);
        let attention_bright = crate::ui::project_tab::blink_is_bright(blink_started.elapsed());
        redraw.observe_attention(has_attention, attention_bright);
        let caret_active = ws
            .active()
            .is_some_and(crate::app::App::search_overlay_active);
        redraw.observe_caret(caret_active, crate::ui::current_caret_lit());
        let active_tab = ws.active_index();
        let empty_notice = ws.empty_notice().cloned();
        let prefix_armed = ws.prefix_armed();
        // One colour for the session, so it is read off the workspace and the
        // empty screen is painted in it too — read before `render_parts` takes
        // the borrow the projects need.
        let accent = ws.current_accent();

        if redraw.take() {
            let (app_opt, repo_input) = ws.render_parts();
            let tabs = crate::ui::Chrome {
                repo_paths: &tab_paths,
                attention: &tab_attention,
                attention_bright,
                active: active_tab,
                repo_input,
            };
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
        }

        // `tabs` above borrows the workspace for the draw; input needs it
        // mutably, so rebuild the same view over a snapshot of the dialog.
        // Only the buffer is copied here; the frame itself may be skipped when
        // no state or visual clock phase changed.
        let repo_input = ws.repo_input.clone();
        let tabs = crate::ui::Chrome {
            repo_paths: &tab_paths,
            attention: &tab_attention,
            attention_bright,
            active: active_tab,
            repo_input: &repo_input,
        };

        // 16 ms ≈ 60 fps is only the polling latency cap. Unlike the old frame
        // clock, an idle tick does not draw; the wait lets asynchronous PTY and
        // watcher results be noticed without keeping a terminal frame alive.
        if event::poll(Duration::from_millis(16))? {
            let first = event::read()?;
            // Unix gets a real `Event::Paste` from crossterm; Windows never
            // does, so a paste burst is drained and rewritten into one here
            // (`input::burst`) for the same arm below to route.
            #[cfg(windows)]
            let events = crate::application::input::burst::coalesce_paste(first)?;
            #[cfg(not(windows))]
            let events = [first];

            for event in events {
                match event {
                    // Ratatui's next draw will pick up the new size from
                    // `Frame::area()`. An explicit clear() here only adds a
                    // visible flash on resize without improving correctness.
                    Event::Resize(_, _) => redraw.request(RedrawCause::Resize),
                    Event::Key(key) => {
                        let pressed = key.kind == crossterm::event::KeyEventKind::Press;
                        if pressed {
                            redraw.request(RedrawCause::Input);
                        }
                        let outcome = dispatch_key(ws, key);
                        let force_redraw = matches!(outcome, KeyOutcome::Redraw);
                        if apply_outcome(terminal, ws, &mut link, outcome)? {
                            return Ok(());
                        }
                        if force_redraw {
                            redraw.request(RedrawCause::Redraw);
                        }
                    }
                    Event::Paste(text) => {
                        redraw.request(RedrawCause::Input);
                        dispatch_paste(ws, &text);
                    }
                    Event::Mouse(mouse) => {
                        redraw.request(RedrawCause::Input);
                        let screen = Rect::new(0, 0, size.width, size.height);
                        let outcome =
                            dispatch_mouse(ws, tabs, mouse, screen, &cfg.layout, cfg.mouse.enabled);
                        let force_redraw = matches!(outcome, KeyOutcome::Redraw);
                        if apply_outcome(terminal, ws, &mut link, outcome)? {
                            return Ok(());
                        }
                        if force_redraw {
                            redraw.request(RedrawCause::Redraw);
                        }
                    }
                    _ => redraw.request(RedrawCause::Input),
                }
            }
        }
    }
}

/// Carry out a handler's outcome. Returns `true` when the app should quit.
pub(crate) fn apply_outcome(
    terminal: &mut TuiTerminal,
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
