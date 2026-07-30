//! Rules 10-11 and `cancel`: how much a plugin may do, and what it costs.
//!
//! Every test here binds ONE token and reuses it. The budget is keyed by the
//! slot's token, so a fresh token per call would each get its own allowance and
//! the ceiling these tests exist to pin would never be reached.

use super::*;

fn limits(sends: u32, relaunches: u32) -> RateLimits {
    RateLimits {
        max_sends_per_window: sends,
        max_relaunches_per_window: relaunches,
        ..RateLimits::default()
    }
}

#[test]
fn sends_beyond_the_window_limit_are_refused() {
    let limits = limits(2, 1);
    let mut g = Guard::new(MIN_IDLE, limits);
    let now = Instant::now();
    let t = token();
    for _ in 0..limits.max_sends_per_window {
        assert!(g.judge(send(&t, "hi\n"), Some(&facts()), &[], now).is_ok());
    }
    assert_eq!(
        g.judge(send(&t, "hi\n"), Some(&facts()), &[], now)
            .expect_err("refused"),
        Refused::RateLimited {
            pane: PANE,
            action: RateAction::SendInput,
            limit: 2,
            window: limits.window
        }
    );
}

#[test]
fn relaunches_beyond_the_window_limit_are_refused() {
    let mut g = Guard::new(MIN_IDLE, limits(2, 1));
    let now = Instant::now();
    let t = token();
    assert!(
        g.judge(relaunch(&t, &[]), Some(&exited_facts()), &[], now)
            .is_ok()
    );
    assert!(matches!(
        g.judge(relaunch(&t, &[]), Some(&exited_facts()), &[], now),
        Err(Refused::RateLimited {
            action: RateAction::Relaunch,
            ..
        })
    ));
}

#[test]
fn a_relaunch_budget_is_not_refreshed_by_the_new_pane_id_it_produces() {
    // The loop this closes: a relaunch always mints a new `PaneId`, so an
    // id-keyed budget handed out a fresh allowance every time and a plugin
    // answering every exit with another relaunch never hit the ceiling. The
    // token is what a relaunch preserves, so it is what the ceiling counts.
    let mut g = Guard::new(MIN_IDLE, limits(1, 1));
    let now = Instant::now();
    let t = token();

    assert!(
        g.judge(relaunch(&t, &[]), Some(&exited_facts()), &[], now)
            .is_ok()
    );

    // The replacement process: same slot and so the same token, a different id,
    // and the next generation.
    let replacement = PaneFacts {
        pane: PANE + 1,
        generation: GENERATION + 1,
        ..exited_facts()
    };
    let mut cmd = relaunch(&t, &[]);
    if let PluginCommand::Relaunch { generation, .. } = &mut cmd {
        *generation = GENERATION + 1;
    }
    assert!(matches!(
        g.judge(cmd, Some(&replacement), &[], now),
        Err(Refused::RateLimited {
            action: RateAction::Relaunch,
            ..
        })
    ));
}

#[test]
fn two_panes_have_budgets_of_their_own() {
    // Two opted-in panes in one repository is a supported layout, so one pane
    // exhausting its allowance must not silence the other.
    let mut g = Guard::new(MIN_IDLE, limits(1, 1));
    let now = Instant::now();
    let a = token();
    let b = token();

    assert!(g.judge(send(&a, "hi\n"), Some(&facts()), &[], now).is_ok());
    assert!(g.judge(send(&a, "hi\n"), Some(&facts()), &[], now).is_err());
    assert!(g.judge(send(&b, "hi\n"), Some(&facts()), &[], now).is_ok());
}

#[test]
fn the_two_budgets_are_counted_separately() {
    let mut g = Guard::new(MIN_IDLE, limits(1, 1));
    let now = Instant::now();
    let t = token();
    assert!(g.judge(send(&t, "hi\n"), Some(&facts()), &[], now).is_ok());
    // A spent send budget must not block the relaunch the pane is owed.
    assert!(
        g.judge(relaunch(&t, &[]), Some(&exited_facts()), &[], now)
            .is_ok()
    );
}

#[test]
fn a_zero_limit_refuses_every_attempt() {
    let mut g = Guard::new(MIN_IDLE, limits(0, 0));
    assert!(matches!(
        g.judge(send(&token(), "hi\n"), Some(&facts()), &[], Instant::now()),
        Err(Refused::RateLimited { .. })
    ));
}

#[test]
fn a_status_command_spends_no_budget() {
    let mut g = Guard::new(MIN_IDLE, limits(1, 1));
    let now = Instant::now();
    let t = token();
    for _ in 0..5 {
        assert!(g.judge(status(&t), Some(&facts()), &[], now).is_ok());
    }
    assert!(g.judge(send(&t, "hi\n"), Some(&facts()), &[], now).is_ok());
}

#[test]
fn budget_spent_before_the_window_elapsed_is_available_again_after_it() {
    let limits = limits(1, 1);
    let mut g = Guard::new(MIN_IDLE, limits);
    let start = Instant::now();
    let t = token();
    assert!(
        g.judge(send(&t, "hi\n"), Some(&facts()), &[], start)
            .is_ok()
    );
    assert!(
        g.judge(send(&t, "hi\n"), Some(&facts()), &[], start + limits.window)
            .is_ok()
    );
}

#[test]
fn a_refused_command_does_not_consume_budget() {
    let limits = limits(1, 1);
    let mut g = Guard::new(MIN_IDLE, limits);
    let now = Instant::now();
    let t = token();
    let busy = PaneFacts {
        idle: Duration::ZERO,
        ..facts()
    };
    // Refusals are what a plugin produces by racing or by being wrong, and they
    // must not cost the pane the one legitimate attempt it is owed.
    for _ in 0..5 {
        assert!(g.judge(send(&t, "hi\n"), Some(&busy), &[], now).is_err());
    }
    assert_eq!(g.budgets.spent(&t, RateAction::SendInput, &limits, now), 0);
    assert!(g.judge(send(&t, "hi\n"), Some(&facts()), &[], now).is_ok());
}

#[test]
fn cancelling_a_pane_clears_its_spent_budget() {
    let limits = limits(1, 1);
    let mut g = Guard::new(MIN_IDLE, limits);
    let now = Instant::now();
    let t = token();
    assert!(g.judge(send(&t, "hi\n"), Some(&facts()), &[], now).is_ok());
    assert!(g.judge(send(&t, "hi\n"), Some(&facts()), &[], now).is_err());

    g.cancel(&t);

    assert_eq!(g.budgets.spent(&t, RateAction::SendInput, &limits, now), 0);
    assert!(g.judge(send(&t, "hi\n"), Some(&facts()), &[], now).is_ok());
}

#[test]
fn cancelling_a_pane_that_holds_no_state_is_harmless() {
    let mut g = guard();
    let t = token();
    g.cancel(&t);
    g.cancel(&t);
}
