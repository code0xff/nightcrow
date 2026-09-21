use super::*;
use crate::ui::help_entries::HELP_SECTIONS;
use ratatui::{Terminal, backend::TestBackend};

fn rendered(overlay: &HelpOverlay, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    terminal
        .draw(|frame| {
            let area = frame.area();
            render(frame, overlay, "^F", area, Color::Yellow);
        })
        .expect("a frame");
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn open_overlay() -> HelpOverlay {
    let mut overlay = HelpOverlay::default();
    overlay.toggle();
    overlay
}

#[test]
fn the_overlay_prints_the_configured_leader_not_the_placeholder() {
    let text = rendered(&open_overlay(), 100, 40);

    assert!(text.contains("^F t"), "got: {text}");
    assert!(!text.contains("{L}"), "the placeholder leaked: {text}");
}

#[test]
fn a_screen_too_short_for_every_row_says_where_the_view_is() {
    let text = rendered(&open_overlay(), 100, 12);

    assert!(text.contains("1/"), "no position indicator: {text}");
}

#[test]
fn scrolling_past_the_last_line_is_clamped_to_it() {
    let overlay = open_overlay();
    let mut scrolled = overlay.clone();
    scrolled.scroll_by(500);
    rendered(&scrolled, 100, 20);

    let at_end = scrolled.scroll_offset();
    assert!(at_end > 0, "nothing scrolled");
    let mut again = scrolled.clone();
    again.scroll_by(1);
    rendered(&again, 100, 20);
    assert_eq!(again.scroll_offset(), at_end, "the clamp did not stick");
}

#[test]
fn closing_returns_the_view_to_the_first_line() {
    let mut overlay = open_overlay();
    overlay.scroll_by(5);
    overlay.close();

    assert!(!overlay.open);
    assert_eq!(overlay.scroll_offset(), 0);
}

#[test]
fn a_screen_too_small_for_the_frame_draws_nothing_rather_than_panicking() {
    let text = rendered(&open_overlay(), 10, 3);

    assert!(!text.contains("Keyboard help"), "got: {text}");
}

#[test]
fn every_section_reaches_the_screen_when_it_is_tall_enough() {
    let mut overlay = open_overlay();
    let mut seen = 0;
    for _ in 0..40 {
        let text = rendered(&overlay, 100, 20);
        seen += HELP_SECTIONS
            .iter()
            .filter(|section| text.contains(section.title))
            .count();
        overlay.scroll_by(5);
    }

    assert!(seen >= HELP_SECTIONS.len(), "sections seen: {seen}");
}
