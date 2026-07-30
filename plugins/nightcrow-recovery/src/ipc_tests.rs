use super::*;
use std::sync::mpsc::channel;

const TOKEN: &str = "0123456789abcdef0123456789abcdef";
const OTHER_TOKEN: &str = "ffffffffffffffffffffffffffffffff";

fn message(token: &str, kind: SignalKind) -> IpcMessage {
    IpcMessage {
        token: token.to_string(),
        kind,
        payload: serde_json::json!({"session_id": "abc", "error_type": "rate_limit"}),
    }
}

fn line(token: &str) -> String {
    format!(r#"{{"v":1,"token":"{token}","kind":"stop_failure","payload":{{"a":1}}}}"#)
}

#[test]
fn a_message_round_trips_through_its_wire_line() {
    let sent = message(TOKEN, SignalKind::StopFailure);
    let parsed = parse_line(&encode(&sent).expect("encodable")).expect("parsable");
    assert_eq!(parsed, sent);
}

#[test]
fn a_statusline_message_round_trips_too() {
    let sent = IpcMessage {
        token: TOKEN.to_string(),
        kind: SignalKind::RateLimits,
        payload: serde_json::json!({"five_hour": {"resets_at": 1_767_225_600}}),
    };
    let parsed = parse_line(&encode(&sent).expect("encodable")).expect("parsable");
    assert_eq!(parsed, sent);
}

#[test]
fn a_line_that_is_not_a_json_object_is_refused() {
    for bad in ["", "   ", "not json", "[1,2]", "\"a\"", "null", "7"] {
        assert!(parse_line(bad).is_err(), "{bad:?} is not a message");
    }
}

#[test]
fn a_line_from_another_ipc_version_is_refused() {
    let bad = line(TOKEN).replace("\"v\":1", "\"v\":2");
    let err = parse_line(&bad)
        .expect_err("a version mismatch")
        .to_string();
    assert!(err.contains('2'), "{err}");
}

#[test]
fn a_line_with_no_version_is_refused() {
    let bad = line(TOKEN).replace("\"v\":1,", "");
    assert!(parse_line(&bad).is_err());
}

#[test]
fn a_token_that_no_host_would_mint_is_refused() {
    for token in ["", "abc def", "abc;rm", "../../etc/passwd", "tokén"] {
        let err = parse_line(&line(token));
        assert!(err.is_err(), "{token:?} is not a pane token");
    }
    let long = "a".repeat(MAX_IPC_LINE_BYTES.min(200));
    assert!(parse_line(&line(&long)).is_err(), "an over-long token");
}

#[test]
fn an_unknown_kind_is_refused_rather_than_ignored() {
    let bad = line(TOKEN).replace("stop_failure", "transcript");
    let err = parse_line(&bad).expect_err("an unknown kind").to_string();
    assert!(err.contains("transcript"), "{err}");
}

#[test]
fn a_payload_that_is_not_an_object_is_refused() {
    for payload in ["null", "\"text\"", "[1]", "3"] {
        let bad = line(TOKEN).replace(r#"{"a":1}"#, payload);
        assert!(parse_line(&bad).is_err(), "{payload} is not a payload");
    }
}

#[test]
fn a_line_over_the_length_limit_is_refused_before_it_is_parsed() {
    let bad = format!("{}{}", line(TOKEN), " ".repeat(MAX_IPC_LINE_BYTES));
    let err = parse_line(&bad).expect_err("over the limit").to_string();
    assert!(err.contains("limit"), "{err}");
}

#[test]
fn encoding_refuses_a_payload_too_large_to_send() {
    let huge = IpcMessage {
        token: TOKEN.to_string(),
        kind: SignalKind::StopFailure,
        payload: serde_json::json!({"error_message": "x".repeat(MAX_IPC_LINE_BYTES)}),
    };
    assert!(encode(&huge).is_err());
}

#[test]
fn a_message_becomes_a_signal_keyed_by_its_pane_token() {
    let (token, signal) = message(TOKEN, SignalKind::RateLimits).into_signal();
    assert_eq!(token, TOKEN);
    assert_eq!(signal.kind, SignalKind::RateLimits);
}

#[test]
fn the_socket_path_prefers_the_systems_runtime_directory() {
    let path = socket_path_from(
        Some(OsStr::new("/run/user/1000")),
        Some(OsStr::new("/home/x")),
    )
    .expect("a path");
    assert_eq!(
        path,
        PathBuf::from("/run/user/1000/nightcrow/recovery.sock")
    );
}

#[test]
fn the_socket_path_falls_back_to_the_home_directory() {
    for runtime in [None, Some(OsStr::new(""))] {
        let path = socket_path_from(runtime, Some(OsStr::new("/home/x"))).expect("a path");
        assert_eq!(path, PathBuf::from("/home/x/.nightcrow/run/recovery.sock"));
    }
}

#[test]
fn a_process_with_neither_variable_set_has_nowhere_to_put_the_socket() {
    let err = socket_path_from(None, None)
        .expect_err("nowhere to put it")
        .to_string();
    assert!(
        err.contains("XDG_RUNTIME_DIR") && err.contains("HOME"),
        "{err}"
    );
    assert!(socket_path_from(Some(OsStr::new("")), Some(OsStr::new(""))).is_err());
}

#[test]
fn a_helpers_line_reaches_the_plugin_and_names_its_pane() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("run").join("recovery.sock");
    let ipc = Ipc::bind(path.clone()).expect("a listener");
    let (tx, rx) = channel();
    ipc.serve(move |msg| tx.send(msg).is_ok()).expect("serving");

    send(&path, &message(TOKEN, SignalKind::StopFailure)).expect("sent");
    send(&path, &message(OTHER_TOKEN, SignalKind::RateLimits)).expect("sent");

    let first = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("first");
    let second = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("second");
    let tokens = [first.token.clone(), second.token.clone()];
    assert!(tokens.contains(&TOKEN.to_string()));
    assert!(tokens.contains(&OTHER_TOKEN.to_string()));
}

#[test]
fn a_malformed_line_is_dropped_without_ending_the_listener() {
    use std::io::Write as _;
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("recovery.sock");
    let ipc = Ipc::bind(path.clone()).expect("a listener");
    let (tx, rx) = channel();
    ipc.serve(move |msg| tx.send(msg).is_ok()).expect("serving");

    let mut stream = std::os::unix::net::UnixStream::connect(&path).expect("connected");
    stream.write_all(b"garbage\n").expect("wrote");
    drop(stream);

    send(&path, &message(TOKEN, SignalKind::StopFailure)).expect("sent");
    let received = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the good line");
    assert_eq!(received.token, TOKEN);
}

#[test]
fn the_socket_is_readable_only_by_its_owner_and_removed_on_exit() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("recovery.sock");
    {
        let ipc = Ipc::bind(path.clone()).expect("a listener");
        assert_eq!(ipc.path(), path.as_path());
        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, SOCKET_MODE);
        let dir_mode = fs::metadata(dir.path())
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, DIR_MODE);
    }
    assert!(!path.exists(), "a normal exit leaves no socket behind");
}

#[test]
fn a_socket_left_by_a_crashed_run_is_replaced() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("recovery.sock");
    fs::write(&path, b"stale").expect("a leftover file");
    let ipc = Ipc::bind(path.clone()).expect("bind over the leftover");
    assert!(ipc.path().exists());
}

#[test]
fn sending_to_a_plugin_that_is_not_running_fails_without_blocking() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("absent.sock");
    let err = send(&path, &message(TOKEN, SignalKind::StopFailure))
        .expect_err("nothing is listening")
        .to_string();
    assert!(err.contains("absent.sock"), "{err}");
}
