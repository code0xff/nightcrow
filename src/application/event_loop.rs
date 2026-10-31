use crate::application::input::dispatch::KeyOutcome;
pub(crate) use crate::application::input::dispatch::ProjectContext;
use crate::application::input::events::dispatch_input_event;
use crate::application::input::ready::{InputControl, process_ready_input};
use crate::application::redraw::{RedrawCause, RedrawState};
use crate::application::session_link::SessionLink;
use crate::application::size_activity::claim_for_user_activity;
use crate::application::terminal_guard::TuiTerminal;
use crate::workspace::Workspace;
use crossterm::event::{self, Event};
use ratatui::layout::Rect;
use std::time::{Duration, Instant, SystemTime};
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
        // Pane lifecycle and resize confirmations must precede the next input.
        poll_terminals_before_input(ws, &mut redraw);

        let progress = process_ready_input(read_ready_event, expand_input_event, |event| {
            claim_for_user_activity(ws, &event);
            let outcome =
                dispatch_input_event(terminal, ws, cfg, event, &mut redraw, blink_started)?;
            if let Some(outcome) = outcome {
                let force_redraw = matches!(outcome, KeyOutcome::Redraw);
                if apply_outcome(terminal, ws, &mut link, outcome)? {
                    return Ok(InputControl::Quit);
                }
                if force_redraw {
                    redraw.request(RedrawCause::Redraw);
                }
            }
            Ok(InputControl::Continue)
        })?;
        if progress.quit {
            return Ok(());
        }
        let active = ws.active_index();
        // Every project drains its queues, not just the visible one: the
        // snapshot worker keeps producing regardless of which tab is on
        // screen. Terminal events were already drained before input; only the
        // active project applies its snapshot, while hidden ones wait in
        // `pending_snapshot` until shown.
        for (i, project) in ws.projects_mut().iter_mut().enumerate() {
            if project.poll_git_loads() && i == active {
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
            // Cheap drains that must run everywhere: the tree watcher keeps OS
            // filesystem events from piling up. Acting on a watcher event is
            // active-only; a hidden project records the event.
            if i == active {
                if project.poll_tree_watcher() {
                    redraw.request(RedrawCause::Tree);
                }
            } else {
                project.drain_tree_watcher();
            }
        }

        let size = terminal.size()?;
        let screen = Rect::new(0, 0, size.width, size.height);
        observe_terminal_size(&mut redraw, size.width, size.height);
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
        let attention_bright = crate::ui::project_tab::blink_is_bright(blink_started.elapsed());
        redraw.observe_attention(&tab_attention, attention_bright);
        let now = SystemTime::now();
        let hot_deadline = ws
            .active()
            .and_then(|app| crate::ui::next_hot_deadline_for_app(app, now));
        redraw.observe_hot_deadline(hot_deadline, now);
        let caret_active = ws
            .active()
            .is_some_and(crate::app::App::search_overlay_active);
        redraw.observe_caret(caret_active, crate::ui::current_caret_lit());
        if ws.advance_repo_input_focus_flash(Instant::now()) {
            redraw.request(RedrawCause::RepoInputFocus);
        }
        let active_tab = ws.active_index();
        let empty_notice = ws.empty_notice().cloned();
        let prefix_armed = ws.prefix_armed();
        // One colour for the session, so it is read off the workspace and the
        // empty screen is painted in it too — read before `render_parts` takes
        // the borrow the projects need.
        let accent = ws.current_accent();

        if redraw.take() {
            let (app_opt, repo_input, help) = ws.render_parts();
            let tabs = crate::ui::Chrome {
                repo_paths: &tab_paths,
                attention: &tab_attention,
                attention_bright,
                active: active_tab,
                repo_input,
                help,
                strip: cfg.layout.tabs,
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

        // Ready input was handled before rendering; this 16 ms wait is only
        // the idle latency cap between source events.
        event::poll(Duration::from_millis(16))?;
    }
}

pub(crate) fn poll_terminals_before_input(ws: &mut Workspace, redraw: &mut RedrawState) {
    let active = ws.active_index();
    for (i, project) in ws.projects_mut().iter_mut().enumerate() {
        if project.poll_terminal() && i == active {
            redraw.request(RedrawCause::Terminal);
        }
    }
    ws.acknowledge_active_attention();
}

fn read_ready_event() -> anyhow::Result<Option<Event>> {
    if event::poll(Duration::ZERO)? {
        return Ok(Some(event::read()?));
    }
    Ok(None)
}

fn expand_input_event(first: Event) -> anyhow::Result<Vec<Event>> {
    #[cfg(windows)]
    {
        Ok(crate::application::input::burst::coalesce_paste(first)?)
    }
    #[cfg(not(windows))]
    {
        Ok(vec![first])
    }
}

/// Track physical screen changes for redraw. Geometry changes do not claim
/// shared PTY sizing; only direct key, paste, click, or wheel input does.
pub(crate) fn observe_terminal_size(redraw: &mut RedrawState, width: u16, height: u16) {
    redraw.observe_screen(width, height);
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
        KeyOutcome::OpenLink { target, base } => {
            if let Err(error) = crate::platform::links::open(&target, std::path::Path::new(&base)) {
                ws.raise_notice(
                    crate::app::NoticeKind::Terminal,
                    format!("could not open terminal link {target:?}: {error}"),
                );
            }
        }
        // Through the link: attached, opening and closing a tab is a request to
        // whoever owns the tab list, not a local edit.
        KeyOutcome::Project(request) => link.request(ws, request),
    }
    Ok(false)
}
