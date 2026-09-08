use super::PaneEmulator;

fn link(input: &str, row: u16, col: u16) -> Option<String> {
    let mut emulator = PaneEmulator::new(4, 80, 8);
    emulator.process(input.as_bytes());
    emulator.view().link_at(row, col)
}

fn link_at(input: &str, needle: &str, target: &str) {
    let col = input
        .find(needle)
        .map(|offset| input[..offset].chars().count() as u16)
        .expect("needle in fixture");
    assert_eq!(link(input, 0, col).as_deref(), Some(target));
}

#[test]
fn osc8_is_authoritative_for_glyph_and_wide_spacer() {
    let target = "https://example.test/wide";
    let input = format!("\x1b]8;;{target}\x1b\\가\x1b]8;;\x1b\\");
    let mut emulator = PaneEmulator::new(4, 80, 0);
    emulator.process(input.as_bytes());
    let view = emulator.view();

    assert_eq!(view.link_at(0, 0).as_deref(), Some(target));
    assert_eq!(view.link_at(0, 1).as_deref(), Some(target));
}

#[test]
fn markdown_label_returns_destination() {
    assert_eq!(
        link("See [the source](src/main.rs:42:7) here", 0, 6).as_deref(),
        Some("src/main.rs:42:7")
    );
}

#[test]
fn urls_keep_balanced_parentheses_and_drop_sentence_punctuation() {
    let input = "Open https://example.test/a_(b), then stop";
    assert_eq!(
        link(input, 0, 16).as_deref(),
        Some("https://example.test/a_(b)")
    );
}

#[test]
fn url_schemes_are_case_insensitive() {
    link_at(
        "Open HTTPS://example.test/docs",
        "HTTPS://example.test/docs",
        "HTTPS://example.test/docs",
    );
}

#[test]
fn wrapped_urls_are_one_visible_link() {
    let mut emulator = PaneEmulator::new(3, 10, 0);
    emulator.process(b"https://example.test/path");
    assert_eq!(
        emulator.view().link_at(1, 2).as_deref(),
        Some("https://example.test/path")
    );
}

#[test]
fn paths_with_line_locations_are_detected_without_link_metadata() {
    let input = r"Open C:\repo\src\main.rs:12:4 and src/lib.rs#L9";
    assert_eq!(
        link(input, 0, 10).as_deref(),
        Some(r"C:\repo\src\main.rs:12:4")
    );
    assert_eq!(link(input, 0, 39).as_deref(), Some("src/lib.rs#L9"));
}

#[test]
fn absolute_home_and_unicode_paths_support_line_locations() {
    link_at("/repo/한글.rs:12", "/repo/한글.rs:12", "/repo/한글.rs:12");
    link_at(
        "/repo/한글.rs#L12C3-L14C2",
        "/repo/한글.rs#L12C3-L14C2",
        "/repo/한글.rs#L12C3-L14C2",
    );
    link_at("~/src/main.rs:4", "~/src/main.rs:4", "~/src/main.rs:4");
}

#[test]
fn malformed_markdown_does_not_hide_a_later_valid_link() {
    let input = "[unfinished text [open](https://example.test/ok)";
    link_at(input, "open", "https://example.test/ok");
}

#[test]
fn file_like_names_without_locations_are_detected_but_bare_words_are_not() {
    link_at("Read README.md next", "README.md", "README.md");
    link_at("Open src/main.rs now", "src/main.rs", "src/main.rs");
    let mut emulator = PaneEmulator::new(2, 80, 0);
    emulator.process(b"ordinary words version.2");
    assert_eq!(emulator.view().link_at(0, 0), None);
    assert_eq!(emulator.view().link_at(0, 18), None);
}

#[test]
fn filename_line_ranges_and_hash_columns_are_detected_without_truncation() {
    for input in ["README.md:12-14", "README.md:12:3-14", "README.md#L12C3"] {
        link_at(input, input, input);
    }
    for input in ["README.md:12-", "README.md:12:", "README.md#L12C"] {
        let mut emulator = PaneEmulator::new(2, 80, 0);
        emulator.process(input.as_bytes());
        assert_eq!(emulator.view().link_at(0, 0), None, "{input}");
    }
}

#[test]
fn scrollback_uses_viewport_coordinates() {
    let mut emulator = PaneEmulator::new(2, 30, 8);
    emulator.process(b"old\r\nhttps://example.test/history\r\nnow");
    emulator.set_scroll_offset(1);
    assert_eq!(
        emulator.view().link_at(1, 8).as_deref(),
        Some("https://example.test/history")
    );
}

#[test]
fn empty_plain_text_and_out_of_range_are_not_links() {
    let mut emulator = PaneEmulator::new(2, 30, 0);
    emulator.process(b"ordinary words");
    let view = emulator.view();
    assert_eq!(view.link_at(0, 0), None);
    assert_eq!(view.link_at(99, 0), None);
    assert_eq!(view.link_at(0, 99), None);
    assert_eq!(PaneEmulator::new(2, 30, 0).view().link_at(0, 0), None);
}
