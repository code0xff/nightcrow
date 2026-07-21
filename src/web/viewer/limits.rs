//! Ceilings on everything the viewer will serialize or hold open.
//!
//! Every route reads from a repository whose size the server does not control:
//! a commit log can be a million entries, a generated file a gigabyte, a
//! `node_modules` directory a hundred thousand children. Without a ceiling, one
//! request turns into an unbounded allocation and a response no browser can
//! render. Each limit below is the point past which more data stops being
//! useful to a human reading a diff.
//!
//! Truncation is always *reported* — [`Capped::truncated`] rides along into the
//! DTO so the UI can say "showing the first N", never silently imply it showed
//! everything.

/// Commits returned by one page of `/api/log`.
pub const MAX_LOG_PAGE: usize = 200;
/// Entries returned for one directory level of `/api/tree`.
pub const MAX_TREE_ENTRIES: usize = 2_000;
/// Depth cap for the recursive `/api/tree/search` walk. Matches the TUI tree's
/// default `max_depth` (`config.rs`) so both surfaces reach the same files.
pub const MAX_TREE_SEARCH_DEPTH: usize = 64;
/// Entries one `/api/tree/search` walk may inspect before it stops and reports
/// the listing as incomplete. Bounds filesystem work per request (the TUI has no
/// equivalent cap because it walks once, in-process, for a single local user).
pub const MAX_TREE_SEARCH_VISITS: usize = 100_000;
/// Matches returned by one `/api/tree/search` request.
pub const MAX_TREE_SEARCH_RESULTS: usize = 500;
/// Longest accepted `/api/tree/search` query. A filename substring never needs
/// to be large; anything past this is rejected at the boundary rather than
/// lowercased and matched against every basename.
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

/// Cut `text` to at most `max_bytes`, never splitting a UTF-8 character.
///
/// Returns the kept prefix and whether anything was dropped. Byte-slicing a
/// `String` at an arbitrary index panics mid-character, so the cut is walked
/// back to the nearest boundary — a multi-byte character straddling the limit
/// is dropped whole rather than emitted as a broken fragment.
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
