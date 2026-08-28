//! Whether a pane's byte stream stands at a sequence boundary.
//!
//! A snapshot is spliced *into* the recorded stream on replay (see
//! `session::terminal::hub_replay`), and PTY reads land at arbitrary byte
//! offsets — anchoring inside an escape sequence or multi-byte character hands
//! a reattaching client the sequence's tail as ordinary input. The emulator's
//! own parser knows it is mid-sequence but does not say so, so this mirrors
//! just enough of its state machine to answer "is a sequence in flight".
//!
//! The mirror tracks the parser's *abort* semantics (`CAN`, `SUB`, a fresh
//! `ESC`) as well as its progress, so it cannot drift into claiming a sequence
//! the real parser has abandoned. Where they disagree, they disagree safely:
//! this may stay "open" after the parser moved on, which only defers a
//! snapshot — it never reports a boundary the parser would not also be at.
//! The cost of deferring is the caller's to bound (see `session::terminal::hub_run`);
//! a cap *here* would be a lie told to every caller at once.

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Escape,
    EscapeIntermediate,
    Csi,
    Osc,
    OscEsc,
    /// DCS, SOS, PM and APC — one state, because all that matters here is that
    /// each runs until `ST`.
    Str,
    StrEsc,
    /// A multi-byte UTF-8 character with this many continuation bytes to go.
    Utf8(u8),
}

pub(super) struct StreamBoundary {
    state: State,
}

impl Default for StreamBoundary {
    fn default() -> Self {
        Self {
            state: State::Ground,
        }
    }
}

impl StreamBoundary {
    pub(super) fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.step(byte);
        }
    }

    /// Whether the stream so far ends with every sequence closed.
    pub(super) fn at_boundary(&self) -> bool {
        self.state == State::Ground
    }

    fn step(&mut self, byte: u8) {
        use State::*;
        // An abort re-dispatches its byte from the state it fell back to — an
        // `ESC` that cut an OSC short is also the start of whatever comes next.
        let mut again = true;
        while again {
            again = false;
            self.state = match self.state {
                Ground => match byte {
                    0x1b => Escape,
                    0xc2..=0xdf => Utf8(1),
                    0xe0..=0xef => Utf8(2),
                    0xf0..=0xf4 => Utf8(3),
                    // Controls execute in place; stray continuation bytes are
                    // replacement characters. Neither opens anything.
                    _ => Ground,
                },
                Utf8(left) => match byte {
                    0x80..=0xbf if left == 1 => Ground,
                    0x80..=0xbf => Utf8(left - 1),
                    // Invalid where a continuation was due: the character is
                    // abandoned and this byte is processed on its own.
                    _ => {
                        again = true;
                        Ground
                    }
                },
                Escape => match byte {
                    b'[' => Csi,
                    b']' => Osc,
                    b'P' | b'X' | b'^' | b'_' => Str,
                    0x20..=0x2f => EscapeIntermediate,
                    0x18 | 0x1a => Ground,
                    0x1b => Escape,
                    0x30..=0x7e => Ground,
                    _ => Escape,
                },
                EscapeIntermediate => match byte {
                    0x20..=0x2f => EscapeIntermediate,
                    0x30..=0x7e | 0x18 | 0x1a => Ground,
                    0x1b => Escape,
                    _ => EscapeIntermediate,
                },
                Csi => match byte {
                    0x40..=0x7e | 0x18 | 0x1a => Ground,
                    0x1b => Escape,
                    _ => Csi,
                },
                Osc => match byte {
                    0x07 | 0x18 | 0x1a => Ground,
                    0x1b => OscEsc,
                    _ => Osc,
                },
                OscEsc => match byte {
                    b'\\' => Ground,
                    _ => {
                        again = true;
                        Escape
                    }
                },
                Str => match byte {
                    0x18 | 0x1a => Ground,
                    0x1b => StrEsc,
                    _ => Str,
                },
                StrEsc => match byte {
                    b'\\' => Ground,
                    _ => {
                        again = true;
                        Escape
                    }
                },
            };
        }
    }
}
