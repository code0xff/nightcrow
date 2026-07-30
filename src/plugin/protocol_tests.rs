use super::{
    LogLevel, MAX_INPUT_BYTES, MAX_LINE_BYTES, PROTOCOL_VERSION, PluginCommand, PluginEvent,
    decode_command, encode_event, is_blank_line,
};
use crate::backend::{PaneGeneration, PaneToken};

const TOKEN: &str = "0123456789abcdef0123456789abcdef";
const GENERATION: PaneGeneration = 2;

fn token() -> PaneToken {
    serde_json::from_str(&format!("\"{TOKEN}\"")).expect("a token is a JSON string")
}

fn every_event() -> Vec<PluginEvent> {
    vec![
        PluginEvent::PaneOpened {
            v: PROTOCOL_VERSION,
            token: token(),
            generation: GENERATION,
            title: Some("worker".into()),
            command: None,
            cwd: "/w/repo".into(),
        },
        PluginEvent::PaneOutput {
            v: PROTOCOL_VERSION,
            token: token(),
            generation: GENERATION,
            text: "first\nsecond".into(),
        },
        PluginEvent::PaneIdle {
            v: PROTOCOL_VERSION,
            token: token(),
            generation: GENERATION,
            idle_ms: 30_000,
        },
        PluginEvent::PaneExited {
            v: PROTOCOL_VERSION,
            token: token(),
            generation: GENERATION,
        },
        PluginEvent::PaneClosed {
            v: PROTOCOL_VERSION,
            token: token(),
            generation: GENERATION,
        },
        PluginEvent::UserInput {
            v: PROTOCOL_VERSION,
            token: token(),
            generation: GENERATION,
        },
        PluginEvent::Shutdown {
            v: PROTOCOL_VERSION,
        },
    ]
}

fn every_command() -> Vec<PluginCommand> {
    vec![
        PluginCommand::SendInput {
            v: PROTOCOL_VERSION,
            token: token(),
            generation: GENERATION,
            data: "continue\r".into(),
        },
        PluginCommand::Relaunch {
            v: PROTOCOL_VERSION,
            token: token(),
            generation: GENERATION,
            resume_args: vec!["--resume".into(), "last".into()],
        },
        PluginCommand::Status {
            v: PROTOCOL_VERSION,
            token: token(),
            generation: GENERATION,
            state: "waiting".into(),
            detail: None,
            deadline_epoch: Some(1_700_000_000),
            attempt: 1,
        },
        PluginCommand::Log {
            v: PROTOCOL_VERSION,
            level: LogLevel::Warn,
            message: "retrying".into(),
        },
    ]
}

#[test]
fn encoding_an_event_yields_one_line_tagged_with_its_name() {
    let expected = [
        "pane_opened",
        "pane_output",
        "pane_idle",
        "pane_exited",
        "pane_closed",
        "user_input",
        "shutdown",
    ];
    for (event, tag) in every_event().iter().zip(expected) {
        let line = encode_event(event).expect("encodes");
        assert!(!line.contains('\n'), "{tag} split the frame: {line}");
        assert!(
            line.starts_with(&format!(r#"{{"event":"{tag}""#)),
            "{tag} is not the tag of {line}"
        );
    }
}

#[test]
fn text_holding_a_newline_is_escaped_rather_than_splitting_the_line() {
    let event = PluginEvent::PaneOutput {
        v: PROTOCOL_VERSION,
        token: token(),
        generation: GENERATION,
        text: "first\nsecond".into(),
    };
    let line = encode_event(&event).expect("encodes");
    assert!(!line.contains('\n'));
    assert!(line.contains(r"first\nsecond"));
}

#[test]
fn every_command_survives_the_round_trip() {
    for command in &every_command() {
        let line = serde_json::to_string(command).expect("encodes");
        assert_eq!(&decode_command(&line).expect("decodes"), command);
    }
}

#[test]
fn an_events_json_shape_is_the_wire_contract() {
    // Pinned literally: this is what an independently built plugin parses, so
    // an accidental rename must fail here rather than in the field.
    let event = PluginEvent::PaneIdle {
        v: PROTOCOL_VERSION,
        token: token(),
        generation: GENERATION,
        idle_ms: 30_000,
    };
    assert_eq!(
        encode_event(&event).unwrap(),
        format!(
            r#"{{"event":"pane_idle","v":1,"token":"{TOKEN}","generation":2,"idle_ms":30000}}"#
        )
    );
}

#[test]
fn a_commands_json_shape_is_the_wire_contract() {
    let command = PluginCommand::SendInput {
        v: PROTOCOL_VERSION,
        token: token(),
        generation: GENERATION,
        data: "go".into(),
    };
    assert_eq!(
        serde_json::to_string(&command).unwrap(),
        format!(r#"{{"cmd":"send_input","v":1,"token":"{TOKEN}","generation":2,"data":"go"}}"#)
    );
}

#[test]
fn a_log_levels_json_shape_is_lowercase() {
    assert_eq!(
        serde_json::to_string(&LogLevel::Error).unwrap(),
        r#""error""#
    );
    assert_eq!(serde_json::to_string(&LogLevel::Warn).unwrap(), r#""warn""#);
    assert_eq!(serde_json::to_string(&LogLevel::Info).unwrap(), r#""info""#);
    assert_eq!(
        serde_json::to_string(&LogLevel::Debug).unwrap(),
        r#""debug""#
    );
}

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
    let line = format!(r#"{{"cmd":"log","v":1,"level":"debug","message":"{padding}"}}"#);
    assert!(line.len() <= MAX_LINE_BYTES);
    assert!(decode_command(&line).is_ok());
}

#[test]
fn an_unknown_cmd_is_refused_rather_than_guessed() {
    let message = decode_command(r#"{"cmd":"drop_everything","v":1}"#)
        .expect_err("refused")
        .to_string();
    assert!(message.contains("drop_everything"), "{message}");
    assert!(message.contains("cmd"), "{message}");
}

#[test]
fn a_command_missing_a_required_field_is_refused() {
    assert!(decode_command(r#"{"cmd":"send_input","v":1}"#).is_err());
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
        r#"{"cmd":"log","v":1,"level":"info","message":"x"}"#
    ));
}

#[test]
fn only_a_pane_scoped_command_reports_an_identity() {
    for command in &every_command() {
        match command {
            PluginCommand::Log { .. } => {
                assert_eq!(command.token(), None);
                assert_eq!(command.generation(), None);
            }
            _ => {
                assert_eq!(command.token(), Some(&token()));
                assert_eq!(command.generation(), Some(GENERATION));
            }
        }
    }
}
