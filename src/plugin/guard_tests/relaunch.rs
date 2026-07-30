//! Rules 8-9: when a pane's process may be replaced, and with what.

use super::*;

#[test]
fn relaunching_a_pane_whose_process_is_still_running_is_refused() {
    let mut g = guard();
    assert_eq!(
        g.judge(relaunch(&token(), &[]), Some(&facts()), &[], Instant::now())
            .expect_err("refused"),
        Refused::PaneStillRunning { pane: PANE }
    );
}

#[test]
fn a_relaunch_with_no_arguments_reproduces_the_original_command() {
    let mut g = guard();
    let approved = g
        .judge(
            relaunch(&token(), &[]),
            Some(&exited_facts()),
            &[],
            Instant::now(),
        )
        .expect("allowed");
    assert_eq!(
        approved,
        Approved::Relaunch {
            pane: PANE,
            resume_args: Vec::new(),
            command_line: LAUNCH.to_string()
        }
    );
}

#[test]
fn a_relaunch_flag_outside_the_allowed_list_is_refused() {
    let mut g = guard();
    assert!(matches!(
        g.judge(
            relaunch(&token(), &["--continue"]),
            Some(&exited_facts()),
            &flags(&["--resume"]),
            Instant::now()
        ),
        Err(Refused::ResumeArgsRejected { pane: PANE, .. })
    ));
}

#[test]
fn a_relaunch_argument_holding_shell_metacharacters_is_refused() {
    let mut g = guard();
    assert!(matches!(
        g.judge(
            relaunch(&token(), &["a; rm -rf /"]),
            Some(&exited_facts()),
            &[],
            Instant::now()
        ),
        Err(Refused::ResumeArgsRejected { pane: PANE, .. })
    ));
}

#[test]
fn a_relaunch_with_an_allowed_flag_carries_the_built_command_line() {
    let mut g = guard();
    let approved = g
        .judge(
            relaunch(&token(), &["--resume", "abc123"]),
            Some(&exited_facts()),
            &flags(&["--resume"]),
            Instant::now(),
        )
        .expect("allowed");
    assert_eq!(
        approved,
        Approved::Relaunch {
            pane: PANE,
            resume_args: vec!["--resume".to_string(), "abc123".to_string()],
            command_line: format!("{LAUNCH} '--resume' 'abc123'"),
        }
    );
}

#[test]
fn relaunching_a_pane_with_no_configured_command_is_refused() {
    let mut g = guard();
    let facts = PaneFacts {
        launch_command: None,
        ..exited_facts()
    };
    assert!(matches!(
        g.judge(relaunch(&token(), &[]), Some(&facts), &[], Instant::now()),
        Err(Refused::ResumeArgsRejected { pane: PANE, .. })
    ));
}
