//! Ceilings on everything the viewer will serialize or hold open.
//!
//! Every route reads from a repository whose size the server does not control.
//! Without a ceiling, one request turns into an unbounded allocation and a
//! response no browser can render. Each limit below is the point past which
//! more data stops being useful to a human reading a diff.
//!
//! Truncation is always reported — [`Capped::truncated`] rides along into the
//! DTO so the UI can say "showing the first N", never silently imply it showed
//! everything.

/// Commits returned by one page of `/api/log`. Matches the TUI's
/// `commit_log_page_size` default so both surfaces move through history at the
/// same pace.
pub const MAX_LOG_PAGE: usize = 100;
// `/api/log?skip=` deliberately has no ceiling. A ceiling here would look
// prudent and protect nothing: the skip feeds `Iterator::skip` on a revwalk, so
// a request walks at most `skip + page` *or* the whole history, whichever is
// smaller. An absurd skip costs what walking the repository costs and no more,
// while a ceiling would turn the deep end of a long history into a page the
// client can see exists and can never fetch.
/// Changed paths returned while drilling into one commit.
pub const MAX_COMMIT_FILES: usize = 2_000;
/// Entries returned for one directory level of `/api/tree`.
pub const MAX_TREE_ENTRIES: usize = 2_000;
/// Depth cap for the recursive `/api/tree/search` walk. Matches the TUI tree's
/// `max_depth` (`config.rs`).
pub const MAX_TREE_SEARCH_DEPTH: usize = 64;
/// Entries one `/api/tree/search` walk may inspect before it stops and reports
/// the listing as incomplete. Bounds filesystem work per request.
pub const MAX_TREE_SEARCH_VISITS: usize = 100_000;
/// Matches returned by one `/api/tree/search` request.
pub const MAX_TREE_SEARCH_RESULTS: usize = 500;
/// Longest accepted `/api/tree/search` query. Anything past this is rejected
/// at the boundary rather than matched against every basename.
pub const MAX_TREE_SEARCH_QUERY_BYTES: usize = 256;
/// Changed files reported in one status payload.
pub const MAX_STATUS_FILES: usize = 2_000;
/// Bytes of diff text returned for one file.
pub const MAX_DIFF_BYTES: usize = 1024 * 1024;
/// Lines of diff returned for one file, whichever ceiling is hit first.
pub const MAX_DIFF_LINES: usize = 20_000;
/// Bytes of a single SSE payload. Status is conflated to the latest value, so
/// this bounds one snapshot, not a backlog.
pub const MAX_SSE_PAYLOAD_BYTES: usize = 1024 * 1024;
/// Terminals one repository may hold open at once. Each is a real process.
pub const MAX_PTYS_PER_REPO: usize = 8;
/// Bounds on a PTY's size. The client measures these from its own layout, so
/// they are input from outside and are clamped rather than trusted: a zero
/// dimension gives the child a terminal it cannot draw in (and can fail
/// `openpty` outright), and the far end lets one message ask a full-screen
/// program to allocate a screen buffer of `rows * cols` cells. The ceiling is
/// far above any real display — a 6K screen at a 6px font is under 500 columns
/// — so it never truncates a size someone actually has.
pub const MIN_PANE_DIMENSION: u16 = 1;
pub const MAX_PANE_ROWS: u16 = 1_000;
pub const MAX_PANE_COLS: u16 = 1_000;
/// Raw PTY bytes retained per terminal to replay to a (re)connecting client.
/// Restore is best-effort, not an exact snapshot: replaying only a byte-window
/// means a terminal mode set before that window (alternate screen, persistent
/// SGR) can be lost. A full-screen program repaints on the resize every client
/// sends right after connecting; a true fix would need server-side VT
/// emulation, which this viewer deliberately does not do (xterm.js is the only
/// emulator).
pub const MAX_TERMINAL_SCROLLBACK_BYTES: usize = 256 * 1024;
/// Live connections the viewer's accept loop will hold. Separate from the
/// mirror's cap: they are different servers on different ports.
pub const MAX_VIEWER_CONNECTIONS: usize = 64;

/// A list that may have been cut short, with the fact recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capped<T> {
    pub items: Vec<T>,
    /// True when entries were dropped to fit the ceiling.
    pub truncated: bool,
}

impl<T> Capped<T> {
    /// Keep at most `max` items, reporting whether any were dropped.
    pub fn new(mut items: Vec<T>, max: usize) -> Self {
        let truncated = items.len() > max;
        if truncated {
            items.truncate(max);
        }
        Self { items, truncated }
    }

