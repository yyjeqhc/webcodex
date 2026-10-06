//! Typed, read-only interpretation of canonical execution-result facts.
//!
//! Producers keep their established JSON wire shape, but internal consumers must
//! not each redefine lifecycle vocabulary or retry semantics from raw keys.

use super::ToolResult;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutionState {
    Completed,
    Queued,
    Running,
    Started,
    Pending,
    NotStarted,
    OutcomeUnknown,
    Other,
}

impl ExecutionState {
    fn parse(value: Option<&str>) -> Option<Self> {
        value.map(|value| match value {
            "completed" => Self::Completed,
            "queued" => Self::Queued,
            "running" => Self::Running,
            "started" => Self::Started,
            "pending" => Self::Pending,
            "not_started" => Self::NotStarted,
            "outcome_unknown" => Self::OutcomeUnknown,
            _ => Self::Other,
        })
    }

    fn is_pending(self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Running | Self::Started | Self::Pending
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExecutionOutcomeFacts<'a> {
    success: bool,
    output: Option<&'a Map<String, Value>>,
}

impl<'a> ExecutionOutcomeFacts<'a> {
    pub(crate) fn from_result(result: &'a ToolResult) -> Self {
        Self {
            success: result.success,
            output: result.output.as_object(),
        }
    }

    fn value(self, key: &str) -> Option<&'a Value> {
        self.output.and_then(|output| output.get(key))
    }

