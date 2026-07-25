use super::*;

fn parse(text: &str) -> RequestHead {
    parse_request_head(text).unwrap()
}

#[test]
fn parses_method_and_strips_query_from_path() {
    let head = parse("GET /login?token=abc&x=1 HTTP/1.1\r\nHost: localhost\r\n\r\n");
    assert_eq!(head.method, "GET");
    assert_eq!(
        head.path, "/login",
        "the query string must be stripped for routing"
    );
}

#[test]
fn query_params_are_captured_and_decoded() {
    let head = parse("GET /api/diff?repo=r1&path=src%2Fmain.rs HTTP/1.1\r\n\r\n");
    assert_eq!(head.path, "/api/diff");
    assert_eq!(head.query, "repo=r1&path=src%2Fmain.rs");
    assert_eq!(head.query_param("repo").as_deref(), Some("r1"));
    assert_eq!(head.query_param("path").as_deref(), Some("src/main.rs"));
    assert_eq!(head.query_param("missing"), None);
}

#[test]
fn query_param_is_absent_without_a_query_string() {
    let head = parse("GET /api/repos HTTP/1.1\r\n\r\n");
    assert_eq!(head.query, "");
    assert_eq!(head.query_param("repo"), None);
}

#[test]
fn query_param_takes_the_first_of_a_repeated_name() {
    // A checked-then-reused parameter must not be overridable by appending
    // a second copy.
    let head = parse("GET /api/file?path=ok.txt&path=..%2F..%2Fetc%2Fpasswd HTTP/1.1\r\n\r\n");
    assert_eq!(head.query_param("path").as_deref(), Some("ok.txt"));
}

#[test]
fn query_param_decodes_only_once() {
    // `%252e` is a literal `%2e` after one decode. Decoding twice would
    // turn it into `.` and let `..` past a traversal check.
    let head = parse("GET /api/file?path=%252e%252e%2Fsecret HTTP/1.1\r\n\r\n");
    assert_eq!(head.query_param("path").as_deref(), Some("%2e%2e/secret"));
}

#[test]
fn header_lookup_is_case_insensitive() {
    let head = parse("GET / HTTP/1.1\r\nContent-Length: 5\r\nX-Foo: Bar\r\n\r\n");
    assert_eq!(head.header("content-length"), Some("5"));
    assert_eq!(head.header("X-FOO"), Some("Bar"));
    assert_eq!(head.content_length, 5);
}

#[test]
fn parses_cookies() {
    let head = parse("GET / HTTP/1.1\r\nCookie: a=1; nightcrow_session=tok123; b=2\r\n\r\n");
    assert_eq!(head.cookie("nightcrow_session"), Some("tok123"));
    assert_eq!(head.cookie("a"), Some("1"));
    assert_eq!(head.cookie("missing"), None);
}

#[test]
fn detects_websocket_upgrade() {
    let ws = parse("GET /ws HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n");
    assert!(ws.is_websocket_upgrade());
    let plain = parse("GET / HTTP/1.1\r\nConnection: keep-alive\r\n\r\n");
    assert!(!plain.is_websocket_upgrade());
}

#[test]
fn detects_websocket_upgrade_with_combined_connection_header() {
    // Browsers often send "Connection: keep-alive, Upgrade".
    let ws =
        parse("GET /ws HTTP/1.1\r\nUpgrade: WebSocket\r\nConnection: keep-alive, Upgrade\r\n\r\n");
    assert!(ws.is_websocket_upgrade());
}

#[test]
fn rejects_malformed_request_line() {
    assert!(parse_request_head("garbage\r\n\r\n").is_err());
    assert!(parse_request_head("").is_err());
}

#[test]
fn parses_url_encoded_form_body() {
    let fields = parse_form("password=p%40ss+word&remember=on");
    assert_eq!(form_field(&fields, "password"), Some("p@ss word"));
    assert_eq!(form_field(&fields, "remember"), Some("on"));
    assert_eq!(form_field(&fields, "missing"), None);
}

#[test]
fn response_has_content_length_and_no_store() {
    let bytes = response("200 OK", "text/plain", &[("X-A", "b")], b"hello");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(text.contains("Content-Length: 5\r\n"));
    assert!(text.contains("Cache-Control: no-store\r\n"));
    assert!(text.contains("X-A: b\r\n"));
    assert!(text.ends_with("\r\n\r\nhello"));
}

#[test]
fn redirect_carries_location_and_extra_headers() {
    let bytes = redirect("/", &[("Set-Cookie", "s=1")]);
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.starts_with("HTTP/1.1 303 See Other\r\n"));
    assert!(text.contains("Location: /\r\n"));
    assert!(text.contains("Set-Cookie: s=1\r\n"));
}
