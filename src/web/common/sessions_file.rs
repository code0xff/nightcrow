//! The session file's token list: parsing one field back into an expiry,
//! normalizing what a legacy or hand-edited file left behind, and serializing
//! the list the store is about to write.
//!
//! These are pure functions over a token list, kept apart from the store so
//! loading and writing apply the same rules without either holding a lock.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// On-disk marker for a token with no expiry. Not a number, so a build that
/// predates this simply drops the line as it does any other it cannot parse.
pub(super) const NEVER: &str = "never";

/// When a token stops being valid. `None` is "not on its own" — only logout or
/// a tightened lifetime ends it.
pub(super) type Expiry = Option<SystemTime>;

/// One login, kept in the order it was issued.
pub(super) struct Token {
    pub(super) value: String,
    pub(super) expiry: Expiry,
}

/// Read one file field back into an expiry, or `None` for a line to drop.
///
/// The numeric branch uses `checked_add` because `UNIX_EPOCH + Duration` panics
/// on overflow: a file naming an expiry no clock can hold would otherwise take
/// the server down on the way up, which is the one thing reading this file is
/// not allowed to do. A line like that is corrupt, so it is dropped like any
/// other unreadable one.
pub(super) fn parse_expiry(field: &str) -> Option<Expiry> {
    if field == NEVER {
        return Some(None);
    }
    let secs = field.parse::<u64>().ok()?;
    UNIX_EPOCH.checked_add(Duration::from_secs(secs)).map(Some)
}

/// Bring a loaded set into its canonical shape: one entry per token value,
/// carrying the earliest expiry, front to back in eviction order.
///
/// Files written before this order existed were serialized in hash-map order,
/// and a hand-edited one may name the same token twice. A duplicate keeps its
/// earliest expiry — `Some(earlier)` beats `Some(later)`, and both beat "never"
/// — because the earlier deadline is the safe reading of the two.
pub(super) fn normalize(tokens: &mut Vec<Token>) {
    // Stable, so tokens sharing an expiry keep the order the file gave them;
    // the tokens most likely to be stale end up first, where `trim` forgets
    // them, with the never-expiring ones last.
    tokens.sort_by(|a, b| match (a.expiry, b.expiry) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });
    // Sorted by expiry, so the first line naming a value is the one to keep.
    let mut seen = HashSet::new();
    tokens.retain(|token| seen.insert(token.value.clone()));
}

/// Bring loaded expiries down to what the configured lifetime allows.
///
/// Only ever lowers: a token issued under a longer lifetime should not
/// outlive a policy the operator has since tightened, and a token already
/// closer to running out keeps its earlier deadline. Reports whether anything
/// moved, which is what tells the caller the file no longer matches the tokens.
pub(super) fn clamp(tokens: &mut [Token], ttl: Option<Duration>, now: SystemTime) -> bool {
    let Some(ceiling) = ttl.and_then(|ttl| now.checked_add(ttl)) else {
        return false;
    };
    let mut shortened = false;
    for token in tokens.iter_mut() {
        if token.expiry.is_none_or(|at| at > ceiling) {
            token.expiry = Some(ceiling);
            shortened = true;
        }
    }
    shortened
}

/// Forget expired tokens, then the oldest logins beyond `cap`. Tokens are kept
/// in issue order, so the front of the list is what makes room.
pub(super) fn trim(tokens: &mut Vec<Token>, cap: usize, now: SystemTime) {
    sweep(tokens, now);
    let excess = tokens.len().saturating_sub(cap);
    tokens.drain(..excess);
}

/// Forget every token whose expiry has passed. One rule, one place: loading and
/// writing both apply it, so a token's being on disk means the same thing as its
/// being in memory.
pub(super) fn sweep(tokens: &mut Vec<Token>, now: SystemTime) {
    tokens.retain(|token| token.expiry.is_none_or(|at| at > now));
}

pub(super) fn serialize(tokens: &[Token]) -> String {
    let mut out = String::new();
    for token in tokens {
        out.push_str(&token.value);
        out.push('\t');
        match token.expiry {
            Some(at) => {
                let secs = at
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                out.push_str(&secs.to_string());
            }
            None => out.push_str(NEVER),
        }
        out.push('\n');
    }
    out
}
