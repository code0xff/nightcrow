use crate::ui::diff_pane::SplitRow;

/// Flush the pending removed/added runs into paired `SplitRow::Body` rows,
/// padding the shorter run with `None` cells, then clear both queues. Called
/// whenever a context line or hunk boundary breaks a change block.
pub(crate) fn flush_split_blocks(
    rows: &mut Vec<SplitRow>,
    hi: usize,
    removed: &mut Vec<usize>,
    added: &mut Vec<usize>,
) {
    let pairs = removed.len().max(added.len());
    for i in 0..pairs {
        rows.push(SplitRow::Body {
            left: removed.get(i).map(|&li| (hi, li)),
            right: added.get(i).map(|&li| (hi, li)),
        });
    }
    removed.clear();
    added.clear();
}

/// Pick the syntect syntax from a mutation-time file-extension key. Falls back
/// to plain text when the path is absent (test fixtures) or the extension is
/// unknown.
pub(crate) fn resolve_syntax_extension<'a>(
    ss: &'a syntect::parsing::SyntaxSet,
    extension: Option<&str>,
) -> &'a syntect::parsing::SyntaxReference {
    extension
        .and_then(|ext| ss.find_syntax_by_extension(ext))
        .unwrap_or_else(|| ss.find_syntax_plain_text())
}
