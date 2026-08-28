pub(crate) fn strip_escape_sequences(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => consume_escape_sequence(&mut chars),
            // \r, \n, and the line-editing controls (BS, DEL) are forwarded
            // so `buffer_prompt_input` can flush on newlines and pop on
            // backspace; every other control byte is dropped.
            '\r' | '\n' | '\x08' | '\x7f' => result.push(ch),
            c if !c.is_control() => result.push(c),
            _ => {}
        }
    }
    result
}

/// Consume the body of an ESC-introduced control sequence. Called with the
/// leading ESC already taken; advances `chars` past the sequence's terminator
/// (or leaves the iterator alone for a bare ESC).
fn consume_escape_sequence(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match chars.peek().copied() {
        Some('[') => {
            chars.next();
            consume_csi(chars);
        }
        Some(']') => {
            chars.next();
            consume_osc(chars);
        }
        Some('O') => {
            chars.next();
            consume_ss3(chars);
        }
        Some('(') | Some(')') | Some('*') | Some('+') | Some('-') | Some('.') | Some('/')
        | Some('#') => {
            // Charset designators / DEC private 2-byte escapes: skip both bytes.
            chars.next();
            chars.next();
        }
        _ => {
            // Drop the bare ESC and let the next iteration process whatever
            // follows as ordinary input. Consuming an extra byte here would
            // silently swallow user keystrokes that happened to land right
            // after a stray Esc.
        }
    }
}

/// CSI: consume parameter/intermediate bytes (0x20–0x3f), stop at the final
/// byte (0x40–0x7e). Break early on a control char and leave it in the
/// iterator: eating it here would silently drop a `\n`/`\r` the outer pass
/// needs to flush the prompt buffer. DEL (0x7f) is treated per ECMA-48 as a
/// no-op inside the sequence.
fn consume_csi(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while let Some(&c) = chars.peek() {
        if c < '\x20' {
            return;
        }
        chars.next();
        if c == '\x7f' {
            continue;
        }
        if ('\x40'..='\x7e').contains(&c) {
            return;
        }
    }
}

/// OSC: skip until BEL (0x07) or ST (ESC \).
fn consume_osc(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    loop {
        match chars.next() {
            None | Some('\x07') => break,
            Some('\x1b') if chars.peek() == Some(&'\\') => {
                chars.next();
                break;
            }
            _ => {}
        }
    }
}

/// SS3: ESC O <final>. Consume the next char only when it looks like a valid
/// SS3 final byte (0x40–0x7e) — a malformed `ESC O <x>` sequence used to
/// swallow the following ordinary char.
fn consume_ss3(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    if let Some(&next) = chars.peek()
        && ('\x40'..='\x7e').contains(&next)
    {
        chars.next();
    }
}
