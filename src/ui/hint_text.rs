use crate::app::{App, DiffPaneView, Focus, ViewMode};
use crate::runtime::terminal::TerminalFullscreen;

pub(crate) const PREFIX_CHIP: &str = " PREFIX ";
pub(crate) const EMPTY_HINT: &str = " <prefix> o: open project | <prefix> q: detach";
pub(crate) const EMPTY_HINT_ARMED: &str = " o: open project | q: detach | esc: cancel";

pub(crate) fn prefix_armed_hint_text(app: &App) -> String {
    // While the terminal fills the body the digit row addresses panes
    // directly (`1-8`); in the split view `1`/`2` focus the list/diff and
    // `3-9,0` jump to panes.
    let digits = if app.terminal.fullscreen.fills_body() {
        "1-8: pane"
    } else {
        "1-9: focus/pane"
    };
    // `w`/`s` only act under their availability predicates, so only advertise
    // them there — a hint for a no-op key would lie.
    let close = if app.can_close_pane() {
        "w: close pane | "
    } else {
        ""
    };
    let swap = if app.can_swap_panes() {
        "s: swap pane | "
    } else {
        ""
    };
    // Only while another client is sizing the panes; with the sizing already
    // here the key does nothing.
    let resize = if app.can_claim_pane_sizing() {
        "z: resize panes here | "
    } else {
        ""
    };
    // Only while a plugin actually has a recovery pending, which is rare — an
    // always-present hint for it would spend a scarce row on a key that is
    // usually inert.
    let cancel = if app.can_cancel_recovery() {
        "c: cancel recovery | "
    } else {
        ""
    };
    // The view toggles name their destination from the current mode.
    let (log_toggle, tree_toggle) = match app.mode {
        ViewMode::Log => ("l: status view", "b: tree view"),
        ViewMode::Status => ("l: log view", "b: tree view"),
        ViewMode::Tree => ("l: log view", "b: status view"),
    };
    // `x` is advertised unconditionally: refusing to close the last project
    // reports why on the notice row, so the key always produces a visible
    // result.
    format!(
        " t: new pane | {close}{swap}{resize}{cancel}{log_toggle} | {tree_toggle} | f: fullscreen | o: open project | x: close project | p: theme | u: reload config | r: redraw | q: detach | {digits} | esc: cancel"
    )
}

