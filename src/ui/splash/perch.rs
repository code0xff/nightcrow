use super::crow;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// How long one wing position is held.
pub const FLAP_FRAME: Duration = Duration::from_millis(110);

/// The crow plus the branch it perches on.
pub(super) const PERCH_HEIGHT: u16 = crow::HEIGHT as u16 + 1;

/// Below this the crow is cropped past recognition, so it is left out entirely.
const MIN_PERCH_HEIGHT: u16 = 10;

/// Rows between the crow and whatever a caller draws under it.
const GAP: u16 = 1;

/// Which animation frame `elapsed` since the flap's origin falls in.
fn flap_frame(elapsed: Duration) -> usize {
    (elapsed.as_millis() / FLAP_FRAME.as_millis()) as usize
}

/// One origin for every flap, so crows on screen together beat in step. Callers
/// that keep their own frame counter (the startup splash) don't need this.
fn flap_phase() -> Duration {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed()
}

/// Draw the flapping crow on its branch, filling `area` from the top.
///
/// A short `area` crops the tail and the branch takes its place, which reads as
/// the bird standing behind the branch rather than as a cut-off drawing. Too
/// short for even the head and shoulders ([`MIN_PERCH_HEIGHT`]) and nothing is
/// drawn — the caller's `area` is left blank.
pub(super) fn draw_perch(frame: &mut Frame, area: Rect, accent: Color, tick: usize) {
    if area.height < MIN_PERCH_HEIGHT || area.width < crow::WIDTH as u16 {
        return;
    }

    let body_rows = (area.height - 1).min(crow::HEIGHT as u16) as usize;
    let mut lines: Vec<Line> = crow::frame(tick)
        .into_iter()
        .take(body_rows)
        .map(|row| Line::from(Span::styled(row, Style::default().fg(accent))))
        .collect();
    lines.push(Line::from(Span::styled(
        "─".repeat(crow::WIDTH),
        Style::default().fg(accent).add_modifier(Modifier::DIM),
    )));

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

/// Fill an empty terminal pane: the flapping crow above `hint`.
///
/// The flap runs off the shared clock, so this needs no frame counter from the
/// caller — it animates as long as the event loop keeps redrawing.
pub fn draw_idle(frame: &mut Frame, area: Rect, accent: Color, hint: Line<'_>) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let perch = perch_height(area);
    let block_h = if perch == 0 { 1 } else { perch + GAP + 1 };
    let top = area.y + (area.height.saturating_sub(block_h)) / 2;

    if perch > 0 {
        draw_perch(
            frame,
            Rect::new(area.x, top, area.width, perch),
            accent,
            flap_frame(flap_phase()),
        );
    }

    frame.render_widget(
        Paragraph::new(hint).alignment(Alignment::Center),
        Rect::new(area.x, top + block_h - 1, area.width, 1),
    );
}

/// Rows to give the perch in `area`, leaving room for the hint; 0 when the pane
/// cannot hold a crow at all.
fn perch_height(area: Rect) -> u16 {
    let spare = area.height.saturating_sub(GAP + 1);
    if area.width < crow::WIDTH as u16 || spare < MIN_PERCH_HEIGHT {
        return 0;
    }
    spare.min(PERCH_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::{FLAP_FRAME, MIN_PERCH_HEIGHT, PERCH_HEIGHT, flap_frame, perch_height};
    use crate::ui::splash::crow;
    use ratatui::layout::Rect;
    use std::time::Duration;

    #[test]
    fn the_flap_advances_one_frame_per_interval() {
        assert_eq!(flap_frame(Duration::ZERO), 0);
        assert_eq!(flap_frame(FLAP_FRAME - Duration::from_millis(1)), 0);
        assert_eq!(flap_frame(FLAP_FRAME), 1);
        assert_eq!(flap_frame(FLAP_FRAME * 7), 7);
    }

    #[test]
    fn a_roomy_pane_gets_the_whole_crow() {
        let area = Rect::new(0, 0, 80, 40);
        assert_eq!(perch_height(area), PERCH_HEIGHT);
    }

    #[test]
    fn a_short_pane_crops_the_crow_instead_of_overflowing() {
        let height = MIN_PERCH_HEIGHT + 3;
        let area = Rect::new(0, 0, 80, height);
        let perch = perch_height(area);
        assert!(perch < PERCH_HEIGHT, "expected a cropped crow, got {perch}");
        assert!(
            perch + 2 <= height,
            "the crow and hint must fit in {height}"
        );
    }

    #[test]
    fn a_pane_too_small_for_a_recognisable_crow_gets_none() {
        assert_eq!(perch_height(Rect::new(0, 0, 80, MIN_PERCH_HEIGHT)), 0);
        assert_eq!(perch_height(Rect::new(0, 0, 80, 1)), 0);
        assert_eq!(perch_height(Rect::new(0, 0, 0, 0)), 0);
        assert_eq!(perch_height(Rect::new(0, 0, crow::WIDTH as u16 - 1, 40)), 0);
    }
}
