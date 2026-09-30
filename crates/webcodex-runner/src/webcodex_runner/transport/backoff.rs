//! Bounded polling and stream reconnect backoff.

use super::*;

#[derive(Debug, Clone)]
pub(super) struct PollingIdleBackoff {
    pub(super) initial: Duration,
    pub(super) next_step: usize,
    pub(super) first: bool,
}

impl PollingIdleBackoff {
    pub(super) fn new(initial: Duration) -> Self {
        Self {
            initial,
            next_step: 0,
            first: true,
        }
    }

    pub(super) fn reset(&mut self) {
        self.next_step = 0;
        self.first = true;
    }

    pub(super) fn next_delay(&mut self) -> Duration {
        if self.first {
            self.first = false;
            return self.initial;
        }
        while let Some(step) = POLLING_IDLE_BACKOFF_STEPS.get(self.next_step).copied() {
            self.next_step += 1;
            if step > self.initial {
                return step;
            }
        }
        self.initial.max(
            *POLLING_IDLE_BACKOFF_STEPS
                .last()
                .expect("polling idle backoff is non-empty"),
        )
    }
}

pub(super) fn polling_idle_delay(
    backoff: &mut PollingIdleBackoff,
    ran_request: bool,
) -> Option<Duration> {
    if ran_request {
        backoff.reset();
        None
    } else {
        Some(backoff.next_delay())
    }
}

#[derive(Debug, Clone)]
pub(super) struct RetryBackoff {
    pub(super) attempts: usize,
    pub(super) steps: &'static [Duration],
}

impl RetryBackoff {
    pub(super) fn new(steps: &'static [Duration]) -> Self {
        Self { attempts: 0, steps }
    }

    pub(super) fn reset(&mut self) {
        self.attempts = 0;
    }

    pub(super) fn next_delay(&mut self) -> Duration {
        let delay = self
            .steps
            .get(self.attempts)
            .copied()
            .unwrap_or_else(|| *self.steps.last().expect("retry backoff is non-empty"));
        self.attempts = self.attempts.saturating_add(1);
        delay
    }
}

pub(super) fn next_lease_conflict_delay(
    backoff: &mut RetryBackoff,
    elapsed: Duration,
) -> Option<Duration> {
    if elapsed >= POLLING_LEASE_CONFLICT_MAX_WAIT {
        return None;
    }
    Some(
        backoff
            .next_delay()
            .min(POLLING_LEASE_CONFLICT_MAX_WAIT.saturating_sub(elapsed)),
    )
}

pub(super) fn format_delay(delay: Duration) -> String {
    if delay.as_millis().is_multiple_of(1000) {
        format!("{}s", delay.as_secs())
    } else {
        format!("{}ms", delay.as_millis())
    }
}

pub(super) fn schedule_reconnect(transport: &str, backoff: &mut RetryBackoff) -> Duration {
    let delay = backoff.next_delay();
    eprintln!(
        "webcodex-runner reconnect attempt scheduled transport={} delay={}",
        transport,
        format_delay(delay)
    );
    tracing::debug!(
        transport,
        delay_ms = delay.as_millis() as u64,
        "webcodex-runner reconnect attempt scheduled"
    );
    delay
}

pub(super) fn reset_backoff_after_stable_session(backoff: &mut RetryBackoff, started_at: Instant) {
    if started_at.elapsed() >= RECONNECT_STABLE_RESET_AFTER {
        backoff.reset();
    }
}
