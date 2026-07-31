//! The hub's test harness: the shared deadline and the frame accessors every
//! hub test reads its assertions through. The tests themselves live beside it.

mod behavior;
mod plugin_reload;
mod plugin_reload_panes;
mod plugin_rules;
mod plugin_slots;
mod plugin_watch;
mod plugins;
mod reattach;
mod recovery;
mod scrollback_depth;
mod size_owner;
mod startup;
mod wire;

use crate::backend::PaneId;
use crate::web::viewer::terminal::frame::TerminalFrame;
use std::thread;
use std::time::{Duration, Instant};

use super::TerminalSession;

/// Deadline for the real-shell tests below. `connect` spawns the user's
/// actual `$SHELL` (an interactive zsh sources its full rc chain), and
/// cargo runs tests in parallel, so several shells initialize at once — a
/// tighter budget was measurably flaky under load. A generous bound only
/// delays the failure verdict; passing runs still finish the instant the
/// frame arrives. Mirrors `backend::pty::tests::PTY_TEST_DEADLINE`.
pub(super) const SHELL_TEST_DEADLINE: Duration = Duration::from_secs(15);

pub(super) fn wait_for<T>(mut take: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + SHELL_TEST_DEADLINE;
    while Instant::now() < deadline {
        if let Some(value) = take() {
            return Some(value);
        }
        thread::sleep(Duration::from_millis(10));
    }
    None
}

/// Pull frames until one satisfies `want`, ignoring the rest.
pub(super) fn next_matching(
    session: &TerminalSession,
    mut want: impl FnMut(&TerminalFrame) -> bool,
) -> Option<TerminalFrame> {
    wait_for(|| {
        session
            .next_frame(Duration::from_millis(50))
            .filter(|f| want(f))
    })
}

pub(super) fn created_pane(frame: &TerminalFrame) -> Option<PaneId> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] == "created" {
        return value["pane"].as_u64().map(|n| n as PaneId);
    }
    None
}

/// The pane an `exited` frame announces the end of.
pub(super) fn exited_pane(frame: &TerminalFrame) -> Option<PaneId> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "exited" {
        return None;
    }
    value["pane"].as_u64().map(|n| n as PaneId)
}

/// The pane count a `pending` frame offers for sizing.
pub(super) fn pending_count(frame: &TerminalFrame) -> Option<usize> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "pending" {
        return None;
    }
    value["count"].as_u64().map(|n| n as usize)
}

/// The name a `created` frame gives its pane, if the session named it.
pub(super) fn created_title(frame: &TerminalFrame) -> Option<String> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "created" {
        return None;
    }
    value["title"].as_str().map(str::to_string)
}

/// The size a `created` frame reports for its pane.
pub(super) fn created_size(frame: &TerminalFrame) -> Option<(u16, u16)> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "created" {
        return None;
    }
    Some((
        value["rows"].as_u64()? as u16,
        value["cols"].as_u64()? as u16,
    ))
}

/// The size a `resized` frame reports. Broadcast once the worker has actually
/// applied a resize, so a test that needs the new size to be in effect waits
/// for this rather than guessing at the worker's timing.
pub(super) fn resized_size(frame: &TerminalFrame) -> Option<(u16, u16)> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "resized" {
        return None;
    }
    Some((
        value["rows"].as_u64()? as u16,
        value["cols"].as_u64()? as u16,
    ))
}

pub(super) fn reordered_order(frame: &TerminalFrame) -> Option<Vec<PaneId>> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "reordered" {
        return None;
    }
    Some(
        value["order"]
            .as_array()?
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as PaneId))
            .collect(),
    )
}

/// Collect the ids of the first `n` distinct panes announced to `session`,
/// in the order the `created` frames arrive.
pub(super) fn collect_created(session: &TerminalSession, n: usize) -> Vec<PaneId> {
    let mut ids = Vec::new();
    while ids.len() < n {
        let created =
            next_matching(session, |f| created_pane(f).is_some()).expect("no created message");
        let id = created_pane(&created).unwrap();
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// A hub with a size ownership of its own.
///
/// Every test but the size-ownership ones wants an isolated session, so this is
/// the default: two hubs from two calls share nothing, exactly as two separate
/// sessions would. A test that needs one session across several repositories
/// builds the `SizeOwnership` itself and calls `TerminalHub::spawn`.
pub(super) fn spawn_hub(
    cwd: &str,
    startup: Vec<crate::config::StartupCommand>,
    plugins: Vec<crate::config::PluginConfig>,
) -> std::sync::Arc<super::TerminalHub> {
    super::TerminalHub::spawn(cwd, startup, plugins, Default::default())
}

/// A client arriving at `hub` — a page someone just opened, which is what every
/// test that is not about ownership means by connecting.
pub(super) fn attach(hub: &std::sync::Arc<super::TerminalHub>) -> TerminalSession {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = crate::web::viewer::size_owner::ViewerId::Browser(format!(
        "test-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    hub.connect(id, true)
}
