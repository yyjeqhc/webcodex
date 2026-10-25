use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Arc,
};

/// Safe, payload-free process-local observations. Readiness requires a successful poll.
#[derive(Clone, Default)]
pub struct Health(pub(crate) Arc<State>);
#[derive(Default)]
pub(crate) struct State {
    pub ready: AtomicBool,
    /// Elapsed milliseconds until first successful control-plane poll (+1).
    pub first_poll_ms_plus_one: AtomicU64,
    /// Total requests and HTTP time before the first successful poll.
    pub first_poll_attempts: AtomicU64,
    pub first_poll_request_ms: AtomicU64,
    pub unsettled: AtomicUsize,
    pub rejected: AtomicUsize,
    enqueue_timestamp_valid: AtomicU64,
    enqueue_timestamp_unverifiable: AtomicU64,
}
/// A payload-free observation for embedding hosts and the standalone daemon.
/// Counters are sampled independently; this is not an execution/replay ledger.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HealthSnapshot {
    pub schema_version: u16,
    /// A recent poll succeeded; does not assert end-to-end MCP readiness.
    pub ready: bool,
    /// Includes both currently executing and unconfirmed exchanges.
    pub unsettled_commands: usize,
    pub rejected_commands: usize,
}
impl Health {
    /// Payload-free preflight evidence; valid syntax alone does not establish
    /// the control plane's truthfulness or the operator's clock attestation.
    pub fn enqueue_timestamp_counts(&self) -> (u64, u64) {
        (
            self.0.enqueue_timestamp_valid.load(Ordering::Relaxed),
            self.0
                .enqueue_timestamp_unverifiable
                .load(Ordering::Relaxed),
        )
    }

    pub(crate) fn record_enqueue_timestamp(&self, valid: bool) {
        let counter = if valid {
            &self.0.enqueue_timestamp_valid
        } else {
            &self.0.enqueue_timestamp_unverifiable
        };
        let mut current = counter.load(Ordering::Relaxed);
        while current != u64::MAX {
            match counter.compare_exchange_weak(
                current,
                current + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
    }

    pub fn snapshot(&self) -> HealthSnapshot {
        HealthSnapshot {
            schema_version: 1,
            ready: self.is_ready(),
            unsettled_commands: self.0.unsettled.load(Ordering::SeqCst),
            rejected_commands: self.rejected_commands(),
        }
    }
    pub fn is_ready(&self) -> bool {
        self.0.ready.load(Ordering::SeqCst)
    }
    /// Payload-free, monotonic duration to the first successful remote poll.
    pub fn initial_poll_ms(&self) -> Option<u64> {
        self.0
            .first_poll_ms_plus_one
            .load(Ordering::SeqCst)
            .checked_sub(1)
    }
    /// The additional values distinguish a single server-held poll from retries.
    pub fn initial_poll_details(&self) -> Option<(u64, u64)> {
        self.initial_poll_ms()?;
        Some((
            self.0.first_poll_attempts.load(Ordering::SeqCst),
            self.0.first_poll_request_ms.load(Ordering::SeqCst),
        ))
    }
    pub(crate) fn record_initial_poll(
        &self,
        elapsed: std::time::Duration,
        attempts: u64,
        request_time: std::time::Duration,
    ) {
        let millis = elapsed.as_millis().min(u64::MAX as u128 - 1) as u64;
        let request_ms = request_time.as_millis().min(millis as u128) as u64;
        self.0.first_poll_attempts.store(attempts, Ordering::SeqCst);
        self.0
            .first_poll_request_ms
            .store(request_ms, Ordering::SeqCst);
        let _ = self.0.first_poll_ms_plus_one.compare_exchange(
            0,
            millis + 1,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    }
    pub fn has_uncertain_work(&self) -> bool {
        self.0.unsettled.load(Ordering::SeqCst) != 0
    }
    pub fn rejected_commands(&self) -> usize {
        self.0.rejected.load(Ordering::Relaxed)
    }
    pub(crate) fn ready(&self, ready: bool) {
        self.0.ready.store(ready, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_counters_saturate_independently() {
        let health = Health::default();
        health
            .0
            .enqueue_timestamp_valid
            .store(u64::MAX - 1, Ordering::Relaxed);
        health.record_enqueue_timestamp(true);
        health.record_enqueue_timestamp(true);
        health.record_enqueue_timestamp(false);
        assert_eq!(health.enqueue_timestamp_counts(), (u64::MAX, 1));
        health
            .0
            .enqueue_timestamp_unverifiable
            .store(u64::MAX, Ordering::Relaxed);
        health.record_enqueue_timestamp(false);
        assert_eq!(health.enqueue_timestamp_counts(), (u64::MAX, u64::MAX));
    }
}
