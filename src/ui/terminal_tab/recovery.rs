//! The recovery marker a pane's tab label carries: a suffix on an existing
//! label rather than a row or an overlay, because adding or removing a layout
//! row resizes every open PTY (see `docs/architecture.md`). The full report
//! lives on the notice row; this is only the "which pane" pointer.

use crate::app::App;
use crate::runtime::terminal::PaneRecovery;
use crate::ui::terminal_tab::layout::{TAB_TITLE_MAX_CHARS, truncate_tab_title};
use crate::ui::wall_clock::local_hour_minute;

/// Chars of the pane title kept when a marker rides along — well under
/// [`TAB_TITLE_MAX_CHARS`](super::layout::TAB_TITLE_MAX_CHARS) so the title is
/// truncated before the marker is appended. Losing the marker would defeat
/// the point of having it.
pub(crate) const RECOVERY_TITLE_MAX_CHARS: usize = 8;

/// A wait with a known end.
const WAITING_GLYPH: char = '⏳';
/// Attempts already spent, which is what a person judges "is this going
/// anywhere" by.
const ATTENTION_GLYPH: char = '⚠';

/// The label for the pane at `index`: its title, plus a recovery marker when its
/// plugin has reported one. Single source for the tab bar and the pane's own
/// border title, so the two cannot disagree about which pane is waiting.
pub(crate) fn pane_label(app: &App, index: usize) -> String {
    let Some(pane) = app.terminal.panes.get(index) else {
        return String::new();
    };
    match app.terminal.recovery_for(pane.id).map(recovery_marker) {
        Some(marker) => format!(
            "{} {marker}",
            truncate_tab_title(&pane.title, RECOVERY_TITLE_MAX_CHARS)
        ),
        None => truncate_tab_title(&pane.title, TAB_TITLE_MAX_CHARS),
    }
}

/// The marker for one pane's report: the deadline as a local wall-clock time
/// when there is one, and the attempt count when any have been spent. A
/// report with neither is still marked with the bare hourglass — a pane its
/// plugin is doing something about must be distinguishable from one it is not.
pub(crate) fn recovery_marker(report: &PaneRecovery) -> String {
    let mut marker = String::new();
    if let Some(at) = report.deadline_epoch.and_then(local_hour_minute) {
        marker.push(WAITING_GLYPH);
        marker.push_str(&at);
    }
    if report.attempt > 0 {
        marker.push(ATTENTION_GLYPH);
        marker.push_str(&report.attempt.to_string());
    }
    if marker.is_empty() {
        marker.push(WAITING_GLYPH);
    }
    marker
}

#[cfg(test)]
mod tests {
    use super::{ATTENTION_GLYPH, WAITING_GLYPH, recovery_marker};
    use crate::runtime::terminal::PaneRecovery;

    fn report(deadline_epoch: Option<i64>, attempt: u32) -> PaneRecovery {
        PaneRecovery {
            state: "waiting_for_reset".to_string(),
            detail: None,
            deadline_epoch,
            attempt,
        }
    }

    #[test]
    fn a_report_with_a_deadline_marks_it_as_a_wall_clock_time() {
        let marker = recovery_marker(&report(Some(1_700_000_000), 0));
        assert!(marker.starts_with(WAITING_GLYPH), "{marker}");
        assert_eq!(marker.chars().count(), 6, "{marker}");
    }

    #[test]
    fn a_report_with_no_deadline_shows_no_time_at_all() {
        let marker = recovery_marker(&report(None, 3));
        assert_eq!(marker, format!("{ATTENTION_GLYPH}3"));
        assert!(!marker.contains(':'), "no deadline must mean no clock time");
    }

    #[test]
    fn a_report_with_both_carries_the_deadline_and_the_attempt_count() {
        let marker = recovery_marker(&report(Some(1_700_000_000), 2));
        assert!(marker.contains(':'), "{marker}");
        assert!(marker.ends_with(&format!("{ATTENTION_GLYPH}2")), "{marker}");
    }

    #[test]
    fn a_report_with_neither_is_still_marked() {
        assert_eq!(recovery_marker(&report(None, 0)), WAITING_GLYPH.to_string());
    }

    #[test]
    fn a_deadline_no_clock_can_place_falls_back_to_the_bare_marker() {
        assert_eq!(
            recovery_marker(&report(Some(i64::MIN), 0)),
            WAITING_GLYPH.to_string(),
            "an unplaceable deadline must not print a wrong time"
        );
    }
}
