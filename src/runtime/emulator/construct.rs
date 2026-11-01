//! Opening a pane emulator with or without the stream-boundary tracking the
//! session hub reads. Every other caller renders its own screen from the same
//! bytes and never anchors a snapshot into them, so it skips the per-byte work.

use super::boundary;
use super::{EventProxy, PaneEmulator, term_size};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;

impl PaneEmulator {
    /// Create an emulator with a `rows` x `cols` screen (clamped to the
    /// minimum grid alacritty supports) and `scrollback_lines` of history,
    /// tracking stream boundaries for [`at_boundary`](Self::at_boundary).
    pub fn new(rows: u16, cols: u16, scrollback_lines: usize) -> Self {
        Self::create(
            rows,
            cols,
            scrollback_lines,
            Some(boundary::StreamBoundary::default()),
        )
    }

    /// Create an emulator without stream-boundary tracking, for a caller that
    /// renders its own screen from the bytes and never anchors a snapshot into
    /// them — every client. The screen is the same as [`new`](Self::new)'s;
    /// only the per-byte bookkeeping the session hub reads is gone.
    ///
    /// [`at_boundary`](Self::at_boundary) has no answer for this emulator and
    /// must not be called on it.
    pub fn without_boundary_tracking(rows: u16, cols: u16, scrollback_lines: usize) -> Self {
        Self::create(rows, cols, scrollback_lines, None)
    }

    fn create(
        rows: u16,
        cols: u16,
        scrollback_lines: usize,
        boundary: Option<boundary::StreamBoundary>,
    ) -> Self {
        let config = Config {
            scrolling_history: scrollback_lines,
            ..Config::default()
        };
        let proxy = EventProxy::default();
        let term = Term::new(config, &term_size(rows, cols), proxy.clone());
        Self {
            term,
            processor: Processor::new(),
            proxy,
            boundary,
        }
    }
}
