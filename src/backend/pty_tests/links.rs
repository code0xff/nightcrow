//! End-to-end link discovery through a real local PTY.
//!
//! The child is this test binary itself. That keeps the fixture independent of
//! the host shell while still making the bytes cross the same PTY backend used
//! by an attached terminal.

use super::*;
use crate::runtime::emulator::PaneEmulator;
use std::io::Write;

const ROWS: u16 = 16;
const COLS: u16 = 100;

const FILE_TARGET: &str = "file:///workspace/src/main.rs";
const WEB_TARGET: &str = "https://example.invalid/docs";
const FILE_LABEL: &str = "FILE LINK";
const WEB_LABEL: &str = "WEB LINK";
const PLAIN_TARGET: &str = "src/runtime/emulator/view.rs:73";
const PLAIN_WEB_TARGET: &str = "https://example.invalid/fallback";

const FILE_ROW: u16 = 0;
const WEB_ROW: u16 = 1;
const PLAIN_ROW: u16 = 2;
const PLAIN_WEB_ROW: u16 = 3;

/// This test is only a child fixture for `a_real_pty_preserves_link_targets`.
/// `--ignored` and `--exact` make the parent launch this one test explicitly.
#[test]
#[ignore]
fn emit_link_fixture() {
    print!(
        "\x1b[2J\x1b[1;1H\x1b]8;;{FILE_TARGET}\x1b\\{FILE_LABEL}\x1b]8;;\x1b\\\x1b[2;1H\x1b]8;;{WEB_TARGET}\x1b\\{WEB_LABEL}\x1b]8;;\x1b\\\x1b[3;1H{PLAIN_TARGET}\r\n\x1b[4;1H{PLAIN_WEB_TARGET}\r\n"
    );
    std::io::stdout().flush().expect("fixture stdout flush");
}

fn fixture_test_name() -> &'static str {
    "backend::pty::tests::links::emit_link_fixture"
}

fn fixture_shell() -> ShellConfig {
    ShellConfig {
        program: Some(
            std::env::current_exe()
                .expect("current test executable")
                .to_string_lossy()
                .into_owned(),
        ),
        command_args: ["--ignored", "--nocapture", "--exact", "--test-threads=1"]
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

fn contains_osc8_target(output: &[u8], target: &str) -> bool {
    let prefix = b"\x1b]8;";
    let mut search_from = 0;
    while search_from < output.len() {
        let Some(offset) = output[search_from..]
            .windows(prefix.len())
            .position(|window| window == prefix)
        else {
            return false;
        };
        let start = search_from + offset + prefix.len();
        let Some(separator) = output[start..].iter().position(|byte| *byte == b';') else {
            return false;
        };
        let target_start = start + separator + 1;
        let mut end = target_start;
        while end < output.len() {
            if output[end] == b'\x07'
                || (output[end] == b'\x1b' && output.get(end + 1) == Some(&b'\\'))
            {
                break;
            }
            end += 1;
        }
        if output[target_start..end]
            .windows(target.len())
            .any(|window| window == target.as_bytes())
        {
            return true;
        }
        search_from = end.saturating_add(2);
    }
    false
}

fn screen_from(output: &[u8]) -> PaneEmulator {
    let mut emulator = PaneEmulator::new(ROWS, COLS, 0);
    emulator.process(output);
    emulator
}

fn assert_target(emulator: &PaneEmulator, row: u16, col: u16, target: &str) {
    assert_eq!(
        emulator.view().link_at(row, col).as_deref(),
        Some(target),
        "link target at ({row}, {col})"
    );
}

#[test]
fn a_real_pty_preserves_link_targets() {
    let mut backend = PtyBackend::new(".", fixture_shell());
    let pane = backend
        .open_pane(ROWS, COLS, Some(fixture_test_name()))
        .expect("open fixture pane");
    let drained = drain_until_exit(&mut backend, pane);

    assert_eq!(drained.exits, 1, "fixture test did not exit");
    assert!(!drained.output.is_empty(), "fixture PTY produced no output");
    let output = String::from_utf8_lossy(&drained.output);
    assert!(
        output.contains(FILE_LABEL),
        "file label missing: {output:?}"
    );
    assert!(output.contains(WEB_LABEL), "web label missing: {output:?}");
    assert!(
        output.contains(PLAIN_TARGET),
        "plain file reference missing: {output:?}"
    );
    assert!(
        output.contains(PLAIN_WEB_TARGET),
        "plain web URL missing: {output:?}"
    );
    let file_osc8 = contains_osc8_target(&drained.output, FILE_TARGET);
    let web_osc8 = contains_osc8_target(&drained.output, WEB_TARGET);
    #[cfg(unix)]
    {
        assert!(
            file_osc8,
            "Unix PTY must preserve the deterministic file OSC 8 target"
        );
        assert!(
            web_osc8,
            "Unix PTY must preserve the deterministic web OSC 8 target"
        );
    }
    let original = screen_from(&drained.output);
    assert_target(&original, PLAIN_ROW, 0, PLAIN_TARGET);
    assert_target(&original, PLAIN_WEB_ROW, 0, PLAIN_WEB_TARGET);
    if file_osc8 {
        assert_target(&original, FILE_ROW, 0, FILE_TARGET);
    }
    if web_osc8 {
        assert_target(&original, WEB_ROW, 0, WEB_TARGET);
    }

    let snapshot = original.screen_snapshot();
    let replayed = screen_from(&snapshot);
    assert_target(&replayed, PLAIN_ROW, 0, PLAIN_TARGET);
    assert_target(&replayed, PLAIN_WEB_ROW, 0, PLAIN_WEB_TARGET);
    if file_osc8 {
        assert_target(&replayed, FILE_ROW, 0, FILE_TARGET);
    }
    if web_osc8 {
        assert_target(&replayed, WEB_ROW, 0, WEB_TARGET);
    }

    backend.destroy_pane(pane);
}
