use super::{
    ClientMessage, DaemonStatus, RepoSummary, RepositoryStatus, ServerMessage, TerminalOutput,
    version,
};

fn round_trip_client(message: &ClientMessage) -> ClientMessage {
    let json = serde_json::to_string(message).expect("encodes");
    serde_json::from_str(&json).expect("decodes")
}

fn round_trip_server(message: &ServerMessage) -> ServerMessage {
    let json = serde_json::to_string(message).expect("encodes");
    serde_json::from_str(&json).expect("decodes")
}

#[test]
fn every_client_message_survives_the_round_trip() {
    let messages = vec![
        ClientMessage::Hello {
            version: "0.1.0".into(),
        },
        ClientMessage::Status {},
        ClientMessage::ListRepos,
        ClientMessage::OpenRepo {
            path: "/w/repo".into(),
        },
        ClientMessage::CloseRepo { repo: "r1".into() },
        ClientMessage::ReorderRepos {
            order: vec!["r2".into(), "r1".into()],
        },
        ClientMessage::SetAccent { accent: 3 },
        ClientMessage::ReloadConfig,
    ];
    for message in &messages {
        assert_eq!(&round_trip_client(message), message);
    }
}

#[test]
fn a_repository_set_without_an_accent_is_refused_rather_than_read_as_yellow() {
    // What an older daemon sends. Its version string can match this build's, so
    // the handshake lets it through and this is the only thing left to catch it;
    // defaulting would paint the session a colour nobody chose.
    let json = r#"{"type":"repos","repos":[],"active":null}"#;

    assert!(serde_json::from_str::<ServerMessage>(json).is_err());
}

#[test]
fn every_server_message_survives_the_round_trip() {
    let messages = vec![
        ServerMessage::Hello {
            version: "0.1.0".into(),
            client: 3,
        },
        ServerMessage::Status {
            status: DaemonStatus {
                pid: 42,
                version: "0.1.0".into(),
                started_at_unix_ms: Ok(123),
                uptime_ms: 7,
                web_endpoint: "http://127.0.0.1:4321/".into(),
                attach_endpoint: Ok("/tmp/nightcrow.sock".into()),
                repositories: vec![RepositoryStatus {
                    id: "r1".into(),
                    path: "/w/repo".into(),
                    panes: vec![3],
                    pane_count: 1,
                }],
                attached_clients: vec![9],
            },
        },
        ServerMessage::Repos {
            repos: vec![RepoSummary {
                id: "r1".into(),
                path: "/w/repo".into(),
            }],
            active: Some("r1".into()),
            accent: 3,
        },
        ServerMessage::Error {
            message: "no such directory".into(),
        },
        ServerMessage::Reloaded {
            summary: "config reloaded: 1 plugin across 2 open projects".into(),
        },
    ];
    for message in &messages {
        assert_eq!(&round_trip_server(message), message);
    }
}

#[test]
fn a_message_with_no_fields_still_carries_its_tag() {
    // `ListRepos` has no payload, so the tag is the entire message. An encoding
    // that dropped it would decode as whatever variant came first.
    let json = serde_json::to_string(&ClientMessage::ListRepos).unwrap();
    assert_eq!(json, r#"{"type":"list_repos"}"#);
}

#[test]
fn an_unknown_message_type_is_refused_rather_than_guessed() {
    let err = serde_json::from_str::<ClientMessage>(r#"{"type":"drop_everything"}"#);
    assert!(err.is_err());
}

#[test]
fn a_message_missing_a_required_field_is_refused() {
    // Reaching the socket is authorization, not a promise of well-formed
    // input: a client bug must be a refused request, not a panic.
    assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"open_repo"}"#).is_err());
}

#[test]
fn status_rejects_unknown_request_fields_and_missing_response_fields() {
    assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"status","pid":1}"#).is_err());
    let missing_uptime = r#"{
        "type":"status","status":{"pid":1,"version":"0.1.0",
        "started_at_unix_ms":{"Ok":1},"web_endpoint":"http://127.0.0.1:4321/",
        "attach_endpoint":"/tmp/d.sock",
        "repositories":[],"attached_clients":[]}}
    "#;
    assert!(serde_json::from_str::<ServerMessage>(missing_uptime).is_err());
}

