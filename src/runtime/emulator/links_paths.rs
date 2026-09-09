pub(super) fn location_suffix(candidate: &str) -> Option<usize> {
    for (offset, character) in candidate.char_indices() {
        if character == ':' && is_line_suffix(&candidate[offset..]) {
            return Some(offset);
        }
    }
    let hash = candidate.rfind("#L")?;
    is_hash_suffix(&candidate[hash..]).then_some(hash)
}

pub(super) fn has_location_marker(candidate: &str) -> bool {
    candidate.contains("#L")
        || candidate.char_indices().any(|(offset, character)| {
            character == ':'
                && candidate
                    .as_bytes()
                    .get(offset + 1)
                    .is_some_and(u8::is_ascii_digit)
        })
}

fn is_line_suffix(suffix: &str) -> bool {
    let bytes = suffix.as_bytes();
    let mut at = 0;
    if bytes.get(at) != Some(&b':') {
        return false;
    }
    let next = digits_end(bytes, at + 1);
    if next == at + 1 {
        return false;
    }
    at = next;
    if bytes.get(at) == Some(&b':') {
        let before = at + 1;
        let next = digits_end(bytes, before);
        if next == before {
            return false;
        }
        at = next;
    }
    if bytes.get(at) == Some(&b'-') {
        let before = at + 1;
        let next = digits_end(bytes, before);
        if next == before {
            return false;
        }
        at = next;
        if bytes.get(at) == Some(&b':') {
            let before = at + 1;
            let next = digits_end(bytes, before);
            if next == before {
                return false;
            }
            at = next;
        }
    }
    at == bytes.len()
}

fn is_hash_suffix(suffix: &str) -> bool {
    let bytes = suffix.as_bytes();
    if !suffix.starts_with("#L") {
        return false;
    }
    let mut at = 2;
    let next = digits_end(bytes, at);
    if next == at {
        return false;
    }
    at = next;
    if bytes.get(at) == Some(&b'C') {
        let before = at + 1;
        let next = digits_end(bytes, before);
        if next == before {
            return false;
        }
        at = next;
    }
    if bytes.get(at) == Some(&b'-') {
        at += 1;
        if bytes.get(at) == Some(&b'L') {
            at += 1;
        }
        let line_start = at;
        let line_end = digits_end(bytes, line_start);
        if line_end == line_start {
            return false;
        }
        at = line_end;
        if bytes.get(at) == Some(&b'C') {
            let before = at + 1;
            let next = digits_end(bytes, before);
            if next == before {
                return false;
            }
            at = next;
        }
    }
    at == bytes.len()
}

fn digits_end(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    at
}
