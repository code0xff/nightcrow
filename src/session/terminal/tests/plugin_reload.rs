//! Re-applying the `[[plugin]]` table on a hub that is already serving panes.
//!
//! Driven through a real hub with real plugin children, like the rest of the
//! plugin tests: what is asserted is which child was handed which pane, read out
//! of the file that child appends to, rather than the diff's own bookkeeping.
//!
//! The fake plugin here appends one extra line after its stdin closes, so "the
//! child was stopped" is something a test can wait *for* rather than infer from
//! nothing happening. Where an absence still has to be asserted, a further reload
//! whose effect is observable is queued behind the one under test — the worker
//! drains its queue in order, so by the time that effect arrives the earlier
//! decision has been made and a missing announcement is a real one.
//!
//! These tests are Unix-only: the fake plugin is `/bin/sh` and the test commands
//! use Unix shell syntax.
#![cfg(unix)]

use super::plugins::{Fixture, LOG_ENV, fixture, logged, logged_event, shell_plugin};
use super::{attach, collect_created, spawn_hub, wait_for};
use crate::config::{PluginConfig, StartupCommand};
use crate::session::terminal::TerminalHub;
use crate::session::terminal::frame::ClientMessage;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Records every event, then says so when its stdin closes — which is how a
/// stopped child announces itself. `{{`/`}}` are `format!`'s escapes for the JSON
/// braces the line needs to parse like any other event.
fn farewell_script() -> String {
    format!(
        r#"while IFS= read -r line; do printf '%s\n' "$line" >> "${LOG_ENV}"; done
printf '{{"event":"stopped"}}\n' >> "${LOG_ENV}""#
    )
}

/// A pane that opts into `plugin` and stays alive long enough to be watched.
fn watched_pane(name: &str, plugin: &str) -> StartupCommand {
    StartupCommand {
        name: Some(name.to_string()),
        command: "sleep 30".to_string(),
        plugin: Some(plugin.to_string()),
    }
}

/// A plugin with its own log, so which child received an event is decided by
/// which file it landed in.
pub(super) fn plugin_with_log(f: &Fixture, name: &str) -> (PluginConfig, PathBuf) {
    let log = f.log.with_file_name(format!("{name}.ndjson"));
    let cfg = shell_plugin(name, farewell_script(), &log, Path::new("/dev/null"));
    (cfg, log)
}

/// The same plugin with a different argv, which is what forces its child to be
/// replaced. The extra argument is inert — `/bin/sh -c script` ignores it.
pub(super) fn respawning(cfg: &PluginConfig) -> PluginConfig {
    let mut next = cfg.clone();
    next.args.push("ignored-extra-arg".to_string());
    next
}

fn disabled(cfg: &PluginConfig) -> PluginConfig {
    let mut next = cfg.clone();
    next.enabled = false;
    next
}

fn is(event: &serde_json::Value, kind: &str) -> bool {
    event["event"] == kind
}

fn count_of(log: &Path, kind: &str) -> usize {
    logged(log).iter().filter(|e| is(e, kind)).count()
}

/// Wait until `log` has recorded more events of `kind` than `was`.
fn beyond(log: &Path, kind: &str, was: usize) -> Option<usize> {
    wait_for(|| {
        let now = count_of(log, kind);
        (now > was).then_some(now)
    })
}

/// A hub serving one watched pane per plugin, both plugins running and both panes
/// already announced. The second plugin is the anchor the absence assertions
/// below wait on.
fn hub_with_two_watched_panes(
    f: &Fixture,
) -> (
    Arc<TerminalHub>,
    (PluginConfig, PathBuf),
    (PluginConfig, PathBuf),
) {
    let subject = plugin_with_log(f, "subject");
    let anchor = plugin_with_log(f, "anchor");
    let hub = spawn_hub(
        &f.cwd(),
        vec![
            watched_pane("subject-pane", "subject"),
            watched_pane("anchor-pane", "anchor"),
        ],
        vec![subject.0.clone(), anchor.0.clone()],
    );
    let session = attach(&hub);
    session.dispatch(ClientMessage::Start { sizes: Vec::new() });
    collect_created(&session, 2);
    logged_event(&subject.1, |e| is(e, "pane_opened")).expect("the subject pane was not announced");
    logged_event(&anchor.1, |e| is(e, "pane_opened")).expect("the anchor pane was not announced");
    (hub, subject, anchor)
}

