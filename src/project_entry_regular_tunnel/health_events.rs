//! Sampling/readiness freshness and human service logging have different
//! lifetimes. Parent-owned Desktop pipes keep the existing 2s heartbeat;
//! persistent processes emit changes immediately and one 60s summary otherwise.
use std::time::{Duration, Instant};

pub(super) struct HealthEvents {
    parent_heartbeat: bool,
    last: Option<(Instant, bool, bool)>,
}
impl HealthEvents {
    pub(super) fn new(parent_heartbeat: bool) -> Self {
        Self {
            parent_heartbeat,
            last: None,
        }
    }
    pub(super) fn should_emit(&mut self, now: Instant, tunnel: bool, local: bool) -> bool {
        let emit = self.parent_heartbeat
            || self.last.is_none_or(|(at, old_tunnel, old_local)| {
                old_tunnel != tunnel
                    || old_local != local
                    || now.saturating_duration_since(at) >= Duration::from_secs(60)
            });
        if emit {
            self.last = Some((now, tunnel, local));
        }
        emit
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persistent_health_emits_changes_and_fixed_heartbeat_without_resetting_on_progress() {
        let start = Instant::now();
        let mut events = HealthEvents::new(false);
        assert!(events.should_emit(start, true, true));
        for sec in (2..60).step_by(2) {
            assert!(!events.should_emit(start + Duration::from_secs(sec), true, true));
        }
        assert!(events.should_emit(start + Duration::from_secs(60), true, true));
        assert!(events.should_emit(start + Duration::from_secs(62), false, true));
        assert!(events.should_emit(start + Duration::from_secs(64), true, true));
        assert!(!events.should_emit(start + Duration::from_secs(66), true, true));
    }
    #[test]
    fn desktop_parent_stream_never_loses_its_freshness_heartbeat() {
        let start = Instant::now();
        let mut events = HealthEvents::new(true);
        for sec in (0..120).step_by(2) {
            assert!(events.should_emit(start + Duration::from_secs(sec), true, true));
        }
    }
}
