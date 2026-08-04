use super::scene::{self, Cell, Ink};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// How long one twinkle frame is held. Slow on purpose: the sky is background,
/// not an animation to watch.
pub const TWINKLE_FRAME: Duration = Duration::from_millis(400);

pub(super) const SCENE_HEIGHT: u16 = scene::HEIGHT as u16;

/// Below this the bird and its bough no longer fit, so the scene is left out.
const MIN_SCENE_HEIGHT: u16 = scene::SUBJECT_HEIGHT as u16;

/// Rows between the scene and whatever a caller draws under it.
const GAP: u16 = 1;

/// Which frame `elapsed` since the sky's origin falls in.
fn twinkle_frame(elapsed: Duration) -> usize {
    (elapsed.as_millis() / TWINKLE_FRAME.as_millis()) as usize
}

/// One origin for every sky, so two of them on screen twinkle together. Callers
/// with their own frame counter (the startup splash) don't need it.
fn sky_phase() -> Duration {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed()
}

/// Draw the night scene into `area`, aligned to its bottom.
///
/// A short `area` drops sky rows from the top — the bird and its bough are the
/// last thing to go, and below [`MIN_SCENE_HEIGHT`] nothing is drawn at all.
pub(super) fn draw_scene(frame: &mut Frame, area: Rect, accent: Color, tick: usize) {
    if area.height < MIN_SCENE_HEIGHT || area.width < scene::WIDTH as u16 {
        return;
    }

    let rows = scene::frame(tick);
    let dropped = rows.len().saturating_sub(area.height as usize);
    let lines: Vec<Line> = rows
        .iter()
        .skip(dropped)
        .map(|row| paint_row(row, accent))
        .collect();

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

/// One row as a `Line`, with a span per run of same-coloured cells.
fn paint_row(row: &[Cell], accent: Color) -> Line<'static> {
    let mut spans: Vec<Span> = Vec::new();
    let mut text = String::new();
    let mut current = row.first().map_or(Ink::Sky, |&(_, ink)| ink);

    for &(ch, ink) in row {
        if ink != current {
            spans.push(Span::styled(
                std::mem::take(&mut text),
                style(current, accent),
            ));
            current = ink;
        }
        text.push(ch);
    }
    spans.push(Span::styled(text, style(current, accent)));
    Line::from(spans)
}

/// The palette. 256-colour indices rather than the sixteen named colours: a crow
/// needs several near-blacks that read apart on a black background, and the sky
/// and the bough need hues the named set does not have. Terminals limited to
/// sixteen colours approximate them.
fn style(ink: Ink, accent: Color) -> Style {
    let fg = match ink {
        Ink::Sky => return Style::default(),
        // The eye is the one cell in the accent, so the session's colour is on
        // screen without painting the bird itself an unbirdlike hue.
        Ink::Eye => accent,
        Ink::Star { bright: true } => Color::Indexed(255),
        Ink::Star { bright: false } => Color::Indexed(244),
        Ink::Moon => Color::Indexed(230),
        Ink::BirdShade => Color::Indexed(236),
        Ink::Bird => Color::Indexed(240),
        Ink::BirdWing => Color::Indexed(245),
        Ink::BirdLit => Color::Indexed(252),
        Ink::BoughShade => Color::Indexed(58),
        Ink::Bough => Color::Indexed(94),
        Ink::BoughLit => Color::Indexed(137),
    };
    Style::default().fg(fg)
}

/// Fill an empty terminal pane: the night scene over `hint` and the build id.
///
/// The sky runs off the shared clock, so this needs no frame counter from the
/// caller — it twinkles as long as the event loop keeps redrawing.
pub fn draw_idle(frame: &mut Frame, area: Rect, accent: Color, hint: Line<'_>) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let scene_h = scene_height(area);
    let gap = if scene_h > 0 { GAP } else { 0 };
    // The build id is what tells a stale client apart from a fresh one, so it is
    // dropped only when there is genuinely no room for a second line.
    let build = u16::from(area.height >= scene_h + gap + 2);
    let block_h = scene_h + gap + 1 + build;
    let top = area.y + area.height.saturating_sub(block_h) / 2;

    if scene_h > 0 {
        draw_scene(
            frame,
            Rect::new(area.x, top, area.width, scene_h),
            accent,
            twinkle_frame(sky_phase()),
        );
    }

    let hint_y = top + scene_h + gap;
    frame.render_widget(
        Paragraph::new(hint).alignment(Alignment::Center),
        Rect::new(area.x, hint_y, area.width, 1),
    );
    if build == 1 {
        frame.render_widget(
            Paragraph::new(super::build_line()).alignment(Alignment::Center),
            Rect::new(area.x, hint_y + 1, area.width, 1),
        );
    }
}

/// Rows to give the scene in `area`, leaving room for the footer; 0 when the
/// pane cannot hold the bird at all.
fn scene_height(area: Rect) -> u16 {
    let spare = area.height.saturating_sub(GAP + 2);
    if area.width < scene::WIDTH as u16 || spare < MIN_SCENE_HEIGHT {
        return 0;
    }
    spare.min(SCENE_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::{
        Ink, MIN_SCENE_HEIGHT, SCENE_HEIGHT, TWINKLE_FRAME, paint_row, scene, scene_height,
        twinkle_frame,
    };
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use std::time::Duration;

    #[test]
    fn the_sky_advances_one_frame_per_interval() {
        assert_eq!(twinkle_frame(Duration::ZERO), 0);
        assert_eq!(twinkle_frame(TWINKLE_FRAME - Duration::from_millis(1)), 0);
        assert_eq!(twinkle_frame(TWINKLE_FRAME), 1);
        assert_eq!(twinkle_frame(TWINKLE_FRAME * 7), 7);
    }

    #[test]
    fn a_roomy_pane_gets_the_whole_scene() {
        assert_eq!(scene_height(Rect::new(0, 0, 80, 40)), SCENE_HEIGHT);
    }

    #[test]
    fn a_short_pane_crops_the_sky_instead_of_overflowing() {
        let height = MIN_SCENE_HEIGHT + 4;
        let shown = scene_height(Rect::new(0, 0, 80, height));
        assert!(shown < SCENE_HEIGHT, "expected a cropped sky, got {shown}");
        assert!(
            shown + 3 <= height,
            "the scene and its footer must fit in {height}"
        );
    }

    #[test]
    fn a_pane_too_small_for_the_bird_gets_no_scene() {
        assert_eq!(scene_height(Rect::new(0, 0, 80, MIN_SCENE_HEIGHT)), 0);
        assert_eq!(scene_height(Rect::new(0, 0, 80, 1)), 0);
        assert_eq!(scene_height(Rect::new(0, 0, 0, 0)), 0);
        assert_eq!(
            scene_height(Rect::new(0, 0, scene::WIDTH as u16 - 1, 40)),
            0
        );
    }

    #[test]
    fn a_row_keeps_its_width_and_splits_at_colour_changes() {
        let row = scene::frame(0);
        let moon_row = &row[3];
        let line = paint_row(moon_row, Color::Yellow);
        assert_eq!(line.width(), scene::WIDTH, "the row must stay padded");
        assert!(
            line.spans.len() > 1,
            "a row holding both sky and moon needs more than one span"
        );
        assert!(
            moon_row.iter().any(|&(_, ink)| ink == Ink::Moon),
            "row 3 is expected to hold the moon"
        );
    }
}
