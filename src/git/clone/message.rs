//! Turn a failed `git clone`'s stderr into one line for the user.
//!
//! A remote controls that stream — `remote:` sidebands are printed verbatim —
//! so it cannot be collected unbounded, and git closes a failure with an advice
//! block whose last line names no cause at all.

/// Most stderr kept from a failing clone. Only the tail is wanted anyway: the
/// reason git gave up is at the end.
pub(super) const MAX_STDERR_BYTES: usize = 64 * 1024;

/// Prefixes that mark a line as a diagnostic rather than progress or advice.
/// git writes these as literals rather than translated text, so matching them
/// survives a localized git; `ERROR:` is what a forge prefixes its own
/// server-side refusals with.
const DIAGNOSTIC_PREFIXES: [&str; 4] = ["fatal:", "error:", "ERROR:", "remote:"];

/// Longest supporting line carried along with the diagnostic. Transfer progress
/// arrives as a single carriage-return ribbon thousands of columns wide, and a
/// one-line message is not the place for it.
const MAX_SUPPORTING_CHARS: usize = 200;

/// Read `reader` to EOF, keeping only the last [`MAX_STDERR_BYTES`].
///
/// Draining to the end matters as much as the cap: stopping early would fill
/// the pipe and block the child forever.
pub(super) fn tail_of<R: std::io::Read>(mut reader: R) -> String {
    let mut kept: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            // A signal can cut a read short; giving up there would drop the
            // rest of git's message and close the pipe under a child that is
            // still writing.
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
            Ok(n) => {
                kept.extend_from_slice(&chunk[..n]);
                if kept.len() > MAX_STDERR_BYTES {
                    kept.drain(..kept.len() - MAX_STDERR_BYTES);
                }
            }
        }
    }
    String::from_utf8_lossy(&kept).into_owned()
}

/// The actionable part of a failed clone's stderr, or `None` if there is none.
///
/// The last line is the wrong pick: an unreachable remote ends with a wrapped
/// piece of advice (`Please make sure you have the correct access rights …`)
/// instead of the reason. The reason is the last diagnostic line — and usually
/// the line before it as well, because `fatal: Could not read from remote
/// repository.` is only a wrapper around what the transport actually said
/// ("Repository not found.", "Permission denied (publickey)."). Both are kept
/// and joined; everything after them is dropped.
pub(super) fn actionable(stderr: &str) -> Option<String> {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let Some(last) = lines.iter().rposition(|line| is_diagnostic(line)) else {
        // Nothing announced itself as a diagnostic — a transport speaking for
        // itself (`ssh: Could not resolve hostname …`) or a git whose wording
        // this does not know. Its last line still beats saying nothing.
        return lines.last().map(|line| (*line).to_string());
    };
    let mut kept = Vec::with_capacity(2);
    if last > 0 {
        let supporting = lines[last - 1];
        if supporting.chars().count() <= MAX_SUPPORTING_CHARS {
            kept.push(supporting);
        }
    }
    kept.push(lines[last]);
    Some(kept.join(" "))
}

fn is_diagnostic(line: &str) -> bool {
    DIAGNOSTIC_PREFIXES
        .iter()
        .any(|prefix| line.starts_with(prefix))
}

#[cfg(test)]
mod tests;
