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
impl Health {
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
