//! Bounded per-provider gateway circuit breaker and bulkhead.
//!
//! This is intentionally process-local and transport-focused. It never retries,
//! retargets, or changes a request's dispatch semantics. Admission happens before
//! Runner enqueue, so open/saturated rejections are definitively NotStarted.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const FAILURE_THRESHOLD: u32 = 3;
const BASE_COOLDOWN: Duration = Duration::from_secs(15);
const JITTER_MAX_SECS: u64 = 5;
const MAX_IN_FLIGHT_PER_KEY: u32 = 8;
const MAX_TRACKED_KEYS: usize = 512;
const IDLE_RETENTION: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GatewayAdmissionKind {
    Normal,
    HalfOpenProbe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GatewayAdmission {
    pub(crate) kind: GatewayAdmissionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GatewayAdmissionRejection {
    CircuitOpen { retry_after_secs: u64 },
    Saturated { in_flight: u32, limit: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GatewayTimeoutOutcome {
    pub(crate) circuit_opened: bool,
    pub(crate) retry_after_secs: Option<u64>,
}

#[derive(Debug, Clone)]
struct BreakerEntry {
    consecutive_timeouts: u32,
    open_until: Option<Instant>,
    half_open_probe_in_flight: bool,
    in_flight: u32,
    last_touched: Instant,
}

impl BreakerEntry {
    fn new(now: Instant) -> Self {
        Self {
            consecutive_timeouts: 0,
            open_until: None,
            half_open_probe_in_flight: false,
            in_flight: 0,
            last_touched: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct GatewayCircuitBreakerStats {
    pub(crate) tracked_keys: usize,
    pub(crate) open_circuits: usize,
    pub(crate) half_open_probes: usize,
    pub(crate) total_in_flight: u64,
    pub(crate) saturated_keys: usize,
}

#[derive(Debug, Default)]
pub(crate) struct GatewayCircuitBreaker {
    entries: Mutex<HashMap<String, BreakerEntry>>,
}

fn stable_jitter_secs(key: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash % (JITTER_MAX_SECS + 1)
}

fn cooldown_for(key: &str) -> Duration {
    BASE_COOLDOWN + Duration::from_secs(stable_jitter_secs(key))
}

fn ceil_seconds(duration: Duration) -> u64 {
    let millis = duration.as_millis();
    ((millis + 999) / 1000).max(1).min(u64::MAX as u128) as u64
}

impl GatewayCircuitBreaker {
    pub(crate) fn admit(&self, key: &str) -> Result<GatewayAdmission, GatewayAdmissionRejection> {
        self.admit_at(key, Instant::now())
    }

    fn admit_at(
        &self,
        key: &str,
        now: Instant,
    ) -> Result<GatewayAdmission, GatewayAdmissionRejection> {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::prune_if_needed(&mut entries, now, key);
        let entry = entries
            .entry(key.to_string())
            .or_insert_with(|| BreakerEntry::new(now));
        entry.last_touched = now;

        if let Some(open_until) = entry.open_until {
            if now < open_until {
                return Err(GatewayAdmissionRejection::CircuitOpen {
                    retry_after_secs: ceil_seconds(open_until.duration_since(now)),
                });
            }
            if entry.half_open_probe_in_flight {
                return Err(GatewayAdmissionRejection::CircuitOpen {
                    retry_after_secs: 1,
                });
            }
            if entry.in_flight >= MAX_IN_FLIGHT_PER_KEY {
                return Err(GatewayAdmissionRejection::Saturated {
                    in_flight: entry.in_flight,
                    limit: MAX_IN_FLIGHT_PER_KEY,
                });
            }
            entry.half_open_probe_in_flight = true;
            entry.in_flight = entry.in_flight.saturating_add(1);
            return Ok(GatewayAdmission {
                kind: GatewayAdmissionKind::HalfOpenProbe,
            });
        }

        if entry.in_flight >= MAX_IN_FLIGHT_PER_KEY {
            return Err(GatewayAdmissionRejection::Saturated {
                in_flight: entry.in_flight,
                limit: MAX_IN_FLIGHT_PER_KEY,
            });
        }
        entry.in_flight = entry.in_flight.saturating_add(1);
        Ok(GatewayAdmission {
            kind: GatewayAdmissionKind::Normal,
        })
    }

    /// A response or immediate non-timeout failure proves the gateway transport
    /// is responsive. Clear timeout history and close any half-open circuit.
    /// Returns true only when this transition closed a previously-open circuit.
    pub(crate) fn record_responsive(&self, key: &str) -> bool {
        self.record_responsive_at(key, Instant::now())
    }

    fn record_responsive_at(&self, key: &str, now: Instant) -> bool {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(entry) = entries.get_mut(key) else {
            return false;
        };
        let was_open = entry.open_until.is_some() || entry.half_open_probe_in_flight;
        entry.in_flight = entry.in_flight.saturating_sub(1);
        entry.consecutive_timeouts = 0;
        entry.open_until = None;
        entry.half_open_probe_in_flight = false;
        entry.last_touched = now;
        if entry.in_flight == 0 {
            entries.remove(key);
        }
        was_open
    }

    pub(crate) fn record_timeout(&self, key: &str) -> GatewayTimeoutOutcome {
        self.record_timeout_at(key, Instant::now())
    }

    fn record_timeout_at(&self, key: &str, now: Instant) -> GatewayTimeoutOutcome {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = entries
            .entry(key.to_string())
            .or_insert_with(|| BreakerEntry::new(now));
        let was_half_open = entry.half_open_probe_in_flight;
        entry.in_flight = entry.in_flight.saturating_sub(1);
        entry.half_open_probe_in_flight = false;
        entry.consecutive_timeouts = entry.consecutive_timeouts.saturating_add(1);
        entry.last_touched = now;

        if was_half_open || entry.consecutive_timeouts >= FAILURE_THRESHOLD {
            let cooldown = cooldown_for(key);
            entry.open_until = Some(now + cooldown);
            entry.consecutive_timeouts = FAILURE_THRESHOLD;
            return GatewayTimeoutOutcome {
                circuit_opened: true,
                retry_after_secs: Some(ceil_seconds(cooldown)),
            };
        }
        GatewayTimeoutOutcome {
            circuit_opened: false,
            retry_after_secs: None,
        }
    }

    pub(crate) fn stats(&self) -> GatewayCircuitBreakerStats {
        let now = Instant::now();
        let entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        GatewayCircuitBreakerStats {
            tracked_keys: entries.len(),
            open_circuits: entries
                .values()
                .filter(|entry| entry.open_until.is_some_and(|deadline| deadline > now))
                .count(),
            half_open_probes: entries
                .values()
                .filter(|entry| entry.half_open_probe_in_flight)
                .count(),
            total_in_flight: entries
                .values()
                .map(|entry| u64::from(entry.in_flight))
                .sum(),
            saturated_keys: entries
                .values()
                .filter(|entry| entry.in_flight >= MAX_IN_FLIGHT_PER_KEY)
                .count(),
        }
    }

    fn prune_if_needed(entries: &mut HashMap<String, BreakerEntry>, now: Instant, incoming: &str) {
        if entries.contains_key(incoming) || entries.len() < MAX_TRACKED_KEYS {
            return;
        }
        entries.retain(|_, entry| {
            entry.in_flight > 0
                || entry.open_until.is_some_and(|deadline| deadline > now)
                || now.saturating_duration_since(entry.last_touched) < IDLE_RETENTION
        });
        if entries.len() < MAX_TRACKED_KEYS {
            return;
        }
        let oldest_idle = entries
            .iter()
            .filter(|(_, entry)| entry.in_flight == 0)
            .min_by_key(|(_, entry)| entry.last_touched)
            .map(|(key, _)| key.clone());
        if let Some(key) = oldest_idle {
            entries.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_timeouts_open_then_one_half_open_probe_closes_on_success() {
        let breaker = GatewayCircuitBreaker::default();
        let key = "runner|provider|instance";
        let start = Instant::now();
        for offset in 0..2 {
            assert_eq!(
                breaker
                    .admit_at(key, start + Duration::from_secs(offset))
                    .unwrap()
                    .kind,
                GatewayAdmissionKind::Normal
            );
            let outcome = breaker.record_timeout_at(key, start + Duration::from_secs(offset));
            assert!(!outcome.circuit_opened);
        }
        breaker
            .admit_at(key, start + Duration::from_secs(2))
            .unwrap();
        let opened = breaker.record_timeout_at(key, start + Duration::from_secs(2));
        assert!(opened.circuit_opened);
        let cooldown = cooldown_for(key);
        assert!(matches!(
            breaker.admit_at(key, start + Duration::from_secs(3)),
            Err(GatewayAdmissionRejection::CircuitOpen { .. })
        ));

        let probe_at = start + Duration::from_secs(2) + cooldown;
        assert_eq!(
            breaker.admit_at(key, probe_at).unwrap().kind,
            GatewayAdmissionKind::HalfOpenProbe
        );
        assert!(matches!(
            breaker.admit_at(key, probe_at),
            Err(GatewayAdmissionRejection::CircuitOpen { .. })
        ));
        assert!(breaker.record_responsive_at(key, probe_at));
        assert_eq!(
            breaker.admit_at(key, probe_at).unwrap().kind,
            GatewayAdmissionKind::Normal
        );
    }

    #[test]
    fn per_provider_bulkhead_rejects_ninth_in_flight_call() {
        let breaker = GatewayCircuitBreaker::default();
        let key = "runner|provider|instance";
        let now = Instant::now();
        for _ in 0..MAX_IN_FLIGHT_PER_KEY {
            breaker.admit_at(key, now).unwrap();
        }
        assert_eq!(
            breaker.admit_at(key, now),
            Err(GatewayAdmissionRejection::Saturated {
                in_flight: MAX_IN_FLIGHT_PER_KEY,
                limit: MAX_IN_FLIGHT_PER_KEY,
            })
        );
        breaker.record_responsive_at(key, now);
        assert!(breaker.admit_at(key, now).is_ok());
    }

    #[test]
    fn stats_report_open_and_saturated_provider_state() {
        let breaker = GatewayCircuitBreaker::default();
        let now = Instant::now();
        let open = "runner|provider-open|instance";
        for _ in 0..FAILURE_THRESHOLD {
            breaker.admit_at(open, now).unwrap();
            breaker.record_timeout_at(open, now);
        }
        let saturated = "runner|provider-saturated|instance";
        for _ in 0..MAX_IN_FLIGHT_PER_KEY {
            breaker.admit_at(saturated, now).unwrap();
        }
        let stats = breaker.stats();
        assert_eq!(stats.tracked_keys, 2);
        assert_eq!(stats.open_circuits, 1);
        assert_eq!(stats.half_open_probes, 0);
        assert_eq!(stats.total_in_flight, u64::from(MAX_IN_FLIGHT_PER_KEY));
        assert_eq!(stats.saturated_keys, 1);
    }

    #[test]
    fn provider_keys_are_isolated_and_jitter_is_stable() {
        let breaker = GatewayCircuitBreaker::default();
        let now = Instant::now();
        let a = "runner|provider-a|instance-a";
        let b = "runner|provider-b|instance-b";
        for _ in 0..FAILURE_THRESHOLD {
            breaker.admit_at(a, now).unwrap();
            breaker.record_timeout_at(a, now);
        }
        assert!(matches!(
            breaker.admit_at(a, now),
            Err(GatewayAdmissionRejection::CircuitOpen { .. })
        ));
        assert!(breaker.admit_at(b, now).is_ok());
        assert_eq!(stable_jitter_secs(a), stable_jitter_secs(a));
        assert!(cooldown_for(a) >= BASE_COOLDOWN);
        assert!(cooldown_for(a) <= BASE_COOLDOWN + Duration::from_secs(JITTER_MAX_SECS));
    }
}
