//! The plugin's main loop: NDJSON on stdin/stdout plus a clock.
//!
//! The host's stdin is read on one thread while the main thread blocks on its
//! channel with a timeout. The timeout *is* the clock: every wakeup, expired or
//! not, ticks the state machines.
//!
//! Everything the plugin says goes out from this one thread, so the NDJSON
//! stream cannot interleave two half-written lines — see
//! [`runloop_io`](crate::runloop_io), which owns both ends of that stream.

use crate::protocol::{LogLevel, PluginEvent, log};
use crate::provider::{PaneContext, Provider, detect};
use crate::runloop_io::{Message, emit, emit_all, spawn_stdin_reader};
use crate::state::{PaneRecovery, RecoveryState};
use crate::wait::now_epoch;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

/// How often the state machines are advanced when nothing else happens.
///
/// Waits last minutes to hours, so a second of granularity is far finer than
/// anything that depends on it; it is small enough that a pane looks responsive
/// and large enough that an idle plugin costs nothing measurable.
const TICK: Duration = Duration::from_secs(1);

/// Most pane slots tracked at once.
///
/// A session holds a handful of panes. The cap exists so a host that somehow
/// announced panes without ever closing them cannot grow this process without
/// bound; reaching it means dropping the *new* pane, which fails closed. It
/// bounds every pane the host gives this plugin.
const MAX_TRACKED_PANES: usize = 64;

/// One tracked pane: its recovery progress and the adapter watching it.
struct Watch {
    recovery: PaneRecovery,
    provider: Box<dyn Provider>,
    ctx: PaneContext,
}

pub fn run() -> Result<()> {
    let (tx, rx) = channel::<Message>();
    spawn_stdin_reader(tx);

    let mut panes: HashMap<String, Watch> = HashMap::new();
    emit(&log(LogLevel::Info, "nightcrow-recovery watching panes"))?;
    loop {
        let message = match rx.recv_timeout(TICK) {
            Ok(message) => Some(message),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
            // Every sender is gone, which only happens once the reader thread
            // has ended; treat it as the host having gone away.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Some(Message::HostGone),
        };
        match message {
            Some(Message::HostGone) | Some(Message::Host(PluginEvent::Shutdown { .. })) => {
                return farewell(&panes);
            }
            Some(Message::Host(event)) => on_host_event(&mut panes, &event)?,
            Some(Message::HostGarbage(reason)) => {
                emit(&log(LogLevel::Warn, reason))?;
            }
            None => {}
        }
        tick(&mut panes)?;
    }
}

/// Say what was left unfinished. A pane parked on a reset hours away simply
/// stops being watched when the host goes away, and a user who comes back to a
/// pane that never resumed deserves to find out why from the log.
fn farewell(panes: &HashMap<String, Watch>) -> Result<()> {
    let unfinished = panes
        .values()
        .filter(|w| w.recovery.state() != RecoveryState::Idle)
        .count();
    if unfinished > 0 {
        let attempts: u32 = panes.values().map(|w| w.recovery.attempt()).sum();
        emit(&log(
            LogLevel::Info,
            format!(
                "stopping with {unfinished} pane(s) mid-recovery after {attempts} resume attempt(s)"
            ),
        ))?;
    }
    Ok(())
}

fn on_host_event(panes: &mut HashMap<String, Watch>, event: &PluginEvent) -> Result<()> {
    let Some(token) = event.token().cloned() else {
        return Ok(());
    };
    let now = now_epoch();
    if let PluginEvent::PaneOpened {
        generation,
        command,
        cwd,
        ..
    } = event
    {
        open_pane(panes, &token, *generation, command.as_deref(), cwd)?;
    }
    let Some(watch) = panes.get_mut(&token) else {
        return Ok(());
    };
    // Housekeeping first: the adapter's answer below depends on whether the host
    // has just said this pane is alive or idle.
    let Some(commands) = watch.recovery.on_event(event) else {
        return Ok(()); // a generation this pane has already moved past
    };
    emit_all(&commands)?;
    watch.ctx.generation = watch.recovery.generation();
    let limit = match event {
        PluginEvent::PaneOutput { text, .. } => watch.provider.on_output(&watch.ctx, text, now),
        PluginEvent::PaneExited { .. } => {
            watch.provider.on_exit(&watch.ctx);
            None
        }
        _ => None,
    };
    if let Some(limit) = limit {
        let commands = watch.recovery.note_limit(limit, now, Instant::now());
        emit_all(&commands)?;
    }
    if matches!(event, PluginEvent::PaneClosed { .. }) {
        panes.remove(&token);
    }
    Ok(())
}

/// Start watching the pane `token` names when its configured command identifies
/// a provider this plugin supports.
fn open_pane(
    panes: &mut HashMap<String, Watch>,
    token: &str,
    generation: u32,
    command: Option<&str>,
    cwd: &str,
) -> Result<()> {
    let ctx = PaneContext {
        token: token.to_string(),
        generation,
        cwd: cwd.to_string(),
        command: command.map(str::to_string),
    };
    if let Some(watch) = panes.get_mut(token) {
        // A relaunch reopens the same slot. The recovery state survives, so an
        // attempt budget cannot be reset by relaunching into the same limit.
        watch.ctx = ctx;
        return Ok(());
    }
    let provider = detect(command);
    let Some(provider) = provider else {
        // A pane running something this plugin knows nothing about is not
        // watched at all, which is the cheapest way to stay out of it.
        return Ok(());
    };
    if panes.len() >= MAX_TRACKED_PANES {
        emit(&log(
            LogLevel::Warn,
            format!("already watching {MAX_TRACKED_PANES} panes; not watching another"),
        ))?;
        return Ok(());
    }
    emit(&log(
        LogLevel::Info,
        format!("watching a {} pane", provider.name()),
    ))?;
    panes.insert(
        token.to_string(),
        Watch {
            recovery: PaneRecovery::new(token.to_string(), generation),
            provider,
            ctx,
        },
    );
    Ok(())
}

fn tick(panes: &mut HashMap<String, Watch>) -> Result<()> {
    let epoch = now_epoch();
    let now = Instant::now();
    for watch in panes.values_mut() {
        if let Some(limit) = watch.provider.poll(&watch.ctx, epoch) {
            let commands = watch.recovery.note_limit(limit, epoch, now);
            emit_all(&commands)?;
        }
        let commands = watch
            .recovery
            .tick(watch.provider.as_ref(), &watch.ctx, epoch, now);
        emit_all(&commands)?;
    }
    Ok(())
}
