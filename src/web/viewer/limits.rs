//! Ceilings on everything the viewer will serialize or hold open.
//!
//! Every route reads from a repository whose size the server does not control.
//! Without a ceiling, one request turns into an unbounded allocation. Truncation
//! is always reported -- [`Capped::truncated`] rides along into the DTO so the UI
//! can say "showing the first N".

/// Commits returned by one page of `/api/log`. Matches the TUI's
/// `commit_log_page_size` default.
pub const MAX_LOG_PAGE: usize = 100;
// `/api/log?skip=` deliberately has no ceiling. A ceiling here would look
// prudent and protect nothing: the skip feeds `Iterator::skip` on a revwalk, so
// a request walks at most `skip + page` or the whole history, whichever is
// smaller. An absurd skip costs what walking the repository costs and no more,
// while a ceiling would turn the deep end of a long history into a page the
// client can see exists and can never fetch.
/// Changed paths returned while drilling into one commit.
pub const MAX_COMMIT_FILES: usize = 2_000;
/// Entries returned for one directory level of `/api/tree`.
pub const MAX_TREE_ENTRIES: usize = 2_000;
/// Depth cap for the recursive `/api/tree/search` walk.
pub const MAX_TREE_SEARCH_DEPTH: usize = 64;
/// Entries one `/api/tree/search` walk may inspect before it stops.
pub const MAX_TREE_SEARCH_VISITS: usize = 100_000;
/// Matches returned by one `/api/tree/search` request.
pub const MAX_TREE_SEARCH_RESULTS: usize = 500;
/// Longest accepted `/api/tree/search` query.
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
/// Live connections the viewer's accept loop will hold. Each one costs a
/// thread, so without a ceiling anything that can reach the port can exhaust
/// the process.
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
}