#[test]
fn an_old_status_shape_is_rejected_even_when_the_build_version_matches() {
    // Status fields are required by the current protocol, so a same-version
    // daemon from before this field split fails closed rather than losing the
    // distinction between its web and attach endpoints.
    let old_shape = format!(
        r#"{{
        "type":"status","status":{{"pid":1,"version":"{}",
        "started_at_unix_ms":{{"Ok":1}},"uptime_ms":0,"endpoint":{{"Ok":"/tmp/d.sock"}},
        "repositories":[],"attached_clients":[]}}
    }}"#,
        version()
    );
    assert!(serde_json::from_str::<ServerMessage>(&old_shape).is_err());
}

#[test]
fn an_unavailable_endpoint_reason_survives_the_status_round_trip() {
    let status = DaemonStatus {
        pid: 1,
        version: version(),
        started_at_unix_ms: Ok(0),
        uptime_ms: 0,
        web_endpoint: "http://127.0.0.1:4321/".into(),
        attach_endpoint: Err(super::StatusUnavailable {
            reason: super::StatusUnavailableReason::EndpointNotUnicode,
        }),
        repositories: vec![],
        attached_clients: vec![],
    };
    assert_eq!(
        round_trip_server(&ServerMessage::Status {
            status: status.clone()
        }),
        ServerMessage::Status { status }
    );
}

#[test]
fn existing_wire_messages_keep_their_shape() {
    assert_eq!(
        serde_json::to_string(&ClientMessage::Hello {
            version: "0.1.0".into()
        })
        .unwrap(),
        r#"{"type":"hello","version":"0.1.0"}"#
    );
    assert!(matches!(
        serde_json::from_str::<ClientMessage>(r#"{"type":"list_repos"}"#),
        Ok(ClientMessage::ListRepos)
    ));
}

/// A reload carries nothing on the way out: the file on the daemon's disk is the
/// request. An encoding that admitted a payload would be a client reconfiguring
/// the session from something it made up.
#[test]
fn a_reload_request_carries_no_configuration() {
    let json = serde_json::to_string(&ClientMessage::ReloadConfig).unwrap();
    assert_eq!(json, r#"{"type":"reload_config"}"#);
}

#[test]
fn the_reported_version_is_this_build() {
    assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    assert!(!version().is_empty());
}

#[test]
fn terminal_output_accepts_empty_and_maximum_length_repository_ids() {
    for repo in [String::new(), "r".repeat(u8::MAX as usize)] {
        let output = TerminalOutput {
            repo,
            pane: 0x0102_0304,
            data: vec![0xff, 0x00],
        };

        let encoded = output.encode().expect("repository id fits");
        assert_eq!(TerminalOutput::decode(&encoded), Some(output));
    }
}

#[test]
fn terminal_output_rejects_a_repository_id_over_the_wire_limit() {
    let output = TerminalOutput {
        repo: "r".repeat(u8::MAX as usize + 1),
        pane: 1,
        data: Vec::new(),
    };

    let error = output.encode().expect_err("repository id is too long");
    assert!(error.to_string().contains("256 bytes"), "{error:#}");
}

#[test]
fn terminal_output_rejects_truncated_or_non_utf8_headers() {
    assert_eq!(TerminalOutput::decode(&[]), None);
    assert_eq!(TerminalOutput::decode(&[0, 1, 2, 3]), None);
    assert_eq!(TerminalOutput::decode(&[2, b'r']), None);
    assert_eq!(TerminalOutput::decode(&[1, 0xff, 0, 0, 0, 0]), None);
}
