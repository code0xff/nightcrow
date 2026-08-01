//! Tab completion for the repo dialog's path field.
//!
//! One `read_dir` per Tab press, against the single directory the buffer names.
//! Directories only: the dialog opens a repo and a file can never be one.
//! The dialog is not a shell, so only what `confirm_repo_input` itself accepts
//! is understood here: `~`, `..`, and cwd-relative paths. No `$VAR`, no globs.

use std::path::{MAIN_SEPARATOR, Path};

/// What one Tab press does to the buffer.
pub(crate) struct PathCompletion {
    /// The buffer after completion — unchanged when nothing matched.
    pub buf: String,
    /// Directory names to offer. Empty when the completion was unambiguous,
    /// when nothing matched, or when the buffer grew.
    pub candidates: Vec<String>,
}

/// Whether `c` ends a path component. `\` counts on Windows only — on Unix it
/// is a legal filename character.
pub(crate) fn is_sep(c: char) -> bool {
    c == '/' || (cfg!(windows) && c == '\\')
}

/// Split a dialog buffer into the directory text (up to and including the last
/// separator) and the trailing component being completed.
pub(crate) fn split_dir(buf: &str) -> (&str, &str) {
    match buf.char_indices().rfind(|(_, c)| is_sep(*c)) {
        Some((i, c)) => (&buf[..i + c.len_utf8()], &buf[i + c.len_utf8()..]),
        None => ("", buf),
    }
}

/// Immediate sub-directory names of `dir`, sorted. Directories only.
pub(crate) fn read_dir_names(dir: &Path, show_hidden: bool) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read
        .flatten()
        .filter(is_dir_entry)
        // A non-UTF-8 name cannot round-trip through the `String` buffer, so
        // completing to it would produce a path that no longer opens.
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| show_hidden || !n.starts_with('.'))
        .collect();
    names.sort_unstable();
    names
}

/// Complete the last component of `buf` against the directory the rest of it
/// names. The user's own text is never rewritten: a leading `~` or a relative
/// path is expanded for reading only, and the returned buffer keeps the typed
/// prefix with just the completed component appended.
pub(crate) fn complete_dir_path(buf: &str) -> PathCompletion {
    let unchanged = || PathCompletion {
        buf: buf.to_string(),
        candidates: Vec::new(),
    };

    let (dir_text, frag) = split_dir(buf);
    // Reuse whatever separator is already in the buffer so a path typed with
    // `/` on Windows does not come back with a `\` spliced into it.
    let sep = dir_text.chars().next_back().unwrap_or(MAIN_SEPARATOR);

    // `dir_text` always ends with a separator (or is empty). Strip it so
    // `read_dir` sees the directory itself — a trailing-separator form fails
    // on Windows verbatim paths (`\\?\C:\...\`). Root paths keep their
    // separator: stripping `/` or `C:\` would lose the root.
    let dir_str = if dir_text.is_empty() {
        "."
    } else {
        let trimmed = dir_text.trim_end_matches(['/', '\\']);
        if trimmed.is_empty() || trimmed.ends_with(':') {
            dir_text
        } else {
            trimmed
        }
    };
    let dir = crate::platform::paths::expand_tilde(dir_str);
    let names = read_dir_names(&dir, frag.starts_with('.'));

    let mut matches: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|n| n.starts_with(frag))
        .collect();
    if matches.is_empty() {
        // Retry ignoring case only once the exact prefix has found nothing: on
        // case-insensitive filesystems (macOS, Windows) the typed casing rarely
        // matches the disk, and on Linux this can never shadow an exact match.
        let lower = frag.to_lowercase();
        matches = names
            .iter()
            .map(String::as_str)
            .filter(|n| n.to_lowercase().starts_with(&lower))
            .collect();
    }
    // `read_dir_names` already sorted, and filtering preserves order.

    match matches.len() {
        0 => unchanged(),
        // Append the separator too, so Tab can be pressed again to descend.
        1 => PathCompletion {
            buf: format!("{dir_text}{}{sep}", matches[0]),
            candidates: Vec::new(),
        },
        _ => {
            let common = longest_common_prefix(&matches);
            // Extending also corrects casing, so this fires whenever the shared
            // prefix reads differently from what was typed, not only when it is
            // longer.
            let extended = common != frag;
            // Listing and extending are independent. While typing can still be
            // narrowed by an extension the list would be noise — except on a
            // directory boundary, where an empty fragment means "what is in
            // here?" and a silent extension answers nothing.
            let candidates = if extended && !frag.is_empty() {
                Vec::new()
            } else {
                matches.iter().map(|n| n.to_string()).collect()
            };
            PathCompletion {
                buf: if extended {
                    format!("{dir_text}{common}")
                } else {
                    buf.to_string()
                },
                candidates,
            }
        }
    }
}

/// `file_type` comes free with the directory read on most platforms; only a
/// symlink costs the extra stat to see what it points at. Symlinked checkouts
/// are common enough that reporting them as non-directories would hide real
/// repos, so unlike the in-repo tree navigator this one follows them.
fn is_dir_entry(entry: &std::fs::DirEntry) -> bool {
    match entry.file_type() {
        Ok(t) if t.is_symlink() => entry.path().is_dir(),
        Ok(t) => t.is_dir(),
        // An entry that vanished or cannot be stat'd is dropped rather than
        // failing the whole listing.
        Err(_) => false,
    }
}

/// Longest prefix shared by every name, truncated on a char boundary so a
/// multi-byte name never splits mid-codepoint.
fn longest_common_prefix(names: &[&str]) -> String {
    let Some((first, rest)) = names.split_first() else {
        return String::new();
    };
    let mut end = first.len();
    for name in rest {
        let shared = first
            .char_indices()
            .zip(name.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map_or(0, |((i, a), _)| i + a.len_utf8());
        end = end.min(shared);
    }
    first[..end].to_string()
}

#[cfg(test)]
mod tests;
