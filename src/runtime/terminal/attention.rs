//! Client-local attention derived from terminal activity.
//!
//! The daemon owns panes, but whether this client has looked at an event is
//! local UI state. A project tab therefore carries one unread bit, not a
//! session-wide acknowledgement that another attached screen could clear.

use super::TerminalState;
use crate::backend::PaneId;
use std::time::{Duration, Instant};

/// Title changes must stay close enough to be one animation.
const TITLE_CHANGE_GAP: Duration = Duration::from_secs(1);
/// A short pair of ordinary title updates is not evidence of background work.
const TITLE_ACTIVITY_MIN_DURATION: Duration = Duration::from_millis(600);
/// Silence after an animated title means the activity has settled.
const TITLE_SETTLE_DELAY: Duration = Duration::from_millis(800);
const TITLE_ACTIVITY_MIN_CHANGES: u8 = 3;

pub(super) struct TitleActivity {
    first_change: Instant,
    last_change: Instant,
    changes: u8,
}

impl TitleActivity {
    fn new(now: Instant) -> Self {
        Self {
            first_change: now,
            last_change: now,
            changes: 1,
        }
    }

    fn record(&mut self, now: Instant) {
        if now.saturating_duration_since(self.last_change) > TITLE_CHANGE_GAP {
            *self = Self::new(now);
            return;
        }
        self.last_change = now;
        self.changes = self.changes.saturating_add(1);
    }

    fn settled_attention(&self, now: Instant) -> Option<bool> {
        if now.saturating_duration_since(self.last_change) < TITLE_SETTLE_DELAY {
            return None;
        }
        Some(
            self.changes >= TITLE_ACTIVITY_MIN_CHANGES
                && self
                    .last_change
                    .saturating_duration_since(self.first_change)
                    >= TITLE_ACTIVITY_MIN_DURATION,
        )
    }
}

impl TerminalState {
    pub(super) fn note_title_change(&mut self, pane: PaneId, now: Instant) {
        self.title_activity
            .entry(pane)
            .and_modify(|activity| activity.record(now))
            .or_insert_with(|| TitleActivity::new(now));
    }

    pub(super) fn settle_title_attention(&mut self, now: Instant) -> bool {
        let mut attention = false;
        self.title_activity.retain(|_, activity| {
            let Some(settled) = activity.settled_attention(now) else {
                return true;
            };
            attention |= settled;
            false
        });
        self.unread_attention |= attention;
        attention
    }

    pub(crate) fn raise_attention(&mut self) {
        self.unread_attention = true;
    }

    pub fn has_unread_attention(&self) -> bool {
        self.unread_attention
    }

    pub fn acknowledge_attention(&mut self) {
        self.unread_attention = false;
        // Activity already visible on this screen must not settle into a new
        // unread event after the user switches away; later title changes
        // start a fresh observation.
        self.title_activity.clear();
    }
}
