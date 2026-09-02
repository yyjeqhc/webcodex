//! Cross-surface runtime observations.
//!
//! Two facts back the `connector_endpoint` and `last_successful_tool_call`
//! connection layers instead of guesses:
//! - connector endpoint activity (readiness probes and successful requests);
//! - the last successful *meaningful* tool call, scoped by principal,
//!   project, surface, and session.
//!
//! Never stores tool arguments, output bodies, command text, or secrets.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Bounded number of retained tool-call observations.
const MAX_TOOL_CALL_OBSERVATIONS: usize = 64;

/// Observability/status tools whose success must not refresh "meaningful
/// activity". Otherwise a read-only status poller keeps
/// `last_successful_tool_call` permanently fresh and the layer never goes
/// stale. Real inspection/edit/shell/git/session work is meaningful.
pub(crate) const NON_MEANINGFUL_ACTIVITY_TOOLS: &[&str] = &[
    "runtime_status",
    "list_tools",
    "list_agents",
    "list_projects",
    "tool_manifest",
    "read_tool_trace",
];

pub(crate) fn is_meaningful_activity_tool(tool_name: &str) -> bool {
    !NON_MEANINGFUL_ACTIVITY_TOOLS.contains(&tool_name)
}

/// One successful meaningful tool call. Scope fields only — no payloads.
#[derive(Debug, Clone)]
pub(crate) struct ToolCallObservation {
    pub(crate) principal_kind: String,
    pub(crate) principal_id: String,
    pub(crate) project: Option<String>,
    /// Calling surface: `api`, `mcp`, or `connector`.
    pub(crate) surface: String,
    pub(crate) session_id: Option<String>,
    pub(crate) tool: String,
    pub(crate) observed_at: i64,
}

/// Latest connector endpoint observation.
#[derive(Debug, Clone)]
pub(crate) struct ConnectorObservation {
    /// `ready`, `not_ready`, or `request_succeeded`.
    pub(crate) status: String,
    /// `readiness_probe` or `connector_request`.
    pub(crate) source: String,
    pub(crate) observed_at: i64,
}

#[derive(Debug, Default)]
pub(crate) struct RuntimeObservations {
    connector_configured: AtomicBool,
    connector: Mutex<Option<ConnectorObservation>>,
    tool_calls: Mutex<VecDeque<ToolCallObservation>>,
}

impl RuntimeObservations {
    pub(crate) fn set_connector_configured(&self) {
        self.connector_configured.store(true, Ordering::SeqCst);
    }

    pub(crate) fn connector_configured(&self) -> bool {
        self.connector_configured.load(Ordering::SeqCst)
    }

    pub(crate) fn record_connector_observation(&self, status: &str, source: &str, now: i64) {
        let mut slot = self.connector.lock().expect("connector observation lock");
        *slot = Some(ConnectorObservation {
            status: status.to_string(),
            source: source.to_string(),
            observed_at: now,
        });
    }

    pub(crate) fn latest_connector_observation(&self) -> Option<ConnectorObservation> {
        self.connector
            .lock()
            .expect("connector observation lock")
            .clone()
    }

    /// Record a successful tool call. Non-meaningful observability tools are
    /// rejected here so the rule is enforced at the single recording funnel.
    pub(crate) fn record_successful_tool_call(&self, observation: ToolCallObservation) {
        if !is_meaningful_activity_tool(&observation.tool) {
            return;
        }
        let mut calls = self.tool_calls.lock().expect("tool call observation lock");
        if calls.len() >= MAX_TOOL_CALL_OBSERVATIONS {
            calls.pop_front();
        }
        calls.push_back(observation);
    }

    /// Latest meaningful call for a specific principal (any project/surface).
    pub(crate) fn latest_tool_call_for_principal(
        &self,
        principal_kind: &str,
        principal_id: &str,
    ) -> Option<ToolCallObservation> {
        let calls = self.tool_calls.lock().expect("tool call observation lock");
        calls
            .iter()
            .rev()
            .find(|obs| obs.principal_kind == principal_kind && obs.principal_id == principal_id)
            .cloned()
    }

    /// Latest meaningful call across all principals.
    pub(crate) fn latest_tool_call(&self) -> Option<ToolCallObservation> {
        let calls = self.tool_calls.lock().expect("tool call observation lock");
        calls.back().cloned()
    }
}
