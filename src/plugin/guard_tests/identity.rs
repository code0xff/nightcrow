//! Rules 1-4: what a command must prove before any of its own rules run.

use super::*;

#[test]
fn a_log_command_is_always_allowed_without_a_pane() {
    let mut g = guard();
    let cmd = PluginCommand::Log {
        v: PROTOCOL_VERSION,
        level: LogLevel::Info,
        message: "watching".to_string(),
    };
    assert_eq!(
        g.judge(cmd, None, &[], Instant::now()).expect("allowed"),
        Approved::Log {
            level: LogLevel::Info,
            message: "watching".to_string()
        }
    );
}

#[test]
fn an_over_long_log_message_is_truncated_rather_than_refused() {
    let mut g = guard();
    let cmd = PluginCommand::Log {
        v: PROTOCOL_VERSION,
        level: LogLevel::Warn,
        // Multi-byte so a naive truncation would split a character.
        message: "가".repeat(MAX_LOG_MESSAGE_BYTES),
    };
    let Approved::Log { message, .. } = g.judge(cmd, None, &[], Instant::now()).expect("allowed")
    else {
        panic!("expected a log approval");
    };
    assert!(message.len() <= MAX_LOG_MESSAGE_BYTES);
    assert!(message.chars().all(|c| c == '가'));
}

#[test]
fn a_command_for_a_token_with_no_live_pane_is_refused() {
    let mut g = guard();
    let t = token();
    assert_eq!(
        g.judge(send(&t, "hi\n"), None, &[], Instant::now())
            .expect_err("refused"),
        Refused::UnknownPane { token: t }
    );
}

#[test]
fn a_command_for_a_pane_that_did_not_opt_in_is_refused() {
    let mut g = guard();
    let t = token();
    let facts = PaneFacts {
        opted_in: false,
        ..facts()
    };
    assert_eq!(
        g.judge(send(&t, "hi\n"), Some(&facts), &[], Instant::now())
            .expect_err("refused"),
        Refused::NotOptedIn {
            pane: PANE,
            token: t
        }
    );
}

#[test]
fn a_status_command_for_a_pane_that_did_not_opt_in_is_refused_too() {
    let mut g = guard();
    let facts = PaneFacts {
        opted_in: false,
        ..facts()
    };
    assert!(matches!(
        g.judge(status(&token()), Some(&facts), &[], Instant::now()),
        Err(Refused::NotOptedIn { .. })
    ));
}

#[test]
fn a_stale_generation_command_is_refused() {
    let mut g = guard();
    let facts = PaneFacts {
        generation: GENERATION + 1,
        ..facts()
    };
    assert_eq!(
        g.judge(send(&token(), "hi\n"), Some(&facts), &[], Instant::now())
            .expect_err("refused"),
        Refused::StaleGeneration {
            pane: PANE,
            claimed: GENERATION,
            current: GENERATION + 1
        }
    );
}

#[test]
fn a_stale_generation_is_refused_for_a_relaunch_too() {
    let mut g = guard();
    let facts = PaneFacts {
        generation: GENERATION + 3,
        ..exited_facts()
    };
    assert!(matches!(
        g.judge(relaunch(&token(), &[]), Some(&facts), &[], Instant::now()),
        Err(Refused::StaleGeneration { .. })
    ));
}

#[test]
fn a_status_command_is_approved_with_its_fields_intact() {
    let mut g = guard();
    assert_eq!(
        g.judge(status(&token()), Some(&facts()), &[], Instant::now())
            .expect("allowed"),
        Approved::Status {
            pane: PANE,
            state: "waiting".to_string(),
            detail: None,
            deadline_epoch: Some(42),
            attempt: 2
        }
    );
}
