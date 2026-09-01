use super::*;

#[cfg(unix)]
#[test]
fn a_non_unicode_endpoint_is_reported_as_unavailable() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let path = std::path::PathBuf::from(OsString::from_vec(vec![b'd', b'.', 0xff]));
    let metadata =
        DaemonMetadata::capture(&path, std::net::SocketAddr::from(([127, 0, 0, 1], 4321)));
    assert_eq!(
        metadata.attach_endpoint,
        Err(StatusUnavailable {
            reason: StatusUnavailableReason::EndpointNotUnicode,
        })
    );
    assert_eq!(metadata.web_endpoint, "http://127.0.0.1:4321/");
}

#[test]
fn an_ipv6_web_endpoint_is_rendered_with_url_brackets() {
    let metadata = DaemonMetadata::capture(
        std::path::Path::new("d.sock"),
        std::net::SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 4321)),
    );

    assert_eq!(metadata.web_endpoint, "http://[::1]:4321/");
}

#[cfg(windows)]
#[test]
fn a_unicode_endpoint_is_preserved_exactly() {
    let path = std::path::Path::new(r"C:\nightcrow\한글.sock");
    let metadata =
        DaemonMetadata::capture(path, std::net::SocketAddr::from(([127, 0, 0, 1], 4321)));
    assert_eq!(
        metadata.attach_endpoint,
        Ok(path.to_str().unwrap().to_owned())
    );
    assert_eq!(metadata.web_endpoint, "http://127.0.0.1:4321/");
}
