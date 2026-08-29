use super::*;
use clap::Parser;

use crate::cli::{Cli, Commands};
use crate::daemon::protocol::{DaemonStatus, RepositoryStatus, ServerMessage, version};

#[test]
fn status_subcommand_accepts_an_optional_socket_override() {
    let cli = Cli::try_parse_from(["nightcrow", "status", "--socket", "custom.sock"]).unwrap();
    match cli.command {
        Some(Commands::Status { socket }) => {
            assert_eq!(socket.unwrap(), std::path::PathBuf::from("custom.sock"));
        }
        _ => panic!("expected status command"),
    }
}

#[test]
fn status_subcommand_defaults_to_the_standard_socket() {
    let cli = Cli::try_parse_from(["nightcrow", "status"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Status { socket: None })
    ));
}

#[test]
fn explicit_socket_override_does_not_evaluate_the_default_socket() {
    let expected = std::path::PathBuf::from("custom.sock");
    let actual = resolve_socket_path(Some(expected.clone()), || {
        anyhow::bail!("default socket path should not be evaluated")
    })
    .unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn a_missing_daemon_is_distinguished_from_a_protocol_failure() {
    let dir = tempfile::TempDir::new().unwrap();
    let error = query_status(&dir.path().join("missing.sock")).unwrap_err();
    assert!(
        error.to_string().contains("daemon unavailable"),
        "{error:#}"
    );
    assert!(error.to_string().contains("nightcrow -d"), "{error:#}");
}

#[test]
fn a_version_mismatch_is_reported_as_a_version_error() {
    let status = DaemonStatus {
        pid: 1,
        version: "old".into(),
        started_at_unix_ms: Ok(0),
        uptime_ms: 0,
        endpoint: Ok("sock".into()),
        repositories: vec![],
        attached_clients: vec![],
    };
    let error = decode_status(ServerMessage::Status { status }).unwrap_err();
    assert!(error.to_string().contains("version mismatch"), "{error:#}");
}

#[test]
fn an_unexpected_server_message_is_reported_as_a_protocol_error() {
    let error = decode_status(ServerMessage::Hello {
        version: version(),
        client: 1,
    })
    .unwrap_err();
    assert!(error.to_string().contains("protocol error"), "{error:#}");
}

#[test]
fn malformed_status_facts_are_rejected_before_rendering() {
    let status = DaemonStatus {
        pid: 1,
        version: version(),
        started_at_unix_ms: Ok(0),
        uptime_ms: 0,
        endpoint: Ok("sock".into()),
        repositories: vec![RepositoryStatus {
            id: "repo".into(),
            path: "/repo".into(),
            pane_count: 2,
            panes: vec![1],
        }],
        attached_clients: vec![],
    };
    let error = validate_status(&status).unwrap_err();
    assert!(error.to_string().contains("malformed status"), "{error:#}");
}
