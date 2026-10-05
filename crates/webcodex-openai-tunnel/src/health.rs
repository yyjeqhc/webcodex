use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

/// Safe, payload-free process-local observations. Readiness requires a successful poll.
#[derive(Clone, Default)]
pub struct Health(pub(crate) Arc<State>);
#[derive(Default)]
pub(crate) struct State {
    pub ready: AtomicBool,
    pub unsettled: AtomicUsize,
    pub rejected: AtomicUsize,
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
