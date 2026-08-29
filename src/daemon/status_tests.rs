use super::*;

#[cfg(unix)]
#[test]
fn a_non_unicode_endpoint_is_reported_as_unavailable() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let path = std::path::PathBuf::from(OsString::from_vec(vec![b'd', b'.', 0xff]));
    let metadata = DaemonMetadata::capture(&path);
    assert_eq!(
        metadata.endpoint,
        Err(StatusUnavailable {
            reason: StatusUnavailableReason::EndpointNotUnicode,
        })
    );
}

#[cfg(windows)]
#[test]
fn a_unicode_endpoint_is_preserved_exactly() {
    let path = std::path::Path::new(r"C:\nightcrow\한글.sock");
    let metadata = DaemonMetadata::capture(path);
    assert_eq!(metadata.endpoint, Ok(path.to_str().unwrap().to_owned()));
}
