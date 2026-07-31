//! What a reload does to pane ownership and to slots held for a relaunch.
//!
//! Driven against `Plugins` directly rather than through a hub, because neither
//! case has a shape at the hub's command queue: one needs a pane adopted with no
//! opt-in (which goes through the token path), the other a pane whose process has
//! already exited while its slot is held.

use super::plugin_reload::{plugin_with_log, respawning};
use super::plugins::fixture;
use crate::backend::{PtyBackend, TerminalBackend};
use crate::config::PluginConfig;
use crate::web::viewer::terminal::hub_plugins::Plugins;
use std::collections::HashMap;

/// The arm of the decision that only a `watch_on_signal` plugin can reach:
/// nothing in the startup list names it, so being told to stop watching on signal
/// would leave it wanted by nothing — except the pane it is already watching.
///
/// Driven against `Plugins` directly, because getting a pane adopted without an
/// opt-in means going through the token path, which has no shape at the hub's
/// command queue.
#[test]
fn a_plugin_no_startup_pane_names_is_kept_while_it_watches_a_live_pane() {
    let f = fixture();
    let mut backend = PtyBackend::new(f.cwd());
    let watcher = PluginConfig {
        watch_on_signal: true,
        ..plugin_with_log(&f, "signal").0
    };
    let mut plugins = Plugins::start(&f.cwd(), std::slice::from_ref(&watcher), &[]);
    let pane = backend
        .open_pane(24, 80, Some("sleep 30"))
        .expect("open a pane");
    assert!(plugins.adopt(pane, "signal"), "the host must be live");
    let titles = HashMap::from([(pane, None)]);

    // The operator's switch goes off. No opt-in names this plugin, so the only
    // thing that can keep it is the pane it is already watching.
    let mut off = watcher;
    off.watch_on_signal = false;
    let outcome = plugins.reload(&mut backend, &[off], &[], &titles);

    assert!(
        outcome.stopped.is_empty(),
        "a plugin watching a live pane must be kept: {outcome:?}"
    );
    assert_eq!(
        plugins.owner(pane),
        Some("signal"),
        "and it must keep the pane"
    );
    backend.destroy_pane(pane);
}

/// A hold belongs to the child that was given the pane's token, and dies with it.
///
/// The successor is handed the panes the hub still has, and a pane whose process
/// exited is not among them — so it never learns that token. Left in place the
/// slot would sit out its whole nine-day window with nothing that could honour
/// it, while every client counted down to a relaunch that was never coming.
///
/// Driven against `Plugins` directly: what is asserted is the slot's fate, which
/// the backend answers for by token.
#[test]
fn a_replaced_plugin_does_not_leave_a_relaunch_hold_behind() {
    use super::plugin_rules::{COLS, LONG_RUNNING, ROWS, opt_in, token_of};
    use crate::web::viewer::terminal::hub_plugins_slots::PaneSpot;
    use std::time::Instant;

    let f = fixture();
    let mut backend = PtyBackend::new(f.cwd());
    let (cfg, _log) = plugin_with_log(&f, "recovery");
    // `opt_in()` names the plugin `plugin_rules` uses, so match it.
    let cfg = PluginConfig {
        name: opt_in().plugin.clone().expect("the fixture opts in"),
        ..cfg
    };
    let mut plugins = Plugins::start(&f.cwd(), std::slice::from_ref(&cfg), &[opt_in()]);
    let pane = backend
        .open_pane(ROWS, COLS, Some(LONG_RUNNING))
        .expect("open a pane");
    let token = token_of(&backend, pane);
    assert!(plugins.adopt(pane, &cfg.name));

    // The pane's process ends and its slot is held for a relaunch.
    backend.release_process(pane);
    plugins.hold_for_relaunch(
        pane,
        PaneSpot {
            index: 0,
            rows: ROWS,
            cols: COLS,
            title: None,
        },
        Instant::now(),
    );
    assert_eq!(backend.pane_for_token(&token), Some(pane), "held to start");

    // A reload that replaces this plugin's child. The pane exited, so it is not
    // among the hub's live panes — hence the empty `titles`.
    let outcome = plugins.reload(
        &mut backend,
        &[respawning(&cfg)],
        &[opt_in()],
        &HashMap::new(),
    );

    assert_eq!(outcome.restarted, std::slice::from_ref(&cfg.name));
    assert_eq!(
        outcome.retired,
        [pane],
        "the caller has to be told, so the clients stop counting down"
    );
    assert_eq!(
        backend.pane_for_token(&token),
        None,
        "a hold no child can honour must not outlive the one that held it"
    );
}
