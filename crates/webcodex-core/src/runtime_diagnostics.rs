//! Secrets-safe bounded runtime diagnostic ring.
//!
//! This is intentionally *not* a raw log buffer. Call sites can only record
//! code-owned component/code identifiers plus an optional strictly token-shaped
//! correlation id. Messages, headers, env values, argv, file paths and arbitrary
//! tracing fields never enter this store.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const RUNTIME_DIAGNOSTIC_CAPACITY: usize = 1_000;
pub const RUNTIME_DIAGNOSTIC_MAX_QUERY: usize = 200;
const MAX_TOKEN_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warn,
    Error,
}

impl DiagnosticSeverity {
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "info" => Some(Self::Info),
            "warn" => Some(Self::Warn),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDiagnosticEvent {
    pub sequence: u64,
    pub observed_at: i64,
    pub severity: DiagnosticSeverity,
    pub component: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeDiagnosticFilter<'a> {
    pub severity: Option<DiagnosticSeverity>,
    pub component: Option<&'a str>,
    pub since: Option<i64>,
    pub correlation_id: Option<&'a str>,
    pub until: Option<i64>,
    pub limit: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDiagnosticStats {
    pub buffered_count: usize,
    pub dropped_count: u64,
    pub oldest_sequence: Option<u64>,
    pub newest_sequence: Option<u64>,
    pub info_count: usize,
    pub warn_count: usize,
    pub error_count: usize,
    pub newest_warn_sequence: Option<u64>,
    pub newest_error_sequence: Option<u64>,
}

#[derive(Default)]
struct DiagnosticRing {
    next_sequence: u64,
    events: VecDeque<RuntimeDiagnosticEvent>,
}

fn ring() -> &'static Mutex<DiagnosticRing> {
    static RING: OnceLock<Mutex<DiagnosticRing>> = OnceLock::new();
    RING.get_or_init(|| Mutex::new(DiagnosticRing::default()))
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0)
}

fn safe_token(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_TOKEN_CHARS
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte))
}

/// Record one code-owned diagnostic event.
///
/// `component` and `code` are `'static` so arbitrary runtime strings cannot be
/// accidentally captured. An invalid correlation id is silently omitted.
pub fn record(
    severity: DiagnosticSeverity,
    component: &'static str,
    code: &'static str,
    correlation_id: Option<&str>,
) {
    debug_assert!(safe_token(component));
    debug_assert!(safe_token(code));
    if !safe_token(component) || !safe_token(code) {
        return;
    }
    let correlation_id = correlation_id
        .filter(|value| safe_token(value))
        .map(str::to_string);
    let mut ring = ring()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    ring.next_sequence = ring.next_sequence.saturating_add(1);
    let sequence = ring.next_sequence;
    if ring.events.len() >= RUNTIME_DIAGNOSTIC_CAPACITY {
        ring.events.pop_front();
    }
    ring.events.push_back(RuntimeDiagnosticEvent {
        sequence,
        observed_at: now_ts(),
        severity,
        component: component.to_string(),
        code: code.to_string(),
        correlation_id,
    });
}

pub fn snapshot(filter: RuntimeDiagnosticFilter<'_>) -> Vec<RuntimeDiagnosticEvent> {
    let limit = filter.limit.clamp(1, RUNTIME_DIAGNOSTIC_MAX_QUERY);
    let ring = ring()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut selected = ring
        .events
        .iter()
        .rev()
        .filter(|event| {
            filter
                .severity
                .is_none_or(|severity| event.severity == severity)
                && filter
                    .component
                    .is_none_or(|component| event.component == component)
                && filter.since.is_none_or(|since| event.observed_at >= since)
                && filter.until.is_none_or(|until| event.observed_at <= until)
                && filter.correlation_id.is_none_or(|correlation_id| {
                    event.correlation_id.as_deref() == Some(correlation_id)
                })
        })
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    selected.reverse();
    selected
}

pub fn stats() -> RuntimeDiagnosticStats {
    let ring = ring()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let info_count = ring
        .events
        .iter()
        .filter(|event| event.severity == DiagnosticSeverity::Info)
        .count();
    let warn_count = ring
        .events
        .iter()
        .filter(|event| event.severity == DiagnosticSeverity::Warn)
        .count();
    let error_count = ring
        .events
        .iter()
        .filter(|event| event.severity == DiagnosticSeverity::Error)
        .count();
    let newest_warn_sequence = ring
        .events
        .iter()
        .rev()
        .find(|event| event.severity == DiagnosticSeverity::Warn)
        .map(|event| event.sequence);
    let newest_error_sequence = ring
        .events
        .iter()
        .rev()
        .find(|event| event.severity == DiagnosticSeverity::Error)
        .map(|event| event.sequence);
    RuntimeDiagnosticStats {
        buffered_count: ring.events.len(),
        dropped_count: ring.next_sequence.saturating_sub(ring.events.len() as u64),
        oldest_sequence: ring.events.front().map(|event| event.sequence),
        newest_sequence: ring.events.back().map(|event| event.sequence),
        info_count,
        warn_count,
        error_count,
        newest_warn_sequence,
        newest_error_sequence,
    }
}

pub fn count() -> usize {
    ring()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .events
        .len()
}

#[cfg(test)]
pub fn clear_for_test() {
    let mut ring = ring()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *ring = DiagnosticRing::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_is_bounded_filterable_and_drops_unsafe_correlation() {
        clear_for_test();
        record(
            DiagnosticSeverity::Warn,
            "runner_registry",
            "job_recovery_started",
            Some("wc_job_abc-1"),
        );
        record(
            DiagnosticSeverity::Error,
            "gateway",
            "provider_timeout",
            Some("Bearer secret value"),
        );
        let all = snapshot(RuntimeDiagnosticFilter {
            limit: 10,
            ..Default::default()
        });
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].correlation_id.as_deref(), Some("wc_job_abc-1"));
        assert_eq!(all[1].correlation_id, None);
        let errors = snapshot(RuntimeDiagnosticFilter {
            severity: Some(DiagnosticSeverity::Error),
            limit: 10,
            ..Default::default()
        });
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "provider_timeout");
        let correlated = snapshot(RuntimeDiagnosticFilter {
            correlation_id: Some("wc_job_abc-1"),
            limit: 10,
            ..Default::default()
        });
        assert_eq!(correlated.len(), 1);
        assert_eq!(correlated[0].component, "runner_registry");
        let timestamp = all[0].observed_at;
        let ranged = snapshot(RuntimeDiagnosticFilter {
            since: Some(timestamp),
            until: Some(timestamp),
            limit: 10,
            ..Default::default()
        });
        assert!(!ranged.is_empty());
        assert!(ranged.iter().all(|event| event.observed_at == timestamp));
        let stats = stats();
        assert_eq!(stats.buffered_count, 2);
        assert_eq!(stats.dropped_count, 0);
        assert_eq!(stats.oldest_sequence, Some(1));
        assert_eq!(stats.newest_sequence, Some(2));
        assert_eq!(stats.info_count, 0);
        assert_eq!(stats.warn_count, 1);
        assert_eq!(stats.error_count, 1);
        assert_eq!(stats.newest_warn_sequence, Some(1));
        assert_eq!(stats.newest_error_sequence, Some(2));
    }
}
