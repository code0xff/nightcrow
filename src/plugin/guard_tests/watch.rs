//! Rule 12: when a plugin may be given a pane nobody handed it.

use super::*;

#[test]
fn a_watch_request_for_a_live_unclaimed_pane_is_approved() {
    let mut g = guard();
    assert_eq!(
        g.judge(
            watch(&token()),
            Some(&adoptable_facts()),
            &[],
            Instant::now()
        )
        .expect("allowed"),
        Approved::WatchPane { pane: PANE }
    );
}

#[test]
fn a_watch_request_for_a_token_with_no_live_pane_is_refused() {
    // The token could not be resolved, so the sender is not inside any pane this
    // host owns — another nightcrow session's helper, most likely.
    let mut g = guard();
    let t = token();
    assert_eq!(
        g.judge(watch(&t), None, &[], Instant::now())
            .expect_err("refused"),
        Refused::UnknownPane { token: t }
    );
}

#[test]
fn a_watch_request_from_a_plugin_the_config_did_not_allow_is_refused() {
    // The switch is the whole difference between this and enumerating panes, so
    // its absence has to refuse even a request that is otherwise perfect.
    let mut g = guard();
    let t = token();
    let facts = PaneFacts {
        may_watch_on_signal: false,
        ..adoptable_facts()
    };
    assert_eq!(
        g.judge(watch(&t), Some(&facts), &[], Instant::now())
            .expect_err("refused"),
        Refused::WatchNotAllowed {
            pane: PANE,
            token: t
        }
    );
}

#[test]
fn a_watch_request_for_a_pane_another_plugin_watches_is_refused() {
    let mut g = guard();
    let facts = PaneFacts {
        watched_by_another: true,
        ..adoptable_facts()
    };
    assert_eq!(
        g.judge(watch(&token()), Some(&facts), &[], Instant::now())
            .expect_err("refused"),
        Refused::PaneWatchedByAnother { pane: PANE }
    );
}

#[test]
fn a_watch_request_for_a_pane_whose_process_has_exited_is_refused() {
    // Nothing left to watch: the only recovery such a pane can be given is typed
    // into a live process, and this one has none.
    let mut g = guard();
    let facts = PaneFacts {
        alive: false,
        ..adoptable_facts()
    };
    assert_eq!(
        g.judge(watch(&token()), Some(&facts), &[], Instant::now())
            .expect_err("refused"),
        Refused::PaneNotRunning { pane: PANE }
    );
}

#[test]
fn a_watch_request_for_a_pane_this_plugin_already_has_is_approved_again() {
    // How a plugin recovers from having been given an opted-in pane it could not
    // recognise: asking again is what gets it another `PaneOpened`.
    let mut g = guard();
    let facts = PaneFacts {
        opted_in: true,
        ..adoptable_facts()
    };
    assert_eq!(
        g.judge(watch(&token()), Some(&facts), &[], Instant::now())
            .expect("allowed"),
        Approved::WatchPane { pane: PANE }
    );
}

#[test]
fn a_watch_request_does_not_spend_the_panes_input_budget() {
    // Being given a pane is not something done *to* the pane. Charging it would
    // spend the allowance the recovery it unlocks is about to need.
    let mut g = guard();
    let t = token();
    let now = Instant::now();
    let limits = RateLimits::default();
    for _ in 0..limits.max_sends_per_window * 2 {
        g.judge(watch(&t), Some(&adoptable_facts()), &[], now)
            .expect("allowed");
    }
    assert_eq!(g.budgets.spent(&t, RateAction::SendInput, &limits, now), 0);
    assert_eq!(g.budgets.spent(&t, RateAction::Relaunch, &limits, now), 0);
}

#[test]
fn a_watch_request_names_no_generation_so_it_cannot_be_stale() {
    // A plugin asking for a pane it has never been told about cannot know which
    // spawn it is looking at; the `PaneOpened` it is answered with is what says.
    // A generation rule here would therefore refuse every honest request.
    assert_eq!(watch(&token()).generation(), None);
    let mut g = guard();
    let facts = PaneFacts {
        generation: GENERATION + 5,
        ..adoptable_facts()
    };
    assert!(
        g.judge(watch(&token()), Some(&facts), &[], Instant::now())
            .is_ok()
    );
}
