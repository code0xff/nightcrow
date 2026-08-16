//! What `decode_command` refuses, and where each limit sits.
//!
//! Split from the shape tests beside it: those pin the wire's *names*, these pin
//! the wire's *edges*. A plugin is untrusted input on a stream with no length
//! prefix, so every one of these is a bound the host would otherwise not have.

use super::{GENERATION, TOKEN, token};
use crate::plugin::protocol::{
    MAX_INPUT_BYTES, MAX_LINE_BYTES, PROTOCOL_VERSION, PluginCommand, decode_command, is_blank_line,
};

#[test]
fn a_command_from_another_protocol_version_is_refused_and_both_versions_named() {
    let line = format!(
        r#"{{"cmd":"log","v":{},"level":"info","message":"hi"}}"#,
        PROTOCOL_VERSION + 1
    );
    let err = decode_command(&line).expect_err("refused");
    let message = err.to_string();
    assert!(
        message.contains(&(PROTOCOL_VERSION + 1).to_string())
            && message.contains(&PROTOCOL_VERSION.to_string()),
        "message names only one version: {message}"
    );
}

#[test]
fn a_line_over_the_length_limit_is_refused_before_it_is_parsed() {
    // Refused on length alone, so nothing here depends on the payload being
    // valid JSON — an unbounded line must not become an unbounded parse.
    let line = "x".repeat(MAX_LINE_BYTES + 1);
    let message = decode_command(&line).expect_err("refused").to_string();
    assert!(message.contains(&MAX_LINE_BYTES.to_string()), "{message}");
}

#[test]
fn a_line_at_the_length_limit_is_parsed() {
    let padding = "p".repeat(MAX_INPUT_BYTES);
    let line = format!(r#"{{"cmd":"log","v":3,"level":"debug","message":"{padding}"}}"#);
    assert!(line.len() <= MAX_LINE_BYTES);
    assert!(decode_command(&line).is_ok());
}

#[test]
fn an_unknown_cmd_is_refused_rather_than_guessed() {
    let message = decode_command(r#"{"cmd":"drop_everything","v":3}"#)
        .expect_err("refused")
        .to_string();
    assert!(message.contains("drop_everything"), "{message}");
    assert!(message.contains("cmd"), "{message}");
}

#[test]
fn a_command_missing_a_required_field_is_refused() {
    assert!(decode_command(r#"{"cmd":"send_input","v":3}"#).is_err());
}

#[test]
fn a_watch_pane_command_from_the_previous_protocol_version_is_refused() {
    // The command did not exist at version 1, so a build that speaks 1 cannot
    // have sent it — accepting one would be honouring a claim about a contract
    // the sender never agreed to.
    let line = format!(r#"{{"cmd":"watch_pane","v":1,"token":"{TOKEN}"}}"#);
    assert!(decode_command(&line).is_err());
}

#[test]
fn send_input_data_over_the_input_limit_is_refused() {
    let command = PluginCommand::SendInput {
        v: PROTOCOL_VERSION,
        token: token(),
        generation: GENERATION,
        data: "y".repeat(MAX_INPUT_BYTES + 1),
    };
    let message = command.validate().expect_err("refused").to_string();
    assert!(message.contains(&MAX_INPUT_BYTES.to_string()), "{message}");

    let line = serde_json::to_string(&command).unwrap();
    assert!(
        line.len() <= MAX_LINE_BYTES,
        "the length cap would mask this"
    );
    assert!(decode_command(&line).is_err());
}

#[test]
fn send_input_data_at_the_input_limit_is_accepted() {
    let command = PluginCommand::SendInput {
        v: PROTOCOL_VERSION,
        token: token(),
        generation: GENERATION,
        data: "y".repeat(MAX_INPUT_BYTES),
    };
    assert!(command.validate().is_ok());
}

#[test]
fn a_blank_line_is_recognised_as_blank_and_carries_no_command() {
    for line in ["", "   ", "\t", "\r\n"] {
        assert!(is_blank_line(line), "not seen as blank: {line:?}");
        assert!(decode_command(line).is_err());
    }
    assert!(!is_blank_line(
        r#"{"cmd":"log","v":3,"level":"info","message":"x"}"#
    ));
}
