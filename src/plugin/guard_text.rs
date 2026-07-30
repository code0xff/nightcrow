//! Bounding and sanitising the text a plugin sends.

/// Longest log message the host will keep from a plugin.
///
/// A log line is unbounded text from an untrusted process heading for a file
/// that rotates on size, so truncating is what keeps a chatty plugin from being
/// a way to fill a disk.
pub const MAX_LOG_MESSAGE_BYTES: usize = 2 * 1024;

/// Whether a character may not appear in input typed into a pane.
///
/// Input stands in for a human at a keyboard, and a keyboard produces `\r`,
/// `\n` and `\t` and no other control characters. Anything else is an escape
/// sequence: a plugin that could send ESC could reprogram the emulator, move the
/// cursor over what the user is reading, or drive the running CLI's own key
/// bindings.
pub(super) fn is_forbidden_control(c: char) -> bool {
    c.is_control() && !matches!(c, '\r' | '\n' | '\t')
}

/// Cut a message to [`MAX_LOG_MESSAGE_BYTES`], on a character boundary.
pub(super) fn truncate_message(mut message: String) -> String {
    if message.len() <= MAX_LOG_MESSAGE_BYTES {
        return message;
    }
    let mut end = MAX_LOG_MESSAGE_BYTES;
    while end > 0 && !message.is_char_boundary(end) {
        end -= 1;
    }
    message.truncate(end);
    message
}
