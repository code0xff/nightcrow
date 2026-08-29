use super::*;

use crate::daemon::protocol::{
    DaemonStatus, RepositoryStatus, StatusUnavailable, StatusUnavailableReason, version,
};

fn status() -> DaemonStatus {
    DaemonStatus {
        pid: 42,
        version: version(),
        started_at_unix_ms: Ok(1_735_689_723_004),
        uptime_ms: 90_061_000,
        endpoint: Ok("custom.sock".into()),
        attached_clients: vec![9, 2],
        repositories: vec![
            RepositoryStatus {
                id: "b".into(),
                path: "/b".into(),
                pane_count: 0,
                panes: vec![],
            },
            RepositoryStatus {
                id: "a".into(),
                path: "/a".into(),
                pane_count: 2,
                panes: vec![8, 3],
            },
        ],
    }
}

#[test]
fn status_output_sorts_ids_and_repositories_and_names_empty_values() {
    let output = render_status(&status());
    assert!(output.contains("Status: running"));
    assert!(output.contains("Started at: 2025-01-01T00:02:03.004Z"));
    assert!(output.contains("Uptime: 1d 1h 1m 1s"));
    assert!(output.contains("Attached client IDs: 2, 9"));
    assert!(output.find("Repository: a") < output.find("Repository: b"));
    assert!(output.contains("    Pane IDs: 3, 8"));
    assert!(output.contains("    Pane IDs: (none)"));
}

#[test]
fn status_output_explains_unavailable_start_time() {
    let mut status = status();
    status.started_at_unix_ms = Err(StatusUnavailable {
        reason: StatusUnavailableReason::ClockBeforeUnixEpoch,
    });
    let output = render_status(&status);
    assert!(output.contains("Started at: unavailable (clock before Unix epoch)"));
}

#[test]
fn status_output_explains_empty_repository_set() {
    let mut status = status();
    status.repositories.clear();
    let output = render_status(&status);
    assert!(output.contains("Repositories: 0\n  (none)"));
}

#[test]
fn status_output_explains_unavailable_endpoint() {
    let mut status = status();
    status.endpoint = Err(StatusUnavailable {
        reason: StatusUnavailableReason::EndpointNotUnicode,
    });
    assert!(
        render_status(&status)
            .contains("Endpoint: unavailable (endpoint path is not valid Unicode)")
    );
}

#[test]
fn status_output_escapes_control_characters_and_preserves_unicode() {
    let mut status = status();
    status.endpoint = Ok("sock\u{1b}]0;evil\u{7}\n\u{9b}".into());
    status.repositories[0].id = "repo-한글\u{1b}".into();
    status.repositories[0].path = "C:\\work\n\u{80}".into();
    let output = render_status(&status);
    assert!(output.contains("repo-한글"));
    assert!(output.contains("\\u{001b}"));
    assert!(output.contains("\\u{009b}"));
    assert!(output.contains("\\n"));
    assert!(output.contains("\\u{0007}"));
    assert!(
        output
            .lines()
            .all(|line| { line.chars().all(|character| !character.is_control()) })
    );
}

#[test]
fn an_unrepresentable_start_time_is_rendered_without_panicking() {
    let mut status = status();
    status.started_at_unix_ms = Ok(u64::MAX);
    assert!(render_status(&status).contains("Started at: "));
}

#[test]
fn utc_start_time_format_handles_epoch_and_leap_year_boundaries() {
    assert_eq!(format_utc_millis(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(
        format_utc_millis(951_782_400_000),
        "2000-02-29T00:00:00.000Z"
    );
    assert_eq!(
        format_utc_millis(4_107_542_400_000),
        "2100-03-01T00:00:00.000Z"
    );
}
