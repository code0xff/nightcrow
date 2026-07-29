use super::{ClientMessage, RepoSummary, ServerMessage, version};

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
        ClientMessage::ListRepos,
        ClientMessage::OpenRepo {
            path: "/w/repo".into(),
        },
        ClientMessage::CloseRepo { repo: "r1".into() },
        ClientMessage::ReorderRepos {
            order: vec!["r2".into(), "r1".into()],
        },
        ClientMessage::SetAccent { accent: 3 },
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
fn the_reported_version_is_this_build() {
    assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    assert!(!version().is_empty());
}
