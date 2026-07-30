//! The wire contract, and the fixtures both halves of it share: this file pins
//! the shapes and names, `bounds` beside it pins what is refused.

use super::{LogLevel, PROTOCOL_VERSION, PluginCommand, PluginEvent, decode_command, encode_event};
use crate::backend::{PaneGeneration, PaneToken};

mod bounds;

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
        PluginCommand::WatchPane {
            v: PROTOCOL_VERSION,
            token: token(),
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
            r#"{{"event":"pane_idle","v":2,"token":"{TOKEN}","generation":2,"idle_ms":30000}}"#
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
        format!(r#"{{"cmd":"send_input","v":2,"token":"{TOKEN}","generation":2,"data":"go"}}"#)
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
fn only_a_pane_scoped_command_reports_an_identity() {
    for command in &every_command() {
        match command {
            PluginCommand::Log { .. } => {
                assert_eq!(command.token(), None);
                assert_eq!(command.generation(), None);
            }
            // Names a slot but not a spawn within it: the plugin is asking about
            // a pane the host has never described to it, so there is no
            // generation it could honestly claim.
            PluginCommand::WatchPane { .. } => {
                assert_eq!(command.token(), Some(&token()));
                assert_eq!(command.generation(), None);
            }
            _ => {
                assert_eq!(command.token(), Some(&token()));
                assert_eq!(command.generation(), Some(GENERATION));
            }
        }
    }
}

#[test]
fn a_watch_pane_commands_json_shape_is_the_wire_contract() {
    // Pinned literally, like the events above: a plugin that cannot be rebuilt
    // with this host writes this exact line, and there is no generation in it.
    let command = PluginCommand::WatchPane {
        v: PROTOCOL_VERSION,
        token: token(),
    };
    assert_eq!(
        serde_json::to_string(&command).unwrap(),
        format!(r#"{{"cmd":"watch_pane","v":2,"token":"{TOKEN}"}}"#)
    );
}
