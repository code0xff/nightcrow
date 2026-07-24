use super::view::to_ratatui_color;
use super::*;
use alacritty_terminal::vte::ansi::{Color, NamedColor};

fn contents(view: &ScreenView<'_>, row: u16, col: u16) -> String {
    let mut out = String::new();
    view.cell(row, col).unwrap().append_contents(&mut out);
    out
}

#[test]
fn process_writes_text_and_tracks_cursor() {
    let mut emu = PaneEmulator::new(3, 10, 0);
    emu.process(b"hi");

    assert_eq!(contents(&emu.view(), 0, 0), "h");
    assert_eq!(contents(&emu.view(), 0, 1), "i");
    assert_eq!(emu.view().cursor_position(), (0, 2));
}

#[test]
fn osc_title_is_captured_and_cleaned() {
    let mut emu = PaneEmulator::new(3, 20, 0);
    // Embedded control bytes and surrounding whitespace must not leak
    // into the tab label; OSC 0 (icon + title) works like OSC 2.
    let events = emu.process(b"\x1b]0;  cargo\t test  \x07");
    assert_eq!(events.title.as_deref(), Some("cargo test"));

    // An empty (or whitespace-only) title must not override the current one.
    let events = emu.process(b"\x1b]2;   \x1b\\");
    assert_eq!(events.title, None);
}

#[test]
fn title_is_none_when_chunk_sets_no_title() {
    let mut emu = PaneEmulator::new(3, 20, 0);
    let events = emu.process(b"plain output");
    assert_eq!(events.title, None);
}

#[test]
fn cursor_position_report_produces_pty_write() {
    let mut emu = PaneEmulator::new(5, 20, 0);
    // DSR 6: the program asks where the cursor is; the emulator must
    // answer through the PTY (vt100 silently dropped such queries).
    let events = emu.process(b"\x1b[2;3H\x1b[6n");
    assert_eq!(events.pty_writes, b"\x1b[2;3R");
}

#[test]
fn bracketed_paste_mode_follows_decset() {
    let mut emu = PaneEmulator::new(3, 10, 0);
    assert!(!emu.view().bracketed_paste());
    emu.process(b"\x1b[?2004h");
    assert!(emu.view().bracketed_paste());
    emu.process(b"\x1b[?2004l");
    assert!(!emu.view().bracketed_paste());
}

#[test]
fn wide_char_occupies_two_columns_with_spacer() {
    let mut emu = PaneEmulator::new(3, 10, 0);
    emu.process("가".as_bytes());

    let view = emu.view();
    assert_eq!(contents(&view, 0, 0), "가");
    assert!(!view.cell(0, 0).unwrap().is_wide_spacer());
    assert!(view.cell(0, 1).unwrap().is_wide_spacer());
}

#[test]
fn shrink_through_wide_char_then_erase_does_not_panic() {
    // Regression for the crash that motivated this module: vt100
    // panicked (row.rs clear_wide index out of bounds) when a resize
    // truncated a wide character at the last column and the program
    // then issued Erase Display. See vt100-rust issue #28.
    let mut emu = PaneEmulator::new(5, 20, 0);
    emu.process("가나다라마바사아자차".as_bytes());
    emu.resize(5, 19);
    emu.process(b"\x1b[1;1H\x1b[J");

    // Survival is the contract; the screen must also report the new size.
    assert_eq!(emu.view().size(), (5, 19));
}

#[test]
fn every_shrink_width_survives_wide_char_erase() {
    for cols in 1..20u16 {
        let mut emu = PaneEmulator::new(5, 20, 0);
        emu.process("가나다라마바사아자차".as_bytes());
        emu.resize(5, cols);
        emu.process(b"\x1b[1;1H\x1b[J");
    }
}

#[test]
fn scroll_offset_is_clamped_to_history() {
    let mut emu = PaneEmulator::new(3, 10, 5);
    // 3-row screen + 8 lines written = 5 lines of history (within cap).
    for i in 0..8 {
        emu.process(format!("line{i}\r\n").as_bytes());
    }
    let applied = emu.set_scroll_offset(9999);
    assert_eq!(applied, 5);
    assert_eq!(emu.set_scroll_offset(0), 0);
}

#[test]
fn scrolled_view_shows_history_lines() {
    let mut emu = PaneEmulator::new(3, 10, 100);
    for i in 0..10 {
        emu.process(format!("line{i}\r\n").as_bytes());
    }
    emu.set_scroll_offset(2);
    // Live top row would be line8; scrolled back 2 it must show line6.
    let view = emu.view();
    let row: String = (0..5).map(|c| contents(&view, 0, c)).collect();
    assert_eq!(row, "line6");
}

#[test]
fn scroll_sink_defaults_to_scrollback_for_a_plain_shell() {
    let emu = PaneEmulator::new(3, 10, 100);
    assert_eq!(emu.scroll_sink(), ScrollSink::Scrollback);
}

