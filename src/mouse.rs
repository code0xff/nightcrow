use crate::app::{App, Focus};
use crate::key_dispatch::{KeyOutcome, ProjectRequest, handle_key};
use crate::runtime::terminal::WHEEL_LINES_PER_NOTCH;
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

/// Route one mouse event. The project tab row is the only target with no
/// project open, so it is resolved before the per-project handler.
pub(crate) fn dispatch_mouse(
    ws: &mut Workspace,
    tabs: crate::ui::Chrome<'_>,
    mouse: MouseEvent,
    screen: Rect,
    layout: &crate::config::LayoutConfig,
    mouse_enabled: bool,
) -> KeyOutcome {
    let ws_leader = ws.leader();
    // A release must reach the pane whose press it pairs with, even when the
    // dialog opened in between: no drag reports are forwarded, so that program
    // cannot track the pointer itself, and a swallowed release leaves
    // `pending_mouse_press` set for a later unrelated release to match.
    let is_release = matches!(mouse.kind, MouseEventKind::Up(_));
    if ws.repo_input.active && !is_release {
        return KeyOutcome::Continue;
    }
    match ws.active_mut() {
        Some(app) => handle_mouse(app, tabs, mouse, screen, layout),
        None => {
            let MouseEventKind::Down(crossterm::event::MouseButton::Left) = mouse.kind else {
                return KeyOutcome::Continue;
            };
            if let Some(idx) = crate::ui::project_tab_at(tabs, screen, mouse.column, mouse.row) {
                return KeyOutcome::Project(ProjectRequest::Switch(idx));
            }
            // The open hint is the empty screen's one action; a click does
            // what its key does.
            let leader_label = crate::app::leader_label_of(ws_leader);
            let armed = ws.prefix_armed();
            match crate::ui::empty_hint_click_at(
                screen,
                &leader_label,
                armed,
                mouse_enabled,
                mouse.column,
                mouse.row,
            ) {
                Some(crate::ui::HintClick::Plain('o'))
                | Some(crate::ui::HintClick::Leader('o')) => {
                    // Disarm like the key path: an armed prefix left standing
                    // would consume the next key as a stale follow-up.
                    ws.cancel_prefix();
                    KeyOutcome::Project(ProjectRequest::OpenDialog)
                }
                _ => KeyOutcome::Continue,
            }
        }
    }
}

/// Route a captured mouse event to the pane under the pointer. Releases pair
/// with the press's pane (not the pointer pane); wheel scrolls the pane under
/// the pointer; a left press outside pane content can focus an upper panel,
/// jump via a tab/`+N` marker, or run a hint-bar shortcut (dispatched as
/// synthesized keypresses so click and key share the path). In swap mode a
/// left click names the swap target. Drag/motion reports are not forwarded —
/// inner-program text selection stays with the outer terminal's Shift+drag.
pub(crate) fn handle_mouse(
    app: &mut App,
    tabs: crate::ui::Chrome<'_>,
    mouse: MouseEvent,
    screen: Rect,
    layout: &crate::config::LayoutConfig,
) -> KeyOutcome {
    // Releases route by the pending press, not the pointer, so they must be
    // handled before the hit test — the pointer may have left the pane. They
    // also bypass the modal guard: the press happened before the modal opened,
    // and swallowing the release would leave the pending slot stale.
    if let MouseEventKind::Up(_) = mouse.kind {
        release_pending_press(app, screen, layout, mouse.column, mouse.row);
        return KeyOutcome::Continue;
    }
    // Modal overlays own all other input while open — same rule as the key
    // handler: a click behind a modal must not move focus or reach a pane.
    if app.search_overlay_active() {
        return KeyOutcome::Continue;
    }
    // Pane-swap mode: a press names the swap target the way a digit does.
    // Without this branch a click would change the active pane while leaving
    // swap mode armed, so a later digit would swap the wrong pane. Wheel
    // events fall through (like a paste): they don't name a pane.
    if app.awaiting_swap_target()
        && let MouseEventKind::Down(button) = mouse.kind
    {
        app.cancel_swap_target();
        if button == crossterm::event::MouseButton::Left {
            let target = crate::ui::pane_at(app, screen, layout, mouse.column, mouse.row)
                .and_then(|(id, _)| app.terminal.panes.iter().position(|p| p.id == id))
                .or_else(|| crate::ui::tab_click_at(app, screen, layout, mouse.column, mouse.row));
            if let Some(idx) = target {
                app.swap_active_pane_with(idx);
            }
        }
        return KeyOutcome::Continue;
    }
    let Some((id, rect)) = crate::ui::pane_at(app, screen, layout, mouse.column, mouse.row) else {
        if let MouseEventKind::Down(button) = mouse.kind {
            // The project tab row sits above the body, so no panel hit test
            // can claim it; a tab click is the pointer equivalent of its F-key.
            if button == crossterm::event::MouseButton::Left
                && let Some(idx) = crate::ui::project_tab_at(tabs, screen, mouse.column, mouse.row)
            {
                app.cancel_prefix();
                return KeyOutcome::Project(ProjectRequest::Switch(idx));
            }
            if let Some(focus) =
                crate::ui::upper_panel_at(app, screen, layout, mouse.column, mouse.row)
            {
                app.cancel_prefix();
                app.focus = focus;
            } else if button == crossterm::event::MouseButton::Left {
                if let Some(idx) =
                    crate::ui::tab_click_at(app, screen, layout, mouse.column, mouse.row)
                {
                    // A tab click is a jump-key press with the pointer: same
                    // prefix resolution and focus/fullscreen handling.
                    app.cancel_prefix();
                    app.switch_pane(idx);
                } else if let Some(click) =
                    crate::ui::hint_click_at(app, tabs, screen, mouse.column, mouse.row)
                {
                    return dispatch_hint_click(app, click);
                }
            }
        }
        return KeyOutcome::Continue;
    };
    // 1-based pane-local cell, as SGR reports expect.
    let col = mouse.column - rect.x + 1;
    let row = mouse.row - rect.y + 1;
    match mouse.kind {
        MouseEventKind::Down(button) => {
            focus_clicked_pane(app, id);
            if app.terminal.click_pane(id, button, true, col, row) {
                app.pending_mouse_press = Some((id, button, col, row));
            }
        }
        MouseEventKind::ScrollUp => {
            app.terminal
                .scroll_pane(id, true, WHEEL_LINES_PER_NOTCH, Some((col, row)));
        }
        MouseEventKind::ScrollDown => {
            app.terminal
                .scroll_pane(id, false, WHEEL_LINES_PER_NOTCH, Some((col, row)));
        }
        // Horizontal wheel has no scrollback fallback; it reaches only a pane
        // that asked for wheel reports.
        MouseEventKind::ScrollLeft => {
            app.terminal.wheel_horizontal_pane(id, true, col, row);
        }
        MouseEventKind::ScrollRight => {
            app.terminal.wheel_horizontal_pane(id, false, col, row);
        }
        _ => {}
    }
    KeyOutcome::Continue
}

