use crate::ui::SearchQuery;

#[derive(Default)]
pub struct DiffSearch {
    pub active: bool,
    pub query: SearchQuery,
    pub(crate) matches: Vec<usize>,
    pub(crate) cursor: usize,
}

impl DiffSearch {
    pub fn is_visible(&self) -> bool {
        self.active || !self.query.is_empty()
    }

    pub fn has_query(&self) -> bool {
        !self.query.is_empty()
    }

    pub fn current_match(&self) -> Option<usize> {
        self.matches.get(self.cursor).copied()
    }

    pub fn is_match(&self, flat_idx: usize) -> bool {
        // `matches` is built by `recompute_matches` in flat_idx-ascending
        // order, so binary_search is always sound here.
        self.matches.binary_search(&flat_idx).is_ok()
    }

    pub(crate) fn start(&mut self) {
        self.active = true;
    }

    pub(crate) fn confirm(&mut self) {
        if self.query.is_empty() {
            self.clear();
        } else {
            self.active = false;
        }
    }

    pub fn clear(&mut self) {
        self.active = false;
        self.query.clear();
        self.matches.clear();
        self.cursor = 0;
    }

    pub(crate) fn push_char(&mut self, ch: char) {
        self.query.push(ch);
    }

    pub(crate) fn pop_char(&mut self) {
        self.query.pop();
    }

    pub(crate) fn next(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        // Defensive clamp: `recompute_matches(false)` re-anchors `cursor` to
        // the nearest match, but a stale cursor can otherwise survive here
        // through code paths that mutate `matches` without re-anchoring.
        if self.cursor >= self.matches.len() {
            self.cursor = 0;
        } else {
            self.cursor = (self.cursor + 1) % self.matches.len();
        }
        self.current_match()
    }

    pub(crate) fn prev(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        if self.cursor == 0 || self.cursor >= self.matches.len() {
            self.cursor = self.matches.len() - 1;
        } else {
            self.cursor -= 1;
        }
        self.current_match()
    }
}

/// Index of the match in `matches` whose flat row is closest to `scroll`.
/// Ties prefer the smaller flat row (the one already on or above the cursor)
/// so a content refresh during reading never jumps the "current match" past
/// where the user is looking. `matches` must be sorted ascending and
/// non-empty.
pub(crate) fn nearest_match_index(matches: &[usize], scroll: usize) -> usize {
    debug_assert!(!matches.is_empty());
    match matches.binary_search(&scroll) {
        Ok(i) => i,
        Err(i) => {
            if i == 0 {
                0
            } else if i == matches.len() {
                matches.len() - 1
            } else {
                let prev = matches[i - 1];
                let next = matches[i];
                if scroll - prev <= next - scroll {
                    i - 1
                } else {
                    i
                }
            }
        }
    }
}
