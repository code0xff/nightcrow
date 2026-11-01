//! How one cell's appearance is written into a snapshot: an absolute `SGR` that leads
//! with a reset, and the OSC 8 sequences that carry a link.
//!
//! A child of [`snapshot`](super) so it can read the pen the row walk carries. The
//! snapshot decides *what* to write; this only writes it.

use super::Pen;
use alacritty_terminal::term::cell::{Flags, Hyperlink};
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use std::fmt::Write as _;

/// One absolute `SGR`. Leads with `0` so the sequence states the whole pen
/// rather than a change from whatever the reader had.
///
/// Appended in place rather than returned: on a densely coloured screen this
/// runs once per cell, and building a string per call was measurably the cost
/// of the whole snapshot.
pub(super) fn write_sgr(out: &mut String, pen: &Pen) {
    out.push_str("\x1b[0");
    for (flag, param) in [
        (Flags::BOLD, "1"),
        (Flags::DIM, "2"),
        (Flags::ITALIC, "3"),
        (Flags::UNDERLINE, "4"),
        (Flags::DOUBLE_UNDERLINE, "21"),
        (Flags::UNDERCURL, "4:3"),
        (Flags::DOTTED_UNDERLINE, "4:4"),
        (Flags::DASHED_UNDERLINE, "4:5"),
        (Flags::INVERSE, "7"),
        (Flags::HIDDEN, "8"),
        (Flags::STRIKEOUT, "9"),
    ] {
        if pen.flags.contains(flag) {
            out.push(';');
            out.push_str(param);
        }
    }
    write_color(out, pen.fg, true);
    write_color(out, pen.bg, false);
    out.push('m');
}

pub(super) fn clear_hyperlink(out: &mut String, hyperlink: &mut Option<Hyperlink>) {
    if hyperlink.is_some() {
        write_hyperlink(out, None);
        *hyperlink = None;
    }
}

pub(in crate::runtime::emulator) fn write_hyperlink(
    out: &mut String,
    hyperlink: Option<&Hyperlink>,
) {
    out.push_str("\x1b]8;;");
    if let Some(hyperlink) = hyperlink {
        for character in hyperlink.uri().chars() {
            if character.is_control() || character == '\u{7f}' {
                let mut encoded = [0; 4];
                for byte in character.encode_utf8(&mut encoded).bytes() {
                    let _ = write!(out, "%{byte:02X}");
                }
            } else {
                out.push(character);
            }
        }
    }
    out.push_str("\x1b\\");
}

/// The `SGR` parameter selecting `color`. The default is written as nothing —
/// every sequence starts from a reset, so it needs no saying.
///
/// A named colour with no fixed palette slot (`Cursor`, `BrightForeground`,
/// `DimForeground`) defers to the default for the same reason
/// [`to_ratatui_color`](super::super::view::to_ratatui_color) renders it as `Reset`:
/// where it lands is the host terminal's business, not this snapshot's.
fn write_color(out: &mut String, color: Color, foreground: bool) {
    let (basic, bright, extended) = if foreground {
        (30, 90, 38)
    } else {
        (40, 100, 48)
    };
    match color {
        Color::Spec(rgb) => {
            let _ = write!(out, ";{extended};2;{};{};{}", rgb.r, rgb.g, rgb.b);
        }
        Color::Indexed(index) => write_indexed(out, index, basic, bright, extended),
        Color::Named(named) => {
            let index = named as usize;
            if index < 16 {
                write_indexed(out, index as u8, basic, bright, extended);
            } else if let Some(dim) = dim_slot(named) {
                write_indexed(out, dim, basic, bright, extended);
            }
        }
    }
}

/// The palette slot a dim named colour stands for. `CellView::dim` carries the dim
/// attribute itself, matching how the renderers read it.
fn dim_slot(named: NamedColor) -> Option<u8> {
    match named {
        NamedColor::DimBlack => Some(0),
        NamedColor::DimRed => Some(1),
        NamedColor::DimGreen => Some(2),
        NamedColor::DimYellow => Some(3),
        NamedColor::DimBlue => Some(4),
        NamedColor::DimMagenta => Some(5),
        NamedColor::DimCyan => Some(6),
        NamedColor::DimWhite => Some(7),
        _ => None,
    }
}

fn write_indexed(out: &mut String, index: u8, basic: u8, bright: u8, extended: u8) {
    if index < 8 {
        let _ = write!(out, ";{}", basic + index);
    } else if index < 16 {
        let _ = write!(out, ";{}", bright + (index - 8));
    } else {
        let _ = write!(out, ";{extended};5;{index}");
    }
}