/// The hint literal (with `<prefix>` placeholders) for the current
/// non-modal state. Single source for `render_hint_bar` and `hint_click_at`,
/// so the click hit-test always segments exactly the text on screen.
pub(crate) fn normal_hint_literal(app: &App) -> &'static str {
    match app.terminal.fullscreen {
        // From Grid the next `f` zooms the active pane — but only when Zoom
        // would look different from Grid; otherwise the cycle skips Zoom and
        // `f` exits.
        TerminalFullscreen::Grid if app.terminal.zoom_distinct_from_grid() => {
            return " <prefix>: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle pane | <prefix> f: zoom active pane | <prefix> t: new pane | <prefix> w: close pane | <prefix> q: detach";
        }
        TerminalFullscreen::Grid => {
            return " <prefix>: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | <prefix> f: exit fullscreen | <prefix> t: new pane | <prefix> w: close pane | <prefix> q: detach";
        }
        TerminalFullscreen::Zoom => {
            return " <prefix>: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle pane | <prefix> f: exit fullscreen | <prefix> t: new pane | <prefix> w: close pane | <prefix> q: detach";
        }
        TerminalFullscreen::Off => {}
    }
    if app.diff.fullscreen {
        let hint = if app.diff.view == DiffPaneView::File {
            // Tree mode's right pane is permanently the file view — `v`
            // can't leave it, so don't advertise a no-op.
            if app.mode == ViewMode::Tree {
                " <prefix> f: exit zoom | j/k: scroll | pgup/pgdn: page | w: wrap | <prefix> q: detach"
            } else {
                " <prefix> f: exit zoom | v: back to diff | j/k: scroll | pgup/pgdn: page | w: wrap | <prefix> q: detach"
            }
        } else if app.diff.view == DiffPaneView::Split {
            // No `w: wrap` here or in the unzoomed split arm: the split view
            // ignores wrapping (halves folding to different heights would stop
            // lining up), and a hint for a no-op key would lie.
            " <prefix> f: exit zoom | s: unified diff | j/k: scroll | pgup/pgdn: page | <prefix> q: detach"
        } else if app.diff.search.active {
            " type to search | enter: confirm | esc: cancel"
        } else if !app.diff.search.query.is_empty() {
            " <prefix> f: exit zoom | n: next match | shift+n: prev match | /: new search | esc: clear"
        } else if app.can_open_file_view() {
            " <prefix> f: exit zoom | j/k: scroll | tab: view | w: wrap | v: view file | s: split | /: search | pgup/pgdn: page | <prefix> q: detach"
        } else {
            // No file target for `v` — a hint for a no-op key would lie.
            " <prefix> f: exit zoom | j/k: scroll | tab: view | w: wrap | s: split | /: search | pgup/pgdn: page | <prefix> q: detach"
        };
        return hint;
    }
    if app.list_fullscreen {
        let hint = match app.mode {
            ViewMode::Log if app.log_view.drill_down => {
                " <prefix> f: exit zoom | esc: back to commits | j/k: navigate files | <prefix> q: detach"
            }
            ViewMode::Log => {
                " <prefix> f: exit zoom | <prefix> l: status view | <prefix> b: tree view | j/k: navigate commits | enter: view files | <prefix> q: detach"
            }
            ViewMode::Status => {
                " <prefix> f: exit zoom | j/k: navigate | /: search | <prefix> l: log view | <prefix> b: tree view | <prefix> q: detach"
            }
            ViewMode::Tree => {
                " <prefix> f: exit zoom | j/k: navigate | /: search | →/enter: expand | ←: collapse | <prefix> b: status view | <prefix> l: log view | <prefix> q: detach"
            }
        };
        return hint;
    }
    if let Focus::Terminal = app.focus {
        // The `l` toggle names its destination: from Log mode it returns to
        // the status view, from Status/Tree it enters the log view.
        return if app.mode == ViewMode::Log {
            " <prefix>: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle | <prefix> t: new pane | <prefix> w: close pane | <prefix> f: fullscreen | <prefix> l: status view | <prefix> o: open project | <prefix> q: detach"
        } else {
            " <prefix>: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle | <prefix> t: new pane | <prefix> w: close pane | <prefix> f: fullscreen | <prefix> l: log view | <prefix> o: open project | <prefix> q: detach"
        };
    }
    match app.focus {
        Focus::Terminal => unreachable!("Focus::Terminal handled above"),
        Focus::FileList => match app.mode {
            ViewMode::Log => {
                if app.log_view.drill_down {
                    " esc: back to commits | j/k: navigate files | shift+←/→: cycle | <prefix> q: detach"
                } else {
                    " shift+←/→: cycle | j/k: navigate commits | enter: view files | <prefix> t: new pane | <prefix> f: fullscreen | <prefix> l: status view | <prefix> b: tree view | <prefix> o: open project | <prefix> q: detach"
                }
            }
            ViewMode::Status => {
                " shift+←/→: cycle | j/k: navigate | /: search | <prefix> t: new pane | <prefix> f: fullscreen | <prefix> l: log view | <prefix> b: tree view | <prefix> o: open project | <prefix> q: detach"
            }
            ViewMode::Tree => {
                " shift+←/→: cycle | j/k: navigate | /: search | →/enter: expand | ←: collapse | <prefix> b: status view | <prefix> l: log view | <prefix> q: detach"
            }
        },
        Focus::DiffViewer => {
            if app.diff.view == DiffPaneView::File && app.diff.search.active {
                " type to search | enter: confirm | esc: cancel"
            } else if app.diff.view == DiffPaneView::File && !app.diff.search.query.is_empty() {
                " n: next match | shift+n: prev match | /: new search | esc: clear"
            } else if app.diff.view == DiffPaneView::File {
                // Tree mode's right pane is permanently the file view — `v`
                // can't leave it, so don't advertise a no-op.
                if app.mode == ViewMode::Tree {
                    " j/k: scroll | pgup/pgdn: page | w: wrap | /: search | shift+←/→: cycle | <prefix> q: detach"
                } else {
                    " v: back to diff | j/k: scroll | pgup/pgdn: page | w: wrap | /: search | shift+←/→: cycle | <prefix> q: detach"
                }
            } else if app.diff.view == DiffPaneView::Split {
                " s: unified diff | j/k: scroll | pgup/pgdn: page | shift+←/→: cycle | <prefix> f: zoom | <prefix> q: detach"
            } else if app.diff.search.active {
                " type to search | enter: confirm | esc: cancel"
            } else if !app.diff.search.query.is_empty() {
                " n: next match | shift+n: prev match | /: new search | esc: clear"
            } else if app.can_open_file_view() {
                // The `l` toggle names its destination (Tree mode never reaches
                // these arms — its right pane is always the file view).
                if app.mode == ViewMode::Log {
                    " shift+←/→: cycle | j/k: scroll | pgup/pgdn: scroll | tab: view | w: wrap | v: view file | s: split | /: search | <prefix> t: new pane | <prefix> f: zoom | <prefix> l: status view | <prefix> b: tree view | <prefix> o: open project | <prefix> q: detach"
                } else {
                    " shift+←/→: cycle | j/k: scroll | pgup/pgdn: scroll | tab: view | w: wrap | v: view file | s: split | /: search | <prefix> t: new pane | <prefix> f: zoom | <prefix> l: log view | <prefix> b: tree view | <prefix> o: open project | <prefix> q: detach"
                }
            } else {
                // No file target for `v` — a hint for a no-op key would lie.
                if app.mode == ViewMode::Log {
                    " shift+←/→: cycle | j/k: scroll | pgup/pgdn: scroll | tab: view | w: wrap | s: split | /: search | <prefix> t: new pane | <prefix> f: zoom | <prefix> l: status view | <prefix> b: tree view | <prefix> o: open project | <prefix> q: detach"
                } else {
                    " shift+←/→: cycle | j/k: scroll | pgup/pgdn: scroll | tab: view | w: wrap | s: split | /: search | <prefix> t: new pane | <prefix> f: zoom | <prefix> l: log view | <prefix> b: tree view | <prefix> o: open project | <prefix> q: detach"
                }
            }
        }
    }
}