/// Run a clicked hint-bar shortcut by synthesizing the keypress(es) its label
/// names, so a click and the real key share every guard and dispatch path in
/// `handle_key` — a hint click can never do something the named key would not.
fn dispatch_hint_click(app: &mut App, click: crate::ui::HintClick) -> KeyOutcome {
    let plain = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
    match click {
        crate::ui::HintClick::Arm => {
            let leader = app.leader;
            handle_key(app, leader)
        }
        crate::ui::HintClick::Leader(c) => {
            let leader = app.leader;
            match handle_key(app, leader) {
                KeyOutcome::Continue => {}
                other => return other,
            }
            handle_key(app, plain(c))
        }
        crate::ui::HintClick::Plain(c) => handle_key(app, plain(c)),
    }
}

/// Deliver a button release to the pane that received the matching press.
/// The release carries the *stored* press button, not crossterm's: legacy
/// encodings don't identify the button on release (some report every `Up` as
/// `Left`), so trusting that would strand a right/middle press without its
/// release. The release cell is clamped into the pressed pane's current rect;
/// if that pane was closed or hidden since, the release is dropped.
fn release_pending_press(
    app: &mut App,
    screen: Rect,
    layout: &crate::config::LayoutConfig,
    x: u16,
    y: u16,
) {
    let Some((id, pressed, _, _)) = app.pending_mouse_press else {
        return;
    };
    app.pending_mouse_press = None;
    let Some(rect) = crate::ui::terminal_content_areas(app, screen, layout)
        .into_iter()
        .find_map(|(pid, rect)| (pid == id).then_some(rect))
    else {
        return;
    };
    // An extreme resize between press and release can shrink the pane to a
    // zero-sized rect, which would invert the clamp bounds (`clamp` panics
    // when min > max).
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let col = x.clamp(rect.x, rect.right() - 1) - rect.x + 1;
    let row = y.clamp(rect.y, rect.bottom() - 1) - rect.y + 1;
    app.terminal.click_pane(id, pressed, false, col, row);
}

/// Make the clicked pane active and move focus to the terminal, exactly what
/// a jump key does. A click is also a non-command event while the prefix is
/// armed, so resolve the prefix first (same rule as `handle_paste`).
fn focus_clicked_pane(app: &mut App, id: crate::backend::PaneId) {
    app.cancel_prefix();
    let Some(idx) = app.terminal.panes.iter().position(|p| p.id == id) else {
        return;
    };
    app.terminal.active = idx;
    app.terminal.sync_visible_window();
    app.focus = Focus::Terminal;
}
