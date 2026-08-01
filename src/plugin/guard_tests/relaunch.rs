//! Rules 8-9: when a pane's process may be replaced, and with what — including
//! the pane that has no launch command to replace it with.

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
            command_line: format!("{LAUNCH} --resume abc123"),
        }
    );
}

#[test]
fn relaunching_a_pane_with_no_configured_command_is_refused() {
    // Its own reason, not one about the arguments: a bare shell is a pane whose
    // process cannot be put back at all, which is what a plugin given a pane on a
    // signal has to be told about the only pane it will ever have.
    let mut g = guard();
    let facts = PaneFacts {
        launch_command: None,
        ..exited_facts()
    };
    assert_eq!(
        g.judge(relaunch(&token(), &[]), Some(&facts), &[], Instant::now())
            .expect_err("refused"),
        Refused::NoLaunchCommand { pane: PANE }
    );
}

#[test]
fn a_relaunch_of_a_pane_with_no_command_is_refused_whatever_flags_are_allowed() {
    // The flag list is what normally decides a relaunch, so the refusal must not
    // be something a generous `allowed_resume_flags` can talk its way past.
    let mut g = guard();
    let facts = PaneFacts {
        launch_command: None,
        ..exited_facts()
    };
    assert_eq!(
        g.judge(
            relaunch(&token(), &["--resume", "abc123"]),
            Some(&facts),
            &flags(&["--resume"]),
            Instant::now()
        )
        .expect_err("refused"),
        Refused::NoLaunchCommand { pane: PANE }
    );
}

#[test]
fn a_refused_relaunch_of_a_bare_shell_costs_no_relaunch_budget() {
    // Nothing was done to the pane, and a plugin that keeps asking must not be
    // able to exhaust an allowance a legitimate pane would need.
    let mut g = guard();
    let t = token();
    let now = Instant::now();
    let limits = RateLimits::default();
    let facts = PaneFacts {
        launch_command: None,
        ..exited_facts()
    };
    for _ in 0..limits.max_relaunches_per_window + 1 {
        assert!(g.judge(relaunch(&t, &[]), Some(&facts), &[], now).is_err());
    }
    assert_eq!(g.budgets.spent(&t, RateAction::Relaunch, &limits, now), 0);
}
