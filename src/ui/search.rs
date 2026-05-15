/// Search-input string paired with its lowercased form. The two used to live
/// as independent `String` fields on `DiffSearch` and `StatusView`, which
/// meant every mutation site had to remember to re-lowercase. Bundling the
/// invariant into one type keeps callers honest: pushing or popping always
/// updates both halves in lockstep, and renderers/filters read the canonical
/// lower form through `lower()`.
#[derive(Default, Clone, Debug)]
pub struct SearchQuery {
    raw: String,
    lower: String,
}

impl SearchQuery {
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    pub fn lower(&self) -> &str {
        &self.lower
    }

    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    pub fn push(&mut self, ch: char) {
        self.raw.push(ch);
        self.lower = self.raw.to_lowercase();
    }

    pub fn pop(&mut self) {
        if self.raw.pop().is_some() {
            self.lower = self.raw.to_lowercase();
        }
    }

    pub fn clear(&mut self) {
        self.raw.clear();
        self.lower.clear();
    }

    /// Replace the query wholesale. Used by tests to seed an initial query
    /// without char-by-char push; runtime callers always go through push/pop.
    #[cfg(test)]
    pub fn set(&mut self, s: impl Into<String>) {
        self.raw = s.into();
        self.lower = self.raw.to_lowercase();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_keeps_lower_in_sync() {
        let mut q = SearchQuery::default();
        q.push('F');
        q.push('o');
        q.push('O');
        assert_eq!(q.as_str(), "FoO");
        assert_eq!(q.lower(), "foo");
        assert!(!q.is_empty());
    }

    #[test]
    fn pop_keeps_lower_in_sync_and_handles_empty() {
        let mut q = SearchQuery::default();
        q.set("AB");
        q.pop();
        assert_eq!(q.as_str(), "A");
        assert_eq!(q.lower(), "a");
        q.pop();
        assert!(q.is_empty());
        assert_eq!(q.lower(), "");
        // Pop on empty is a no-op; no panic, lower stays empty.
        q.pop();
        assert!(q.is_empty());
    }

    #[test]
    fn clear_resets_both_halves() {
        let mut q = SearchQuery::default();
        q.set("Bar");
        q.clear();
        assert_eq!(q.as_str(), "");
        assert_eq!(q.lower(), "");
    }

    #[test]
    fn set_lowercases() {
        let mut q = SearchQuery::default();
        q.set("MiXeD");
        assert_eq!(q.lower(), "mixed");
    }
}
