//! One pass over every pane's queued events: what it reports, and what it leaves
//! for the next pass.

use super::{
    BackendEvent, DrainWindow, EXIT_DRAIN_GRACE, ExitPhase, PER_PANE_DRAIN_BUDGET, PtyBackend,
    PtyEvent,
};
use std::time::Instant;

impl PtyBackend {
    /// Drain every pane up to [`PER_PANE_DRAIN_BUDGET`] events and record whether
    /// any pane was left with more.
    ///
    /// Pane removal is the caller's responsibility (`App::poll_terminal` calls
    /// `destroy_pane` on `Exited`). Doing it here too created a dual-ownership race
    /// where reader-thread events queued after `destroy_pane` were silently dropped,
    /// and where `Exited` could be reported twice.
    ///
    /// The reader thread emits all `Output` messages, then a single `Exited` as the
    /// last message before its sender drops, so the mpsc order means no separate
    /// post-`Exited` drain is needed. `ChildExited` carries no such ordering — it can
    /// overtake output already written — so it only moves the pane to `Draining`.
    pub(super) fn drain_queued(&mut self) -> Vec<BackendEvent> {
        // Each pane is drained up to the budget so one noisy pane (e.g.
        // `yes | head -100000`) cannot starve its siblings within a frame.
        // Spending the budget leaves events queued, and the reader's signal for
        // them was consumed by the pass that got here. A woken owner is asked back
        // once the pane's drain window ends (see `next_deadline`); a polling owner
        // comes back on its own tick.
        // Ahead of any output: a pane has to exist before bytes can be routed to
        // it, and both can be queued before the same drain.
        let mut events: Vec<BackendEvent> = std::mem::take(&mut self.created);
        let now = Instant::now();
        let paced = self.waker.is_some();
        for (id, pane) in &mut self.panes {
            let mut budget = PER_PANE_DRAIN_BUDGET;
            if paced {
                let window = match pane.window {
                    Some(window) if now < window.end() => window,
                    _ => DrainWindow {
                        start: now,
                        used: 0,
                    },
                };
                pane.window = Some(window);
                budget -= window.used;
                if budget == 0 {
                    continue;
                }
            }
            let allowed = budget;
            while budget > 0 {
                match pane
                    .rx
                    .as_ref()
                    .expect("live pane has output receiver")
                    .try_recv()
                {
                    Ok(PtyEvent::Output(data)) => {
                        // One timestamp for the whole drain: the bytes arrived
                        // between this tick and the last, and a per-event clock
                        // read would claim a precision the 8 ms poll does not
                        // have.
                        self.slots.mark_output(*id, now);
                        events.push(BackendEvent::Output { pane: *id, data });
                    }
                    Ok(PtyEvent::ChildExited) => {
                        if pane.exit == ExitPhase::Running {
                            pane.exit = ExitPhase::Draining { since: now };
                        }
                    }
                    Ok(PtyEvent::Exited) => {
                        if pane.exit != ExitPhase::Reported {
                            pane.exit = ExitPhase::Reported;
                            events.push(BackendEvent::Exited { pane: *id });
                        }
                        break;
                    }
                    Err(_) => {
                        // Dry: for a dead child past its drain grace, the cue
                        // to report.
                        if let ExitPhase::Draining { since } = pane.exit
                            && now.duration_since(since) >= EXIT_DRAIN_GRACE
                        {
                            pane.exit = ExitPhase::Reported;
                            events.push(BackendEvent::Exited { pane: *id });
                        }
                        break;
                    }
                }
                budget -= 1;
            }
            if let Some(window) = pane.window.as_mut() {
                window.used += allowed - budget;
            }
        }
        events
    }
}
