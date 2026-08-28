use crate::app::{App, Focus};
use crate::workspace::Workspace;

/// Route pasted text: into the open repo dialog if it owns input, else to the
/// active project. Nothing happens with no project and no dialog — there is no
/// sink for it.
pub(crate) fn dispatch_paste(ws: &mut Workspace, text: &str) {
    if ws.repo_input.active {
        for ch in text.chars().filter(|c| !c.is_control()) {
            ws.repo_input_push(ch);
        }
        return;
    }
    match ws.active_mut() {
        Some(app) => handle_paste(app, text),
        // No sink for the text, but an armed prefix must still resolve — a
        // non-command event cancels it, as it does on the project screen.
        None => ws.cancel_prefix(),
    }
}

/// Route a bracketed-paste payload within one project.
///
/// Search overlays accept the text after stripping control characters, the
/// same rule the typed-key handlers enforce. The terminal pane receives the
/// paste re-wrapped in `ESC [200~ ... ESC [201~` so the inner shell can
/// distinguish multi-line paste from interactive input. `text` never carries
/// the outer markers: crossterm strips them on Unix, and on Windows there are
/// none — `input::burst` synthesises the event from keys.
pub(crate) fn handle_paste(app: &mut App, text: &str) {
    // A paste while the prefix is armed would leave the PREFIX indicator
    // stuck and make the next key resolve as a follow-up; resolve the prefix
    // first (tmux treats a non-command event as a cancel).
    app.interaction.prefix_armed = false;
    if app.focus == Focus::FileList && app.status_view.search_active {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.search_push(ch);
        }
        return;
    }
    if app.focus == Focus::FileList && app.tree_view.search_active {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.tree_search_push(ch);
        }
        return;
    }
    if app.focus == Focus::FileList
        && (app.log_view.commit_search_active || app.log_view.file_search_active)
    {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.log_search_push(ch);
        }
        return;
    }
    if app.focus == Focus::DiffViewer && app.diff.search.active {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.diff.search_push(ch);
        }
        return;
    }
    if app.focus == Focus::Terminal {
        // Strip ESC and NUL before forwarding: an embedded 0x1b can re-arm or
        // cancel the bracketed-paste boundary the shell is parsing, and NUL
        // is malformed for most line-buffered shells. Newlines, tabs, and
        // other printable controls stay — they are what bracketed paste
        // delivers atomically.
        let sanitized: Vec<u8> = text
            .as_bytes()
            .iter()
            .copied()
            .filter(|&b| b != 0x1b && b != 0x00)
            .collect();
        // Only wrap in bracketed-paste markers when the running program asked
        // for them (DECSET 2004). A raw program that never enabled the mode
        // would otherwise receive the literal `[200~`/`[201~` markers as input.
        let bracketed = app
            .active_screen()
            .map(|screen| screen.bracketed_paste())
            .unwrap_or(false);
        if bracketed {
            let mut bytes = Vec::with_capacity(sanitized.len() + 12);
            bytes.extend_from_slice(b"\x1b[200~");
            bytes.extend_from_slice(&sanitized);
            bytes.extend_from_slice(b"\x1b[201~");
            app.terminal.send_input(&bytes);
        } else {
            app.terminal.send_input(&sanitized);
        }
    }
}
