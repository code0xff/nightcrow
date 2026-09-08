pub(super) fn location_suffix(candidate: &str) -> Option<usize> {
    for (offset, character) in candidate.char_indices() {
        if character == ':' && is_line_suffix(&candidate[offset..]) {
            return Some(offset);
        }
    }
    let hash = candidate.rfind("#L")?;
    is_hash_suffix(&candidate[hash..]).then_some(hash)
}

fn is_line_suffix(suffix: &str) -> bool {
    let bytes = suffix.as_bytes();
    let mut at = 0;
    if bytes.get(at) != Some(&b':') {
        return false;
    }
    at = digits_end(bytes, at + 1);
    if at == 1 {
        return false;
    }
    if bytes.get(at) == Some(&b':') {
        let next = digits_end(bytes, at + 1);
        if next == at + 1 {
            return false;
        }
        at = next;
    }
    if bytes.get(at) == Some(&b'-') {
        at = digits_end(bytes, at + 1);
        if at == suffix.len() {
            return false;
        }
        if bytes.get(at) == Some(&b':') {
            let next = digits_end(bytes, at + 1);
            if next == at + 1 {
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
    let mut at = digits_end(bytes, 2);
    if at == 2 {
        return false;
    }
    if bytes.get(at) == Some(&b'C') {
        at = digits_end(bytes, at + 1);
        if at == suffix.len() {
            return false;
        }
    }
    if bytes.get(at) == Some(&b'-') {
        at += 1;
        if bytes.get(at) == Some(&b'L') {
            at += 1;
        }
        let line_end = digits_end(bytes, at);
        if line_end == at {
            return false;
        }
        at = line_end;
        if bytes.get(at) == Some(&b'C') {
            at = digits_end(bytes, at + 1);
            if at == line_end + 1 {
                return false;
            }
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
