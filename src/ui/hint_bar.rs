use crate::app::{App, leader_label_of};
use crate::ui::chrome::{Chrome, chrome_rows};
use crate::ui::hint_text::{
    EMPTY_HINT, EMPTY_HINT_ARMED, PREFIX_CHIP, normal_hint_literal, prefix_armed_hint_text,
};
use crate::ui::repo_dialog::repo_dialog_hint_line;
use ratatui::{
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HintClick {
    Arm,
    Leader(char),
    Plain(char),
}

/// Leader follow-ups a `<prefix> x` segment can name and still be clicked.
const CLICKABLE_LEADER_KEYS: &str = "twflbo";
/// Keys a bare segment can name and still be clicked: the leader follow-ups as
/// they appear on the armed row, plus the commands the focused panel handles
/// unprefixed.
const CLICKABLE_PLAIN_KEYS: &str = "twslbfoxpruvzcn/";

/// The click a hint segment's keyspec resolves to, or `None` for a segment
/// that is not clickable. Command hints dispatch, navigation hints do not, and
/// `q: detach` is held back so detaching stays a deliberate two-key act. The
/// keys are listed rather than derived because nothing in the hint text tells
/// a command apart from a navigation hint — a command added to `hint_text`
/// stays silently unclickable until it is listed here. This list has already
/// had that gap.
pub(crate) fn segment_click(keyspec: &str) -> Option<HintClick> {
    let spec = keyspec.trim();
    if spec == "<prefix>" {
        return Some(HintClick::Arm);
    }
    if let Some(rest) = spec.strip_prefix("<prefix> ") {
        let mut chars = rest.chars();
        if let (Some(c), None) = (chars.next(), chars.next())
            && CLICKABLE_LEADER_KEYS.contains(c)
        {
            return Some(HintClick::Leader(c));
        }
        return None;
    }
    // The one chord in the list. `handlers.rs` reads it with
    // `matches_text_command(key, 'N')`, which looks at the character alone, so
    // the click carries `N` and no shift has to be synthesized to match.
    if spec == "shift+n" {
        return Some(HintClick::Plain('N'));
    }
    let mut chars = spec.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && CLICKABLE_PLAIN_KEYS.contains(c)
    {
        return Some(HintClick::Plain(c));
    }
    None
}

/// Build the styled spans for a hint legend, inverting (`REVERSED`) every
/// clickable segment so the bar itself shows which hints respond to a click.
/// Consumes the same literal and `" | "` segmentation as `hint_click_at` and
/// decides clickability with the same `segment_click`, so an inverted label
/// can never disagree with the hit test. `mark_clickable` is `[mouse] enabled`.
pub(crate) fn hint_spans(text: &str, leader: &str, mark_clickable: bool) -> Vec<Span<'static>> {
    let base = Style::default().fg(Color::DarkGray);
    let inverted = base.add_modifier(Modifier::REVERSED);
    let mut spans = Vec::new();
    for (i, segment) in text.split(" | ").enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", base));
        }
        let rendered = segment.replace("<prefix>", leader);
        let clickable = mark_clickable
            && segment
                .split_once(':')
                .and_then(|(keyspec, _)| segment_click(keyspec))
                .is_some();
        if clickable {
            // Invert the whole segment — the entire label is the click target.
            // Leading whitespace stays plain so the chip doesn't start with a
            // stray block.
            let label_start = rendered.len() - rendered.trim_start().len();
            let (lead_ws, label) = rendered.split_at(label_start);
            if !lead_ws.is_empty() {
                spans.push(Span::styled(lead_ws.to_string(), base));
            }
            spans.push(Span::styled(label.to_string(), inverted));
        } else {
            spans.push(Span::styled(rendered, base));
        }
    }
    spans
}

