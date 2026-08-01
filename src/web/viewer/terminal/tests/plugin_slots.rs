//! How long a watched pane's slot lives, and how a quiet one is announced.
//!
//! Beside `plugin_rules.rs` rather than in it: those tests are about what a
//! plugin is allowed to ask for, these about what the worker keeps, gives up on
//! its own, or hands back to a person. Both drive a real `PtyBackend` with the
//! clock as an input.
//!
//! These tests are Unix-only: they spawn real PTY processes with Unix commands
//! (`sleep 30`) and `/bin/sh`-based plugins.
#![cfg(unix)]

use super::plugin_rules::{
    COLS, LONG_RUNNING, PLUGIN, ROWS, opt_in, send_input, token_of, well_idle,
};
use super::plugins::{fixture, logged, logged_event, recorder};
use crate::backend::{PtyBackend, TerminalBackend};
use crate::config::ShellConfig;
use crate::plugin::{Approved, RateLimits, Refused};
use crate::web::viewer::terminal::hub_plugins::Plugins;
use crate::web::viewer::terminal::hub_plugins_slots::{PENDING_RELAUNCH_TTL, PaneSpot};
use std::time::Instant;

#[test]
fn an_exited_watched_pane_keeps_its_token_while_a_plain_pane_loses_its() {
    // The two calls the hub chooses between on an exit. `release_process` is
    // what makes a relaunch possible at all; `destroy_pane` is what makes an
    // ordinary pane unaddressable the moment it ends.
    let f = fixture();
    let mut backend = PtyBackend::new(f.cwd(), ShellConfig::default());
    let watched = backend
        .open_pane(ROWS, COLS, Some(LONG_RUNNING))
        .expect("open a pane");
    let plain = backend
        .open_pane(ROWS, COLS, Some(LONG_RUNNING))
        .expect("open a pane");
    let (watched_token, plain_token) = (token_of(&backend, watched), token_of(&backend, plain));

    backend.release_process(watched);
    backend.destroy_pane(plain);

    assert_eq!(
        backend.pane_for_token(&watched_token),
        Some(watched),
        "a watched pane's slot must outlive its process"
    );
    assert_eq!(
        backend.pane_for_token(&plain_token),
        None,
        "a plain pane must leave nothing addressable behind"
    );
    backend.destroy_pane(watched);
}

#[test]
fn a_hold_that_runs_out_of_time_retires_the_slot() {
    let f = fixture();
    let mut backend = PtyBackend::new(f.cwd(), ShellConfig::default());
    let mut plugins = Plugins::start(&f.cwd(), &[recorder(PLUGIN, &f.log)], &[opt_in()]);
    let pane = backend
        .open_pane(ROWS, COLS, Some(LONG_RUNNING))
        .expect("open a pane");
    let token = token_of(&backend, pane);
    assert!(plugins.adopt(pane, PLUGIN));
    let exited_at = Instant::now();
    backend.release_process(pane);
    plugins.hold_for_relaunch(
        pane,
        PaneSpot {
            index: 0,
            rows: ROWS,
            cols: COLS,
            title: None,
        },
        exited_at,
    );

    // Still inside the window: nothing is given up.
    plugins.expire_pending(&mut backend, exited_at + PENDING_RELAUNCH_TTL / 2);
    assert_eq!(backend.pane_for_token(&token), Some(pane));

    plugins.expire_pending(&mut backend, exited_at + PENDING_RELAUNCH_TTL);

    assert_eq!(
        backend.pane_for_token(&token),
        None,
        "a hold nobody used must retire the slot"
    );
    assert_eq!(plugins.owner(pane), None);
}

#[test]
fn a_human_typing_into_a_watched_pane_clears_what_its_plugin_had_spent() {
    let f = fixture();
    let mut backend = PtyBackend::new(f.cwd(), ShellConfig::default());
    let mut plugins = Plugins::start(&f.cwd(), &[recorder(PLUGIN, &f.log)], &[opt_in()]);
    let pane = backend
        .open_pane(ROWS, COLS, Some(LONG_RUNNING))
        .expect("open a pane");
    let token = token_of(&backend, pane);
    assert!(plugins.adopt(pane, PLUGIN));
    let now = well_idle();

    // Spend the whole per-pane allowance, then confirm it really is spent.
    for _ in 0..RateLimits::default().max_sends_per_window {
        plugins
            .judge(PLUGIN, send_input(&token, 1), &backend, now)
            .expect("an idle, opted-in pane accepts input");
    }
    let refused = plugins
        .judge(PLUGIN, send_input(&token, 1), &backend, now)
        .expect_err("the allowance should be gone");
    assert!(matches!(refused, Refused::RateLimited { .. }), "{refused}");

    plugins.user_input(&backend, pane);

    // The pane belongs to the person now, so what the plugin had spent on the
    // situation it thought it was in is void — including the budget.
    assert!(
        matches!(
            plugins.judge(PLUGIN, send_input(&token, 1), &backend, now),
            Ok(Approved::SendInput { .. })
        ),
        "a human taking the pane back must clear the plugin's spent budget"
    );
    assert!(
        logged_event(&f.log, |e| e["event"] == "user_input").is_some(),
        "the plugin must be told a human took the pane back"
    );
    backend.destroy_pane(pane);
}

#[test]
fn a_quiet_pane_is_announced_idle_once_until_it_speaks_again() {
    let f = fixture();
    let mut backend = PtyBackend::new(f.cwd(), ShellConfig::default());
    let mut plugins = Plugins::start(&f.cwd(), &[recorder(PLUGIN, &f.log)], &[opt_in()]);
    let pane = backend
        .open_pane(ROWS, COLS, Some(LONG_RUNNING))
        .expect("open a pane");
    assert!(plugins.adopt(pane, PLUGIN));
    let now = well_idle();

    plugins.notify_idle(&backend, now);
    plugins.notify_idle(&backend, now);
    assert!(
        logged_event(&f.log, |e| e["event"] == "pane_idle").is_some(),
        "the plugin was never told the pane went quiet"
    );
    assert_eq!(
        logged(&f.log)
            .iter()
            .filter(|e| e["event"] == "pane_idle")
            .count(),
        1,
        "one quiet period must be announced once, not on every tick"
    );

    // Output ends the quiet period, so the next one is announced again.
    plugins.pane_output(&backend, pane, b"still here\n");
    plugins.notify_idle(&backend, now);

    assert!(
        super::wait_for(|| {
            let idles = logged(&f.log)
                .iter()
                .filter(|e| e["event"] == "pane_idle")
                .count();
            (idles == 2).then_some(())
        })
        .is_some(),
        "a new quiet period must be announced again"
    );
    backend.destroy_pane(pane);
}
