//! Process-local service lifecycle coordination for controlled drain/restart flows.
//!
//! Durable deployment truth belongs in a deployment receipt store. This module
//! intentionally owns only the live Server admission state: whether new
//! consequential tool calls are admitted, plus an optimistic generation fence.

use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ServiceLifecycleSnapshot {
    pub(crate) draining: bool,
    pub(crate) generation: u64,
    pub(crate) changed_at: i64,
}

impl ServiceLifecycleSnapshot {
    pub(crate) fn as_json(self) -> Value {
        json!({
            "draining": self.draining,
            "generation": self.generation,
            "changed_at": self.changed_at,
        })
    }
}

#[derive(Debug)]
struct ServiceLifecycleInner {
    draining: bool,
    generation: u64,
    changed_at: i64,
}

#[derive(Debug)]
pub(crate) struct ServiceLifecycleState {
    inner: Mutex<ServiceLifecycleInner>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ServiceLifecycleConflict {
    pub(crate) expected_generation: u64,
    pub(crate) actual: ServiceLifecycleSnapshot,
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0)
}

impl Default for ServiceLifecycleState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(ServiceLifecycleInner {
                draining: false,
                generation: 1,
                changed_at: unix_now(),
            }),
        }
    }
}

impl ServiceLifecycleState {
    pub(crate) fn snapshot(&self) -> ServiceLifecycleSnapshot {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        ServiceLifecycleSnapshot {
            draining: inner.draining,
            generation: inner.generation,
            changed_at: inner.changed_at,
        }
    }

    /// Desired-state transition with optimistic generation fencing.
    ///
    /// Repeating a request for the already-current desired state is a safe no-op
    /// even with an older generation. This makes a lost successful response safe
    /// to retry without granting stale callers authority to change state again.
    pub(crate) fn set_draining(
        &self,
        draining: bool,
        expected_generation: u64,
    ) -> Result<(ServiceLifecycleSnapshot, bool), ServiceLifecycleConflict> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.draining == draining {
            return Ok((
                ServiceLifecycleSnapshot {
                    draining: inner.draining,
                    generation: inner.generation,
                    changed_at: inner.changed_at,
                },
                false,
            ));
        }
        if expected_generation != inner.generation {
            return Err(ServiceLifecycleConflict {
                expected_generation,
                actual: ServiceLifecycleSnapshot {
                    draining: inner.draining,
                    generation: inner.generation,
                    changed_at: inner.changed_at,
                },
            });
        }
        inner.draining = draining;
        inner.generation = inner.generation.saturating_add(1);
        inner.changed_at = unix_now();
        Ok((
            ServiceLifecycleSnapshot {
                draining: inner.draining,
                generation: inner.generation,
                changed_at: inner.changed_at,
            },
            true,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lost_success_retry_is_idempotent_but_stale_opposite_transition_fails() {
        let state = ServiceLifecycleState::default();
        let initial = state.snapshot();
        let (draining, changed) = state.set_draining(true, initial.generation).unwrap();
        assert!(changed);
        assert!(draining.draining);

        let (retry, retry_changed) = state.set_draining(true, initial.generation).unwrap();
        assert!(!retry_changed);
        assert_eq!(retry.generation, draining.generation);

        let conflict = state
            .set_draining(false, initial.generation)
            .expect_err("stale opposite transition must fail closed");
        assert_eq!(conflict.expected_generation, initial.generation);
        assert_eq!(conflict.actual.generation, draining.generation);
    }
}
