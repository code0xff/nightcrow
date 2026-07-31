//! Being given a pane nobody opted in, driven against a real `PtyBackend`.
//!
//! The half that cannot be tested here is the token's journey: it reaches a
//! plugin through a provider's own hook, inside a pane, over a socket this crate
//! does not own. What *is* pinned here is everything the host decides once a token
//! comes back — which pane it names, whether the operator allowed it, and what
//! being given the pane does and does not entitle a plugin to.

use super::plugin_rules::{COLS, LONG_RUNNING, PLUGIN, ROWS, opt_in, token_of};
use super::plugins::{fixture, recorder};
use crate::backend::{PaneId, PaneToken, PtyBackend, TerminalBackend};
use crate::config::{PluginConfig, ShellConfig};
use crate::plugin::protocol::{PROTOCOL_VERSION, PluginCommand};
use crate::plugin::{Approved, Refused};
use crate::web::viewer::terminal::hub_plugins::Plugins;
use crate::web::viewer::terminal::hub_recovery::is_relaunchable;
use std::path::Path;
use std::time::Instant;

/// The same recording plugin, with the operator's switch on.
fn signal_watcher(name: &str, log: &Path) -> PluginConfig {
    PluginConfig {
        watch_on_signal: true,
        ..recorder(name, log)
    }
}

fn watch(token: &PaneToken) -> PluginCommand {
    PluginCommand::WatchPane {
        v: PROTOCOL_VERSION,
        token: token.clone(),
    }
}

/// A pane a client opened: no title, and no command of its own.
fn bare_pane(backend: &mut PtyBackend) -> PaneId {
    backend
        .open_pane(ROWS, COLS, None)
        .expect("open a bare pane")
}

#[test]
fn a_plugin_that_watches_on_signal_is_launched_with_no_pane_opted_in() {
    // The panes it exists for cannot opt in, so it has to be running before any of
    // them says anything. `adopt` succeeding is how "it has a live host" is read.
    let f = fixture();
    let mut plugins = Plugins::start(&f.cwd(), &[signal_watcher(PLUGIN, &f.log)], &[]);

    assert!(plugins.adopt(1, PLUGIN));
}

#[test]
fn a_watch_request_bearing_a_live_panes_token_is_approved_and_the_pane_handed_over() {
    let f = fixture();
    let mut backend = PtyBackend::new(f.cwd(), ShellConfig::default());
    let mut plugins = Plugins::start(&f.cwd(), &[signal_watcher(PLUGIN, &f.log)], &[]);
    let pane = backend
        .open_pane(ROWS, COLS, Some(LONG_RUNNING))
        .expect("open a pane");
    let token = token_of(&backend, pane);
    assert_eq!(plugins.owner(pane), None, "nothing handed it over yet");

    let approved = plugins
        .judge(PLUGIN, watch(&token), &backend, Instant::now())
        .expect("a live pane's own token is the evidence");

    assert_eq!(approved, Approved::WatchPane { pane });
    // The association is the caller's to record, which is what the hub does next.
    assert!(plugins.adopt(pane, PLUGIN));
    assert_eq!(plugins.owner(pane), Some(PLUGIN));
    backend.destroy_pane(pane);
}

#[test]
fn a_watch_request_from_a_plugin_without_the_switch_is_refused() {
    // The identical request, refused purely because the operator did not ask for
    // it: this is the only thing standing between a signal and a new association.
    let f = fixture();
    let mut backend = PtyBackend::new(f.cwd(), ShellConfig::default());
    let mut plugins = Plugins::start(&f.cwd(), &[recorder(PLUGIN, &f.log)], &[opt_in()]);
    let pane = bare_pane(&mut backend);
    let token = token_of(&backend, pane);

    let refused = plugins
        .judge(PLUGIN, watch(&token), &backend, Instant::now())
        .expect_err("a plugin without the switch may not be given a pane");

    assert!(
        matches!(refused, Refused::WatchNotAllowed { .. }),
        "{refused}"
    );
    assert_eq!(plugins.owner(pane), None);
    backend.destroy_pane(pane);
}

