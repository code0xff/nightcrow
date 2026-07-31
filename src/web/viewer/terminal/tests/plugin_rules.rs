//! The rules the worker applies to a plugin's panes, driven directly against a
//! real `PtyBackend` rather than through the hub.
//!
//! Deterministic on purpose: [`Plugins::judge`] and the idle check both take
//! `now` as a parameter, so the clock is an input and nothing here has to wait
//! for time to pass. The hub's *routing* — which of these calls it makes on an
//! exit — is pinned by the integration tests in `plugins.rs`.

use super::plugins::{fixture, recorder};
use crate::backend::{PaneId, PaneToken, PtyBackend, TerminalBackend};
use crate::config::{ShellConfig, StartupCommand};
use crate::plugin::Refused;
use crate::plugin::protocol::{PROTOCOL_VERSION, PluginCommand};
use crate::web::viewer::terminal::hub_plugins::{PANE_IDLE_THRESHOLD, Plugins};
use std::time::Instant;

pub(super) const ROWS: u16 = 24;
pub(super) const COLS: u16 = 80;
pub(super) const PLUGIN: &str = "watch";
/// Long enough that the panes below stay alive for the whole test.
pub(super) const LONG_RUNNING: &str = "sleep 30";

pub(super) fn opt_in() -> StartupCommand {
    StartupCommand {
        name: None,
        command: LONG_RUNNING.to_string(),
        plugin: Some(PLUGIN.to_string()),
    }
}

/// Far enough past a pane's birth that the guard's idle rule is satisfied
/// without the test sleeping.
pub(super) fn well_idle() -> Instant {
    Instant::now() + PANE_IDLE_THRESHOLD * 2
}

pub(super) fn token_of(backend: &PtyBackend, pane: PaneId) -> PaneToken {
    backend
        .slot(pane)
        .expect("the pane should still have a slot")
        .identity
        .token
        .clone()
}

pub(super) fn send_input(token: &PaneToken, generation: u32) -> PluginCommand {
    PluginCommand::SendInput {
        v: PROTOCOL_VERSION,
        token: token.clone(),
        generation,
        data: "recovered\n".to_string(),
    }
}

#[test]
fn a_plugin_no_pane_opted_into_is_never_launched() {
    // A host with no pane to watch is a child process that can never be given
    // anything to do, so declaring a plugin must not by itself start one.
    let f = fixture();
    let mut plugins = Plugins::start(&f.cwd(), &[recorder(PLUGIN, &f.log)], &[]);

    assert!(
        !plugins.adopt(1, PLUGIN),
        "a plugin nothing opted into must have no host to adopt with"
    );
}

#[test]
fn a_disabled_plugin_is_never_launched_even_when_a_pane_opted_in() {
    let f = fixture();
    let mut off = recorder(PLUGIN, &f.log);
    off.enabled = false;
    let mut plugins = Plugins::start(&f.cwd(), &[off], &[opt_in()]);

    assert!(!plugins.adopt(1, PLUGIN));
}

#[test]
fn a_plugin_that_will_not_launch_leaves_its_panes_unmanaged() {
    // Unmanaged rather than half-managed: a pane recorded against a host that
    // does not exist would take the relaunch path on exit, holding its slot open
    // for hours for a plugin that will never ask.
    let f = fixture();
    let mut broken = recorder(PLUGIN, &f.log);
    broken.command = "/nonexistent/nightcrow-test-plugin".to_string();
    let mut plugins = Plugins::start(&f.cwd(), &[broken], &[opt_in()]);

    assert!(!plugins.adopt(1, PLUGIN));
    assert_eq!(plugins.owner(1), None);
}

#[test]
fn a_command_for_a_pane_that_did_not_opt_in_is_refused() {
    let f = fixture();
    let mut backend = PtyBackend::new(f.cwd(), ShellConfig::default());
    let mut plugins = Plugins::start(&f.cwd(), &[recorder(PLUGIN, &f.log)], &[opt_in()]);
    let pane = backend
        .open_pane(ROWS, COLS, Some(LONG_RUNNING))
        .expect("open a pane");
    let token = token_of(&backend, pane);

    // Deliberately not adopted: the token resolves, but nothing handed the pane
    // over.
    let refused = plugins
        .judge(PLUGIN, send_input(&token, 1), &backend, well_idle())
        .expect_err("a pane nobody handed over must not be typed into");

    assert!(matches!(refused, Refused::NotOptedIn { .. }), "{refused}");
    backend.destroy_pane(pane);
}

#[test]
fn a_command_naming_a_stale_generation_is_refused_and_leaves_the_replacement_alone() {
    let f = fixture();
    let mut backend = PtyBackend::new(f.cwd(), ShellConfig::default());
    let mut plugins = Plugins::start(&f.cwd(), &[recorder(PLUGIN, &f.log)], &[opt_in()]);
    let pane = backend
        .open_pane(ROWS, COLS, Some(LONG_RUNNING))
        .expect("open a pane");
    let token = token_of(&backend, pane);
    assert!(plugins.adopt(pane, PLUGIN));

    // The exit-and-relaunch the hub performs, without the hub.
    backend.release_process(pane);
    let replacement = backend
        .relaunch_pane(pane, ROWS, COLS, &[], &[])
        .expect("relaunch the slot");
    plugins.take_over(pane, replacement);
    assert_eq!(
        token_of(&backend, replacement),
        token,
        "the slot's token must survive the relaunch"
    );

    let refused = plugins
        .judge(PLUGIN, send_input(&token, 1), &backend, well_idle())
        .expect_err("a command about the previous process must not land on this one");

    assert!(
        matches!(
            refused,
            Refused::StaleGeneration {
                claimed: 1,
                current: 2,
                ..
            }
        ),
        "{refused}"
    );
    assert!(
        backend.is_process_alive(replacement),
        "a refused command must leave the replacement process untouched"
    );
    backend.destroy_pane(replacement);
}