    /// Preserve producer vocabulary for diagnostic-only adapters, including
    /// states not in the closed execution classification below.
    pub(crate) fn execution_state_name(self) -> Option<&'a str> {
        self.value("execution_state").and_then(Value::as_str)
    }

    pub(crate) fn execution_state(self) -> Option<ExecutionState> {
        ExecutionState::parse(self.execution_state_name())
    }

    pub(crate) fn state_changed(self) -> Option<bool> {
        self.value("state_changed").and_then(Value::as_bool)
    }

    /// Any non-null Job marker prevents proof of synchronous completion, even
    /// when malformed. This is deliberately not a validated Job identity.
    pub(crate) fn has_job_marker(self) -> bool {
        !self.is_absent_or_null("job_id")
    }

    pub(crate) fn command_started(self) -> Option<bool> {
        self.value("command_started").and_then(Value::as_bool)
    }

    pub(crate) fn command_completed(self) -> Option<bool> {
        self.value("command_completed").and_then(Value::as_bool)
    }

    pub(crate) fn command_ok(self) -> Option<bool> {
        self.value("command_ok").and_then(Value::as_bool)
    }

    pub(crate) fn terminal(self) -> Option<bool> {
        self.value("terminal").and_then(Value::as_bool)
    }

    pub(crate) fn promoted_to_job(self) -> Option<bool> {
        self.value("promoted_to_job").and_then(Value::as_bool)
    }

    /// Raw string receipt, including empty text. Consumers with established
    /// fallback precedence must not silently substitute another Job identity.
    pub(crate) fn job_id_text(self) -> Option<&'a str> {
        self.value("job_id").and_then(Value::as_str)
    }

    pub(crate) fn job_id(self) -> Option<&'a str> {
        self.job_id_text().filter(|value| !value.is_empty())
    }

    pub(crate) fn observation_token(self) -> Option<&'a str> {
        self.value("observation_token").and_then(Value::as_str)
    }

    pub(crate) fn failure_kind(self) -> Option<&'a str> {
        self.value("failure_kind").and_then(Value::as_str)
    }

    /// Specialized diagnostics prefer a present failure_kind field over
    /// error_kind before decoding. A present null/malformed value does not fall
    /// back; changing that precedence would change existing audit semantics.
    pub(crate) fn preferred_failure_kind(self) -> Option<&'a str> {
        self.value("failure_kind")
            .or_else(|| self.value("error_kind"))
            .and_then(Value::as_str)
    }

    pub(crate) fn error_kind(self) -> Option<&'a str> {
        self.value("error_kind").and_then(Value::as_str)
    }

    pub(crate) fn has_pending_execution_state(self) -> bool {
        self.execution_state()
            .is_some_and(ExecutionState::is_pending)
    }

    /// True when the canonical result says execution is still active or was
    /// handed off. This intentionally preserves the established conservative
    /// interpretation of `terminal=false` even when a producer omitted a state.
    pub(crate) fn is_pending(self) -> bool {
        self.has_pending_execution_state()
            || self.terminal() == Some(false)
            || self.promoted_to_job() == Some(true)
    }

    pub(crate) fn is_definitely_not_started(self) -> bool {
        self.execution_state() == Some(ExecutionState::NotStarted)
            || self.value("dispatch_certainty").and_then(Value::as_str) == Some("not_started")
            || self.error_kind() == Some("permission_denied")
    }

    pub(crate) fn is_outcome_unknown(self) -> bool {
        self.execution_state() == Some(ExecutionState::OutcomeUnknown)
            || self.failure_kind() == Some("outcome_unknown")
    }

    fn is_absent_or_null(self, key: &str) -> bool {
        self.value(key).is_none_or(Value::is_null)
    }

    /// Proven synchronous terminal lifecycle success. Domain-specific evidence
    /// such as validator `passed` or process `command_ok` remains an explicit
    /// additional requirement at the caller.
    pub(crate) fn is_terminal_success(self) -> bool {
        self.success
            && self.execution_state() == Some(ExecutionState::Completed)
            && self.command_started() == Some(true)
            && self.command_completed() == Some(true)
            && self.promoted_to_job() == Some(false)
            && self.terminal() == Some(true)
            && self.is_absent_or_null("job_id")
            && self.is_absent_or_null("job_status")
            && self.is_absent_or_null("observation_token")
    }

    pub(crate) fn is_terminal_command_success(self) -> bool {
        self.is_terminal_success() && self.command_ok() == Some(true)
    }

    /// A model-facing Job handoff additionally requires durable Job identity.
    /// Continuation shape is validated by the Job projection that consumes this.
    pub(crate) fn has_job_handoff(self) -> bool {
        self.success && self.has_pending_execution_state() && self.job_id().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn result(output: Value) -> ToolResult {
        ToolResult::ok(output)
    }

    #[test]
    fn pending_vocabulary_and_conservative_markers_share_one_interpretation() {
        for state in ["queued", "running", "started", "pending"] {
            let result = result(json!({"execution_state": state}));
            let facts = ExecutionOutcomeFacts::from_result(&result);
            assert!(facts.has_pending_execution_state(), "{state}");
            assert!(facts.is_pending(), "{state}");
        }
        assert!(
            ExecutionOutcomeFacts::from_result(&result(json!({"terminal": false}))).is_pending()
        );
        assert!(
            ExecutionOutcomeFacts::from_result(&result(json!({"promoted_to_job": true})))
                .is_pending()
        );
    }

    #[test]
    fn terminal_success_requires_complete_synchronous_lifecycle_truth() {
        let canonical = result(json!({
            "execution_state": "completed",
            "command_started": true,
            "command_completed": true,
            "command_ok": true,
            "promoted_to_job": false,
            "terminal": true,
            "job_id": null,
            "job_status": null,
            "observation_token": null
        }));
        let facts = ExecutionOutcomeFacts::from_result(&canonical);
        assert!(facts.is_terminal_success());
        assert!(facts.is_terminal_command_success());

        let with_job = result(json!({
            "execution_state": "completed",
            "command_started": true,
            "command_completed": true,
            "command_ok": true,
            "promoted_to_job": false,
            "terminal": true,
            "job_id": "job-1"
        }));
        assert!(!ExecutionOutcomeFacts::from_result(&with_job).is_terminal_success());
    }

    #[test]
    fn retry_safety_states_are_centralized() {
        for output in [
            json!({"execution_state": "not_started"}),
            json!({"dispatch_certainty": "not_started"}),
            json!({"error_kind": "permission_denied"}),
        ] {
            assert!(ExecutionOutcomeFacts::from_result(&result(output)).is_definitely_not_started());
        }
        for output in [
            json!({"execution_state": "outcome_unknown"}),
            json!({"failure_kind": "outcome_unknown"}),
        ] {
            assert!(ExecutionOutcomeFacts::from_result(&result(output)).is_outcome_unknown());
        }
    }

    #[test]
    fn job_handoff_requires_success_pending_state_and_nonempty_identity() {
        let handoff = result(json!({"execution_state": "running", "job_id": "job-1"}));
        assert!(ExecutionOutcomeFacts::from_result(&handoff).has_job_handoff());

        let missing_id = result(json!({"execution_state": "running", "job_id": ""}));
        assert!(!ExecutionOutcomeFacts::from_result(&missing_id).has_job_handoff());

        let failed = ToolResult::err_with_output(
            "failed",
            json!({"execution_state": "running", "job_id": "job-1"}),
        );
        assert!(!ExecutionOutcomeFacts::from_result(&failed).has_job_handoff());
    }
}
