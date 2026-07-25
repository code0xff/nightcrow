// strip_escape_sequences is re-exported by app.rs and reaches this module
// through `super::` (mod.rs pulls it in via `use super::strip_escape_sequences`).

#[test]
fn strip_escape_sequences_preserves_user_keystroke_after_bare_esc() {
    // ESC followed by an ordinary character was previously consumed; the
    // letter must now survive so user input echoed via PTY isn't lost.
    let out = super::strip_escape_sequences(b"\x1bA");
    assert_eq!(out, "A");
}

#[test]
fn strip_escape_sequences_drops_csi_and_ss3() {
    // CSI (cursor key), SS3 (alternate keypad), and charset designation
    // must all be stripped fully without leaving final bytes behind.
    let out = super::strip_escape_sequences(b"hi\x1b[31mRED\x1b[0m\x1bOA\x1b(Bend");
    assert_eq!(out, "hiREDend");
}

#[test]
fn strip_escape_sequences_keeps_text_after_malformed_ss3() {
    // ESC O followed by a control byte is not a valid SS3 sequence. The
    // old implementation unconditionally consumed two chars after ESC,
    // swallowing the newline (and any subsequent text relying on it).
    let out = super::strip_escape_sequences(b"\x1bO\nhello");
    assert_eq!(out, "\nhello");
}

#[test]
fn strip_escape_sequences_drops_osc_until_terminator() {
    let bel = super::strip_escape_sequences(b"\x1b]0;title\x07ok");
    assert_eq!(bel, "ok");
    let st = super::strip_escape_sequences(b"\x1b]0;title\x1b\\ok");
    assert_eq!(st, "ok");
}

#[test]
fn strip_escape_sequences_preserves_backspace_and_del() {
    // BS (0x08) and DEL (0x7f) survive stripping so `buffer_prompt_input`
    // can replay them as `buf.pop()` instead of logging chars the user
    // already corrected.
    let out = super::strip_escape_sequences(b"ab\x7fcd\x08e");
    assert_eq!(out, "ab\x7fcd\x08e");
}
