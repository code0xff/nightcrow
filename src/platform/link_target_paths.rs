pub(super) fn is_file_url_prefix(text: &str) -> bool {
    text.as_bytes()
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"file://"))
}

pub(super) fn is_drive_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

pub(super) fn is_forbidden_namespace(path: &str) -> bool {
    path.starts_with(r"\\")
        || path.starts_with("//")
        || path.starts_with(r"\\?\")
        || path.starts_with(r"\\.\")
        || path.starts_with("//?/")
        || path.starts_with("//./")
}
