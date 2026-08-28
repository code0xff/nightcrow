use std::time::{Duration, Instant};

const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(16);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(1);
const WARNING_INTERVAL: Duration = Duration::from_secs(30);

pub(super) struct SpawnRetry {
    next_retry: Option<Instant>,
    retry_delay: Duration,
    next_warning: Option<Instant>,
}

impl Default for SpawnRetry {
    fn default() -> Self {
        Self {
            next_retry: None,
            retry_delay: INITIAL_RETRY_DELAY,
            next_warning: None,
        }
    }
}

impl SpawnRetry {
    pub(super) fn is_ready(&self, now: Instant) -> bool {
        self.next_retry.is_none_or(|deadline| now >= deadline)
    }

    pub(super) fn record_failure(&mut self, now: Instant) -> bool {
        self.next_retry = Some(now + self.retry_delay);
        self.retry_delay = self.retry_delay.saturating_mul(2).min(MAX_RETRY_DELAY);

        if self.next_warning.is_some_and(|deadline| now < deadline) {
            return false;
        }
        self.next_warning = Some(now + WARNING_INTERVAL);
        true
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}
