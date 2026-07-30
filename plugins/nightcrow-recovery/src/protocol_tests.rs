use super::*;

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

fn opened_line() -> String {
    format!(
        r#"{{"event":"pane_opened","v":1,"token":"{TOKEN}","generation":2,"title":null,"command":"claude","cwd":"/w/repo"}}"#
    )
}

#[test]
fn a_host_event_is_parsed_from_its_wire_shape() {
    let event = decode_event(&opened_line()).expect("a well-formed event");
    assert_eq!(
        event,
        PluginEvent::PaneOpened {
            v: PROTOCOL_VERSION,
            token: TOKEN.to_string(),
            generation: 2,
            title: None,
            command: Some("claude".to_string()),
            cwd: "/w/repo".to_string(),
        }
    );
    assert_eq!(event.token(), Some(&TOKEN.to_string()));
    assert_eq!(event.generation(), Some(2));
}

#[test]
fn an_event_from_another_protocol_version_is_refused_and_both_versions_named() {
    let line = opened_line().replace("\"v\":1", "\"v\":2");
    let err = decode_event(&line)
        .expect_err("a version this build cannot speak")
        .to_string();
    assert!(err.contains('2') && err.contains('1'), "{err}");
}

#[test]
fn an_unknown_event_is_refused_rather_than_guessed() {
    let line = r#"{"event":"pane_teleported","v":1,"token":"abc","generation":1}"#;
    assert!(decode_event(line).is_err());
}

#[test]
fn an_event_missing_a_required_field_is_refused() {
    let line = r#"{"event":"pane_idle","v":1,"token":"abc","generation":1}"#;
    assert!(decode_event(line).is_err(), "idle_ms is required");
}

#[test]
fn a_line_over_the_length_limit_is_refused_before_it_is_parsed() {
    let line = "x".repeat(MAX_LINE_BYTES + 1);
    let err = decode_event(&line).expect_err("over the limit").to_string();
    assert!(err.contains("limit"), "{err}");
}

#[test]
fn a_shutdown_names_no_pane() {
    let event = decode_event(r#"{"event":"shutdown","v":1}"#).expect("a shutdown");
    assert_eq!(event.token(), None);
    assert_eq!(event.generation(), None);
}

#[test]
fn a_command_is_encoded_as_one_line_tagged_with_its_name() {
    let line = encode_command(&PluginCommand::Relaunch {
        v: PROTOCOL_VERSION,
        token: TOKEN.to_string(),
        generation: 2,
        resume_args: vec!["--resume".to_string(), "abc".to_string()],
    })
    .expect("encodable");
    assert!(!line.contains('\n'));
    assert!(line.contains("\"cmd\":\"relaunch\""), "{line}");
    assert!(line.contains("\"v\":1"), "{line}");
}

#[test]
fn text_holding_a_newline_is_escaped_rather_than_splitting_the_line() {
    let line = encode_command(&PluginCommand::SendInput {
        v: PROTOCOL_VERSION,
        token: TOKEN.to_string(),
        generation: 1,
        data: "first\nsecond".to_string(),
    })
    .expect("encodable");
    assert!(!line.contains('\n'));
    assert!(line.contains("first\\nsecond"), "{line}");
}

#[test]
fn a_log_levels_json_shape_is_lowercase() {
    let line = encode_command(&log(LogLevel::Warn, "careful")).expect("encodable");
    assert!(line.contains("\"level\":\"warn\""), "{line}");
}

#[test]
fn the_input_limit_matches_what_the_host_accepts() {
    // Both sides cap typed input; a plugin that believed a larger number would
    // spend attempts on requests the host refuses.
    assert_eq!(MAX_INPUT_BYTES, 8 * 1024);
    assert_eq!(MAX_LINE_BYTES, 64 * 1024);
    assert_eq!(PANE_TOKEN_ENV, "NIGHTCROW_PANE_TOKEN");
}

/// Pull `pub const NAME: ty = <literal>;` out of the host's source.
///
/// Reading the host's file is the only way to compare: a plugin is a separate
/// build that deliberately does not link nightcrow, so the two copies of the
/// contract cannot share a constant. Re-asserting our own literals — which the
/// test above does — proves nothing about the host, and that is exactly how two
/// copies drift apart in silence.
fn host_const(source: &str, name: &str) -> String {
    let needle = format!("pub const {name}");
    let line = source
        .lines()
        .find(|l| l.trim_start().starts_with(&needle))
        .unwrap_or_else(|| panic!("host no longer declares {name}"));
    line.split_once('=')
        .expect("a const declaration has a value")
        .1
        .trim()
        .trim_end_matches(';')
        .trim()
        .to_string()
}

#[test]
fn the_hosts_copy_of_the_contract_still_says_the_same_thing() {
    // Located from this crate's own manifest, and this crate is never published,
    // so the path is always valid where the test can run at all.
    let host = concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/plugin/protocol.rs");
    let source = std::fs::read_to_string(host)
        .unwrap_or_else(|e| panic!("cannot read the host's protocol at {host}: {e}"));

    assert_eq!(host_const(&source, "PROTOCOL_VERSION"), "1");
    assert_eq!(host_const(&source, "MAX_LINE_BYTES"), "64 * 1024");
    assert_eq!(host_const(&source, "MAX_INPUT_BYTES"), "8 * 1024");
    assert_eq!(
        host_const(&source, "PROTOCOL_VERSION")
            .parse::<u32>()
            .expect("a version is a number"),
        PROTOCOL_VERSION,
        "the host and this plugin claim different protocol versions"
    );
}
