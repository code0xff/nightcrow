//! What the store accepts, checked before it reaches SQL.

use super::{MAX_BODY_BYTES, MAX_QUERY_TERMS, MAX_TAG_BYTES, MAX_TAGS};
use anyhow::{Context, Result, ensure};
use std::path::Path;

/// The text as it will be stored: trimmed, non-empty, within the cap. Over the
/// cap is refused rather than cut — a note silently missing its end reads as a
/// complete one.
pub(super) fn checked_body(body: &str) -> Result<&str> {
    let body = body.trim();
    ensure!(!body.is_empty(), "the text is empty");
    ensure!(
        body.len() <= MAX_BODY_BYTES,
        "the text is {} bytes, over the {MAX_BODY_BYTES}-byte limit; shorten it or split it",
        body.len()
    );
    Ok(body)
}

/// Tags as one space-separated, lowercased string — the form both the row and
/// the search index keep.
pub(super) fn checked_tags(tags: &[String]) -> Result<String> {
    ensure!(
        tags.len() <= MAX_TAGS,
        "{} tags given, at most {MAX_TAGS} allowed",
        tags.len()
    );
    let mut out: Vec<String> = Vec::with_capacity(tags.len());
    for tag in tags {
        let tag = tag.trim().to_lowercase();
        ensure!(
            !tag.is_empty() && tag.len() <= MAX_TAG_BYTES,
            "a tag must be 1 to {MAX_TAG_BYTES} bytes: {tag:?}"
        );
        ensure!(
            tag.chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.')),
            "a tag may hold letters, digits, '-', '_' and '.': {tag:?}"
        );
        if !out.contains(&tag) {
            out.push(tag);
        }
    }
    Ok(out.join(" "))
}

/// An FTS5 expression for free text, or `None` when it holds no word.
///
/// Each word becomes a quoted prefix term, joined with OR. Quoted, because FTS5
/// has a query syntax of its own and text an agent typed is not written in it —
/// a stray `-` or `:` would be a syntax error. A prefix, so a word matches with
/// an ending attached to it, which is how Korean and most inflected languages
/// write the same word twice.
pub(super) fn match_expression(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(MAX_QUERY_TERMS)
        .map(|word| format!("\"{word}\"*"))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

/// Create the store's directory, readable by its owner alone where the
/// platform has such a mode: notes are whatever the agents chose to write down.
/// A directory that already exists is left as it is — it may be one the person
/// pointed the store at, and its mode is theirs to set.
pub(super) fn create_private_dir(dir: &Path) -> Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("cannot restrict {}", dir.display()))?;
    }
    Ok(())
}