/// The case the whole feature exists for: a plugin added to `config.toml` while
/// the session is up gets the pane that was already asking for it.
#[test]
fn enabling_a_plugin_hands_it_the_panes_that_opted_in_while_it_was_off() {
    let f = fixture();
    let (cfg, log) = plugin_with_log(&f, "watch");
    let hub = spawn_hub(
        &f.cwd(),
        vec![watched_pane("agent", "watch")],
        vec![disabled(&cfg)],
    );
    let session = attach(&hub);
    session.dispatch(ClientMessage::Start { sizes: Vec::new() });
    collect_created(&session, 1);

    hub.reload_plugins(vec![cfg]);

    // The pane was created while the plugin was off, so nothing adopted it then.
    // What the opt-in records is that it was meant for this plugin all along.
    let opened = logged_event(&log, |e| is(e, "pane_opened"))
        .expect("the newly enabled plugin was never given the pane that opted into it");
    assert_eq!(opened["title"], "agent", "wrong pane announced: {opened}");
    hub.stop();
}

#[test]
fn a_restarted_plugin_is_handed_its_panes_again() {
    let f = fixture();
    let (hub, (subject, subject_log), _anchor) = hub_with_two_watched_panes(&f);
    let before = count_of(&subject_log, "pane_opened");

    hub.reload_plugins(vec![respawning(&subject)]);

    // The old child is stopped and its successor is told about the pane, which it
    // could not otherwise know exists.
    assert!(
        logged_event(&subject_log, |e| is(e, "stopped")).is_some(),
        "the replaced child was never stopped"
    );
    assert!(
        beyond(&subject_log, "pane_opened", before).is_some(),
        "the replacement was never handed the pane it owns"
    );
    hub.stop();
}

#[test]
fn only_the_rules_changing_leaves_the_child_alone() {
    let f = fixture();
    let (hub, (subject, subject_log), _anchor) = hub_with_two_watched_panes(&f);
    let before = count_of(&subject_log, "pane_opened");

    // Same command, args and environment; only what the plugin may append on a
    // relaunch differs. That is read on every judgement rather than baked into
    // the child, so the process must survive — a plugin can be hours into a wait.
    let mut retuned = subject.clone();
    retuned.allowed_resume_flags = vec!["--resume".to_string()];
    hub.reload_plugins(vec![retuned]);
    // Then a change that *does* force a replacement, as the anchor for the
    // absence: one announcement must follow, not two.
    hub.reload_plugins(vec![respawning(&subject)]);

    assert_eq!(
        beyond(&subject_log, "pane_opened", before).expect("the forced restart never re-announced"),
        before + 1,
        "a rules-only change must not restart the plugin"
    );
    hub.stop();
}

#[test]
fn disabling_a_plugin_stops_it_and_re_enabling_hands_its_panes_back() {
    let f = fixture();
    let (hub, (subject, subject_log), _anchor) = hub_with_two_watched_panes(&f);
    let before = count_of(&subject_log, "pane_opened");

    hub.reload_plugins(vec![disabled(&subject)]);
    assert!(
        logged_event(&subject_log, |e| is(e, "stopped")).is_some(),
        "a disabled plugin's child must be stopped"
    );

    // Back on again. The pane is handed over exactly as enabling it the first
    // time would: `enabled` means the same thing whichever way it was last
    // flipped, which it would not if being switched off also erased the opt-in.
    hub.reload_plugins(vec![subject]);
    assert!(
        beyond(&subject_log, "pane_opened", before).is_some(),
        "re-enabling must hand the pane back"
    );
    hub.stop();
}

/// Pressing the button twice must not churn the session's plugin children.
#[test]
fn a_reload_that_changes_nothing_leaves_every_child_alone() {
    let f = fixture();
    let (hub, (subject, subject_log), (anchor, anchor_log)) = hub_with_two_watched_panes(&f);
    let anchor_before = count_of(&anchor_log, "pane_opened");

    hub.reload_plugins(vec![subject.clone(), anchor.clone()]);
    // The anchor's forced restart is what says the pass above is over, so the
    // absence below is a decision rather than a slow child.
    hub.reload_plugins(vec![subject, respawning(&anchor)]);

    assert!(
        beyond(&anchor_log, "pane_opened", anchor_before).is_some(),
        "the anchor's restart never landed, so the absence below proves nothing"
    );
    assert_eq!(
        count_of(&subject_log, "stopped"),
        0,
        "an unchanged plugin must not be restarted"
    );
    hub.stop();
}

#[test]
fn a_plugin_dropped_from_the_table_is_stopped() {
    let f = fixture();
    let (hub, (_subject, subject_log), (anchor, _anchor_log)) = hub_with_two_watched_panes(&f);

    // Not merely disabled — gone from the table, so nothing can want it.
    hub.reload_plugins(vec![anchor]);

    assert!(
        logged_event(&subject_log, |e| is(e, "stopped")).is_some(),
        "a plugin the table no longer declares must be stopped"
    );
    hub.stop();
}
