use super::links_paths::location_suffix;
use super::view::ScreenView;

const MAX_LINK_SCAN_BYTES: usize = 16 * 1024;

struct CellSpan {
    row: u16,
    col: u16,
    start: usize,
    end: usize,
}

pub(super) fn find(view: &ScreenView<'_>, row: u16, col: u16) -> Option<String> {
    let (rows, cols) = view.size();
    let cell = view.cell(row, col)?;
    if let Some(target) = cell.hyperlink() {
        return Some(target);
    }

    let mut first = row;
    let last_col = cols.saturating_sub(1);
    while first > 0 && view.cell(first - 1, last_col)?.is_wrap_line() {
        first -= 1;
    }
    let mut last = row;
    while last + 1 < rows && view.cell(last, last_col)?.is_wrap_line() {
        last += 1;
    }

    let mut text = String::new();
    let mut spans = Vec::new();
    let mut previous_span = None;
    let mut complete = true;
    'rows: for current_row in first..=last {
        if current_row > first
            && !view
                .cell(current_row - 1, last_col)
                .is_some_and(|line| line.is_wrap_line())
        {
            if text.len() == MAX_LINK_SCAN_BYTES {
                complete = false;
                break;
            }
            text.push('\n');
        }
        for current_col in 0..cols {
            if text.len() >= MAX_LINK_SCAN_BYTES {
                complete = false;
                break 'rows;
            }
            let current = view.cell(current_row, current_col)?;
            let start = text.len();
            if current.is_wide_spacer() {
                let (start, end) = previous_span.unwrap_or((start, start));
                spans.push(CellSpan {
                    row: current_row,
                    col: current_col,
                    start,
                    end,
                });
                continue;
            } else {
                let mut contents = String::new();
                current.append_contents(&mut contents);
                if text.len() + contents.len() > MAX_LINK_SCAN_BYTES {
                    complete = false;
                    break 'rows;
                }
                text.push_str(&contents);
            }
            previous_span = Some((start, text.len()));
            spans.push(CellSpan {
                row: current_row,
                col: current_col,
                start,
                end: text.len(),
            });
        }
    }

    let clicked = spans
        .iter()
        .find(|span| span.row == row && span.col == col)
        .map(|span| {
            if span.start == span.end {
                span.start.saturating_sub(1)
            } else {
                span.start
            }
        })?;
    find_markdown(&text, clicked, complete)
        .or_else(|| find_url(&text, clicked, complete))
        .or_else(|| find_path(&text, clicked, complete))
}

fn find_markdown(text: &str, clicked: usize, complete: bool) -> Option<String> {
    for (open, _) in text.match_indices('[') {
        if open > 0 && text[..open].ends_with('!') {
            continue;
        }
        let Some(close) = matching_bracket(text, open, '[', ']') else {
            continue;
        };
        let target_open = close + 1;
        if text.as_bytes().get(target_open) != Some(&b'(') {
            continue;
        }
        let Some(target_close) = matching_bracket(text, target_open, '(', ')') else {
            continue;
        };
        if !complete && target_close == text.len() - 1 {
            continue;
        }
        if clicked < open || clicked > close {
            continue;
        }
        let mut target = text[target_open + 1..target_close].trim();
        if target.starts_with('<') && target.ends_with('>') {
            target = target[1..target.len() - 1].trim();
        }
        if !target.is_empty() {
            return Some(target.to_owned());
        }
    }
    None
}

fn matching_bracket(text: &str, open: usize, left: char, right: char) -> Option<usize> {
    let mut depth = 0;
    let mut escaped = false;
    for (offset, character) in text[open..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
        } else if character == left {
            depth += 1;
        } else if character == right {
            depth -= 1;
            if depth == 0 {
                return Some(open + offset);
            }
        }
    }
    None
}

fn find_url(text: &str, clicked: usize, complete: bool) -> Option<String> {
    for scheme in ["https://", "http://"] {
        for (start, _) in text.char_indices() {
            if !text[start..]
                .get(..scheme.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(scheme))
            {
                continue;
            }
            if start > 0
                && text[..start].chars().next_back().is_some_and(|previous| {
                    previous.is_ascii_alphanumeric() || "._-".contains(previous)
                })
            {
                continue;
            }
            let raw_end = text[start..]
                .char_indices()
                .find(|(_, character)| character.is_whitespace() || character.is_control())
                .map_or(text.len(), |(offset, _)| start + offset);
            let end = trim_punctuation(text, start, raw_end, true);
            if end > start && (complete || end < text.len()) && clicked >= start && clicked < end {
                return Some(text[start..end].to_owned());
            }
        }
    }
    None
}

fn find_path(text: &str, clicked: usize, complete: bool) -> Option<String> {
    for (start, character) in text.char_indices() {
        if !character.is_alphanumeric() && !"._/~\\".contains(character) {
            continue;
        }
        if start > 0
            && !text[..start]
                .chars()
                .next_back()
                .is_some_and(is_token_boundary)
        {
            continue;
        }
        let raw_end = text[start..]
            .char_indices()
            .find(|(_, character)| character.is_whitespace() || character.is_control())
            .map_or(text.len(), |(offset, _)| start + offset);
        let end = trim_punctuation(text, start, raw_end, false);
        let candidate = &text[start..end];
        let path_end = location_suffix(candidate).unwrap_or(candidate.len());
        let path = &candidate[..path_end];
        if !is_file_like_path(path) {
            continue;
        }
        if (complete || end < text.len()) && clicked >= start && clicked < end {
            return Some(candidate.to_owned());
        }
    }
    None
}

fn is_file_like_path(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    if path.contains(['/', '\\']) {
        return true;
    }
    let Some((stem, extension)) = path.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && (1..=16).contains(&extension.len())
        && extension.bytes().all(|byte| byte.is_ascii_alphabetic())
}

fn is_token_boundary(character: char) -> bool {
    character.is_whitespace() || "([{<'\"`".contains(character)
}

fn trim_punctuation(text: &str, start: usize, mut end: usize, balanced: bool) -> usize {
    while let Some(character) = text[start..end].chars().next_back() {
        let strip = match character {
            '.' | ',' | ';' | ':' | '!' | '?' | ']' | '}' | '\'' | '"' | '`' => true,
            ')' if balanced => {
                text[start..end].chars().filter(|c| *c == ')').count()
                    > text[start..end].chars().filter(|c| *c == '(').count()
            }
            ')' => true,
            _ => false,
        };
        if !strip {
            break;
        }
        end -= character.len_utf8();
    }
    end
}