#[test]
fn scroll_sink_is_mouse_wheel_when_program_reports_sgr_mouse() {
    let mut emu = PaneEmulator::new(3, 10, 100);
    // The exact mode set Claude Code emits on startup.
    emu.process(b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h");
    assert_eq!(emu.scroll_sink(), ScrollSink::MouseWheel);
}

#[test]
fn scroll_sink_ignores_mouse_reporting_without_sgr_encoding() {
    let mut emu = PaneEmulator::new(3, 10, 100);
    // X10-encoded mouse reporting: we have no encoder for it, so the
    // pane must not be handed wheel bytes it cannot parse.
    emu.process(b"\x1b[?1000h");
    assert_eq!(emu.scroll_sink(), ScrollSink::Scrollback);
}

#[test]
fn wants_mouse_buttons_when_program_reports_sgr_mouse() {
    let mut emu = PaneEmulator::new(3, 10, 100);
    // The exact mode set Claude Code emits on startup.
    emu.process(b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h");
    assert!(emu.wants_mouse_buttons());
}

#[test]
fn wants_mouse_buttons_rejects_shell_and_x10_only_panes() {
    let mut emu = PaneEmulator::new(3, 10, 100);
    // A plain shell never claims the mouse: no click byte may reach it.
    assert!(!emu.wants_mouse_buttons());
    // X10-encoded reporting without SGR: we have no encoder for it.
    emu.process(b"\x1b[?1000h");
    assert!(!emu.wants_mouse_buttons());
}

#[test]
fn scroll_sink_is_arrow_keys_on_alternate_screen() {
    let mut emu = PaneEmulator::new(3, 10, 100);
    emu.process(b"\x1b[?1049h");
    assert_eq!(emu.scroll_sink(), ScrollSink::ArrowKeys);
}

#[test]
fn scroll_sink_falls_back_when_alternate_scroll_is_disabled() {
    let mut emu = PaneEmulator::new(3, 10, 100);
    emu.process(b"\x1b[?1049h\x1b[?1007l");
    assert_eq!(emu.scroll_sink(), ScrollSink::Scrollback);
}

#[test]
fn scroll_sink_prefers_mouse_wheel_over_alternate_screen() {
    let mut emu = PaneEmulator::new(3, 10, 100);
    emu.process(b"\x1b[?1049h\x1b[?1000h\x1b[?1006h");
    assert_eq!(emu.scroll_sink(), ScrollSink::MouseWheel);
}

#[test]
fn alternate_screen_keeps_no_scrollback() {
    // The reason `ScrollSink` exists: alacritty gives the alternate grid
    // zero history, so a scroll offset there can never leave 0 and the
    // grid has nothing to reveal.
    let mut emu = PaneEmulator::new(3, 10, 100);
    emu.process(b"\x1b[?1049h");
    for i in 0..20 {
        emu.process(format!("line{i}\r\n").as_bytes());
    }
    assert_eq!(emu.set_scroll_offset(999), 0);
}

#[test]
fn app_cursor_follows_decckm() {
    let mut emu = PaneEmulator::new(3, 10, 0);
    assert!(!emu.app_cursor());
    emu.process(b"\x1b[?1h");
    assert!(emu.app_cursor());
    emu.process(b"\x1b[?1l");
    assert!(!emu.app_cursor());
}

#[test]
fn zero_size_is_clamped_to_minimum_grid() {
    // alacritty's documented minimum is 1 line x 2 columns; anything
    // smaller (a 1-column grid especially) breaks wide-char reflow.
    let mut emu = PaneEmulator::new(0, 0, 0);
    assert_eq!(emu.view().size(), (1, 2));
    emu.resize(0, 0);
    assert_eq!(emu.view().size(), (1, 2));
    emu.process(b"x"); // must not panic on the minimal grid
}

#[test]
fn named_colors_map_to_indexed_and_defaults_to_reset() {
    use ratatui::style::Color as C;
    assert_eq!(
        to_ratatui_color(Color::Named(NamedColor::Red)),
        C::Indexed(1)
    );
    assert_eq!(
        to_ratatui_color(Color::Named(NamedColor::BrightWhite)),
        C::Indexed(15)
    );
    assert_eq!(
        to_ratatui_color(Color::Named(NamedColor::DimBlue)),
        C::Indexed(4)
    );
    assert_eq!(
        to_ratatui_color(Color::Named(NamedColor::Foreground)),
        C::Reset
    );
    assert_eq!(to_ratatui_color(Color::Indexed(42)), C::Indexed(42));
    assert_eq!(
        to_ratatui_color(Color::Spec(alacritty_terminal::vte::ansi::Rgb {
            r: 1,
            g: 2,
            b: 3
        })),
        C::Rgb(1, 2, 3)
    );
}