pub(crate) fn render_hint_bar<'a>(
    app: &'a App,
    chrome: Chrome<'a>,
    accent: Color,
    width: u16,
) -> Paragraph<'a> {
    if chrome.repo_input.active {
        // The input itself sits on the notice row; this row carries the
        // dialog's keys and its reports.
        return Paragraph::new(repo_dialog_hint_line(
            app.notice.as_ref(),
            chrome.repo_input,
            width,
        ));
    }
    let leader = leader_label_of(app.interaction.leader);
    if app.interaction.prefix_armed {
        let mut spans = vec![Span::styled(
            PREFIX_CHIP,
            Style::default()
                .fg(Color::Black)
                .bg(accent)
                .add_modifier(Modifier::BOLD),
        )];
        spans.extend(hint_spans(
            &prefix_armed_hint_text(app),
            &leader,
            app.interaction.mouse_enabled,
        ));
        return Paragraph::new(Line::from(spans));
    }
    if app.interaction.awaiting_swap_target {
        // The swap-target digits follow the same layout-aware mapping as the
        // focus jumps: `1-8` fullscreen, `3-9,0` in the split view.
        let digits = if app.terminal.fullscreen.fills_body() {
            "1-8"
        } else {
            "3-9,0"
        };
        return Paragraph::new(Line::from(vec![
            Span::styled(
                " SWAP ",
                Style::default()
                    .fg(Color::Black)
                    .bg(accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {digits}: swap active pane with this pane | esc: cancel"),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    // `<prefix>` resolves to the configured leader chord (e.g. `^F`) so the
    // footer names the actual key to press rather than an abstract word.
    Paragraph::new(Line::from(hint_spans(
        normal_hint_literal(app),
        &leader,
        app.interaction.mouse_enabled,
    )))
}

/// The click action for `(x, y)` on the empty screen's hint row, or `None`
/// off it. Only `o` resolves — detaching stays a deliberate keyboard act, as
/// on the project screen.
pub(crate) fn empty_hint_click_at(
    screen_area: Rect,
    leader_label: &str,
    prefix_armed: bool,
    mouse_enabled: bool,
    x: u16,
    y: u16,
) -> Option<HintClick> {
    // Gated like `hint_click_at`: with capture off the row renders plain, but
    // a browser mouse event still reaches this path, so a label that does not
    // advertise itself as clickable must not act like one either.
    if !mouse_enabled {
        return None;
    }
    let hint_area = chrome_rows(screen_area).hint;
    if hint_area.height == 0 || !hint_area.contains(Position { x, y }) {
        return None;
    }
    let (chip, text) = if prefix_armed {
        (PREFIX_CHIP, EMPTY_HINT_ARMED)
    } else {
        ("", EMPTY_HINT)
    };
    let mut cursor = hint_area.x + Span::raw(chip).width() as u16;
    for (i, segment) in text.split(" | ").enumerate() {
        if i > 0 {
            cursor += Span::raw(" | ").width() as u16;
        }
        let rendered = segment.replace("<prefix>", leader_label);
        let width = Span::raw(rendered.as_str()).width() as u16;
        if x >= cursor && x < cursor + width {
            // Same rules as `hint_click_at`: leading whitespace renders plain
            // and so is not part of the target, and the key is the text before
            // the colon.
            let label_start = rendered.len() - rendered.trim_start().len();
            let lead_width = Span::raw(&rendered[..label_start]).width() as u16;
            if x < cursor + lead_width {
                return None;
            }
            let (keyspec, _) = segment.split_once(':')?;
            return segment_click(keyspec);
        }
        cursor += width;
    }
    None
}

pub(crate) fn hint_click_at(
    app: &App,
    chrome: Chrome<'_>,
    screen_area: Rect,
    x: u16,
    y: u16,
) -> Option<HintClick> {
    // With mouse capture off no click can reach us anyway, but the bar also
    // renders no inverted labels (`hint_spans`) — keep the affordance and the
    // hit test in agreement rather than relying on the caller.
    if !app.interaction.mouse_enabled {
        return None;
    }
    let hint_area = chrome_rows(screen_area).hint;
    if hint_area.height == 0 || !hint_area.contains(Position { x, y }) {
        return None;
    }

    // Row selection mirrors `render_hint_bar`'s branch order exactly, or a
    // click would resolve against a row the user isn't looking at.
    let (chip, text) = if chrome.repo_input.active {
        return None;
    } else if app.interaction.prefix_armed {
        (PREFIX_CHIP, prefix_armed_hint_text(app))
    } else if app.interaction.awaiting_swap_target {
        return None;
    } else {
        ("", normal_hint_literal(app).to_string())
    };

    let leader = leader_label_of(app.interaction.leader);
    let mut cursor = hint_area.x + Span::raw(chip).width() as u16;
    for (i, segment) in text.split(" | ").enumerate() {
        if i > 0 {
            cursor += Span::raw(" | ").width() as u16;
        }
        let rendered = segment.replace("<prefix>", &leader);
        let width = Span::raw(rendered.as_str()).width() as u16;
        if x >= cursor && x < cursor + width {
            // Leading whitespace renders plain (see `hint_spans`), so it is
            // not part of the click target either — the clickable range must
            // match the inverted label cell for cell.
            let label_start = rendered.len() - rendered.trim_start().len();
            let lead_width = Span::raw(&rendered[..label_start]).width() as u16;
            if x < cursor + lead_width {
                return None;
            }
            let (keyspec, _) = segment.split_once(':')?;
            return segment_click(keyspec);
        }
        cursor += width;
    }
    None
}