#[test]
fn a_watch_request_for_a_pane_another_plugin_holds_is_refused() {
    const OTHER: &str = "other";
    let f = fixture();
    let mut backend = PtyBackend::new(f.cwd(), ShellConfig::default());
    let mut plugins = Plugins::start(
        &f.cwd(),
        &[
            signal_watcher(PLUGIN, &f.log),
            signal_watcher(OTHER, &f.log),
        ],
        &[],
    );
    let pane = bare_pane(&mut backend);
    let token = token_of(&backend, pane);
    assert!(plugins.adopt(pane, OTHER));

    let refused = plugins
        .judge(PLUGIN, watch(&token), &backend, Instant::now())
        .expect_err("one pane, one watcher");

    assert!(
        matches!(refused, Refused::PaneWatchedByAnother { .. }),
        "{refused}"
    );
    assert_eq!(plugins.owner(pane), Some(OTHER), "the holder keeps it");
    backend.destroy_pane(pane);
}

#[test]
fn a_watch_request_for_a_token_this_host_never_minted_is_refused() {
    // A helper from another nightcrow session on the same machine: the socket is
    // per-user, so its tokens do reach us, and they must resolve to nothing.
    let f = fixture();
    let backend = PtyBackend::new(f.cwd(), ShellConfig::default());
    let mut plugins = Plugins::start(&f.cwd(), &[signal_watcher(PLUGIN, &f.log)], &[]);
    let stranger = PaneToken::new().expect("OS RNG");

    let refused = plugins
        .judge(PLUGIN, watch(&stranger), &backend, Instant::now())
        .expect_err("a token naming no pane of ours");

    assert!(matches!(refused, Refused::UnknownPane { .. }), "{refused}");
}

#[test]
fn a_pane_handed_over_on_a_signal_can_never_be_relaunched() {
    // The invariant that keeps late adoption from doing something worse than
    // nothing: the pane is a shell, so a relaunch would restart the shell rather
    // than the session the plugin was recovering.
    let f = fixture();
    let mut backend = PtyBackend::new(f.cwd(), ShellConfig::default());
    let mut plugins = Plugins::start(&f.cwd(), &[signal_watcher(PLUGIN, &f.log)], &[]);
    let pane = bare_pane(&mut backend);
    let token = token_of(&backend, pane);
    assert!(plugins.adopt(pane, PLUGIN));
    // The exit the plugin would answer with a relaunch.
    backend.release_process(pane);

    let refused = plugins
        .judge(
            PLUGIN,
            PluginCommand::Relaunch {
                v: PROTOCOL_VERSION,
                token,
                generation: 1,
                resume_args: Vec::new(),
            },
            &backend,
            Instant::now(),
        )
        .expect_err("a pane with no command of its own has nothing to relaunch");

    assert!(
        matches!(refused, Refused::NoLaunchCommand { .. }),
        "{refused}"
    );
    backend.retire_slot(pane);
}

#[test]
fn an_exited_pane_with_no_command_is_not_worth_holding_a_slot_for() {
    // The condition the hub branches on when a watched pane's process ends. A
    // hold lasts days and exists only to make a relaunch possible, so a pane that
    // can never be relaunched must take the closing path instead — otherwise a
    // shell that exited would keep its slot for the whole window.
    let f = fixture();
    let mut backend = PtyBackend::new(f.cwd(), ShellConfig::default());
    let bare = bare_pane(&mut backend);
    let launched = backend
        .open_pane(ROWS, COLS, Some(LONG_RUNNING))
        .expect("open a pane");

    assert!(!is_relaunchable(&backend, bare));
    assert!(is_relaunchable(&backend, launched));

    backend.destroy_pane(bare);
    backend.destroy_pane(launched);
}
