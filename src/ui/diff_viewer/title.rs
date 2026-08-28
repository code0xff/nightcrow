use crate::app::{App, ViewMode};
use crate::ui::jump_legend;

/// What the diff pane is showing right now: the commit's diff title in log
/// mode, the selected path in status/tree mode, with per-mode fallbacks for
/// "nothing selected".
fn diff_label(app: &App) -> String {
    match app.mode() {
        ViewMode::Log => {
            if app.log_view().diff_title.is_empty() {
                "Diff".to_string()
            } else {
                app.log_view().diff_title.clone()
            }
        }
        ViewMode::Status => app
            .selected_filtered_status_file()
            .map(|f| f.path.clone())
            .unwrap_or_else(|| "Diff".to_string()),
        ViewMode::Tree => app
            .tree_view()
            .selected_path()
            .unwrap_or_else(|| "File".to_string()),
    }
}

/// Title for the unified diff pane: jump legend, label, and the search match
/// counter while a query is active.
pub(crate) fn unified_title(app: &App) -> String {
    let jump = jump_legend(app, '2');
    let label = diff_label(app);
    if !app.diff_pane().search.has_query() {
        return format!(" {jump} {label} ");
    }
    let count = app.diff_pane().search.matches.len();
    if count == 0 {
        format!(" {jump} {label} [no matches] ")
    } else {
        format!(
            " {jump} {label} [{}/{}] ",
            app.diff_pane().search.cursor + 1,
            count
        )
    }
}

/// Title for the split pane: the same label, tagged `[split]`. Search match
/// counts are omitted because the split view does not render search
/// highlights.
pub(crate) fn split_title(app: &App) -> String {
    format!(" {} {} [split] ", jump_legend(app, '2'), diff_label(app))
}