    /// A list that fits by construction.
    pub fn untruncated(items: Vec<T>) -> Self {
        Self {
            items,
            truncated: false,
        }
    }
}

/// Cut `text` to at most `max_bytes`, never splitting a UTF-8 character. The
/// cut walks back to the nearest boundary so a multi-byte character
/// straddling the limit is dropped whole rather than emitted as a broken
/// fragment.
pub fn cap_text(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_string(), false);
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}

/// Cut `text` to at most `max_lines` lines *and* `max_bytes` bytes, whichever
/// binds first. A single enormous line is as unrenderable as a million small
/// ones, so a diff needs both ceilings.
pub fn cap_diff(text: &str, max_lines: usize, max_bytes: usize) -> (String, bool) {
    let mut kept = String::new();
    let mut truncated = false;
    for (index, line) in text.lines().enumerate() {
        if index >= max_lines {
            truncated = true;
            break;
        }
        // +1 for the newline this line will contribute.
        if kept.len() + line.len() + 1 > max_bytes {
            truncated = true;
            break;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    (kept, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capped_reports_when_it_drops_entries() {
        let full = Capped::new(vec![1, 2, 3, 4, 5], 3);
        assert_eq!(full.items, vec![1, 2, 3]);
        assert!(full.truncated);

        let fits = Capped::new(vec![1, 2], 3);
        assert_eq!(fits.items, vec![1, 2]);
        assert!(!fits.truncated, "an exact fit is not a truncation");

        let exact = Capped::new(vec![1, 2, 3], 3);
        assert!(!exact.truncated, "length == max is not a truncation");
    }

    #[test]
    fn capped_handles_a_zero_ceiling() {
        let none = Capped::new(vec![1, 2], 0);
        assert!(none.items.is_empty());
        assert!(none.truncated);
    }

    #[test]
    fn cap_text_keeps_short_input_untouched() {
        let (kept, truncated) = cap_text("hello", 10);
        assert_eq!(kept, "hello");
        assert!(!truncated);
    }

    #[test]
    fn cap_text_never_splits_a_multibyte_character() {
        // "한" is three bytes. Cutting at 2 or 4 lands mid-character; slicing
        // there would panic, so the partial character must be dropped whole.
        let text = "한글";
        for limit in [1, 2, 4, 5] {
            let (kept, truncated) = cap_text(text, limit);
            assert!(truncated, "limit {limit} must truncate");
            assert!(
                text.starts_with(&kept),
                "limit {limit} produced a non-prefix: {kept:?}"
            );
            assert!(
                kept.len() <= limit,
                "limit {limit} kept {} bytes",
                kept.len()
            );
        }
        assert_eq!(cap_text(text, 3).0, "한");
        assert_eq!(cap_text(text, 5).0, "한");
    }

    #[test]
    fn cap_text_can_return_nothing_when_the_first_character_does_not_fit() {
        let (kept, truncated) = cap_text("한", 2);
        assert_eq!(kept, "");
        assert!(truncated);
    }

    #[test]
    fn cap_diff_stops_at_the_line_ceiling() {
        let text = (0..100).map(|i| format!("line{i}\n")).collect::<String>();

        let (kept, truncated) = cap_diff(&text, 10, usize::MAX);

        assert_eq!(kept.lines().count(), 10);
        assert!(truncated);
        assert!(kept.starts_with("line0\n"));
    }

    #[test]
    fn cap_diff_stops_at_the_byte_ceiling_on_one_huge_line() {
        // A single line far past the byte ceiling must not be emitted just
        // because the line count is fine.
        let text = format!("{}\n", "x".repeat(10_000));

        let (kept, truncated) = cap_diff(&text, 1_000, 100);

        assert!(kept.is_empty());
        assert!(truncated);
    }

    #[test]
    fn cap_diff_leaves_content_that_fits_both_ceilings() {
        let text = "a\nb\nc\n";

        let (kept, truncated) = cap_diff(text, 10, 1_000);

        assert_eq!(kept, "a\nb\nc\n");
        assert!(!truncated);
    }

    #[test]
    fn cap_diff_normalizes_a_missing_trailing_newline() {
        // `lines()` drops the distinction, and the viewer renders line-wise, so
        // the output is deliberately newline-terminated either way.
        let (kept, truncated) = cap_diff("a\nb", 10, 1_000);
        assert_eq!(kept, "a\nb\n");
        assert!(!truncated);
    }
}
