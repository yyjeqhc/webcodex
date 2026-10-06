//! Typed transactional edit-effect facts shared by receipt validation and projection.
//!
//! Missing/malformed values remain unknown. In particular a missing planned_count
//! is the older accepted receipt shape; a present null or malformed count is not.
//! This interprets receipts only: it neither authorizes edits nor grants retry safety.
use super::execution_outcome::{ExecutionOutcomeFacts, ExecutionState};
use super::ToolResult;
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub(super) struct EditOutcomeFacts {
    dry_run: Option<bool>,
    applied_count: Option<u64>,
    planned_count: Option<u64>,
    planned_count_present: bool,
    changed: Option<bool>,
    would_change: Option<bool>,
    state_changed: Option<bool>,
    rollback_complete: Option<bool>,
}

impl EditOutcomeFacts {
    pub(super) fn from_output(output: &Value) -> Self {
        Self {
            dry_run: output.get("dry_run").and_then(Value::as_bool),
            applied_count: output.get("applied_count").and_then(Value::as_u64),
            planned_count: output.get("planned_count").and_then(Value::as_u64),
            planned_count_present: output.get("planned_count").is_some(),
            changed: output.get("changed").and_then(Value::as_bool),
            would_change: output.get("would_change").and_then(Value::as_bool),
            state_changed: output.get("state_changed").and_then(Value::as_bool),
            rollback_complete: output.get("rollback_complete").and_then(Value::as_bool),
        }
    }

    pub(super) fn rollback_complete(self) -> Option<bool> {
        self.rollback_complete
    }

    pub(super) fn proves_no_effect_failure(self) -> bool {
        let uncertain = self.rollback_complete == Some(false)
            || self.changed == Some(true)
            || self.state_changed == Some(true);
        !uncertain
            && (self.rollback_complete == Some(true)
                || self.changed == Some(false)
                || self.state_changed == Some(false))
    }

    /// Return the proven changed flag, including a successful no-op. A boolean
    /// result alone would conflate a confirmed no-op with an invalid receipt.
    pub(super) fn confirmed_runner_change(
        self,
        tool_name: &str,
        expected_change_count: usize,
        expected_dry_run: bool,
    ) -> Option<bool> {
        let text_edit = tool_name == "edit_project_files";
        let count = expected_change_count as u64;
        let expected_applied = if text_edit && expected_dry_run && self.planned_count_present {
            0
        } else {
            count
        };
        let valid = self.dry_run == Some(expected_dry_run)
            && self.applied_count == Some(expected_applied)
            && (!text_edit || !self.planned_count_present || self.planned_count == Some(count))
            && self.changed.is_some()
            && self.would_change.is_some()
            && !(expected_dry_run && self.changed == Some(true))
            && (expected_dry_run || self.changed == self.would_change);
        valid.then_some(self.changed).flatten()
    }
}

/// Only post-validation, actual completed edits may omit redundant effect echoes.
/// Dry runs, partial/malformed receipts and uncertain outcomes retain their facts.
pub(super) fn can_compact_edit_success(
    result: &ToolResult,
    expected_change_count: usize,
    requested_dry_run: bool,
) -> bool {
    let facts = EditOutcomeFacts::from_output(&result.output);
    result.success
        && !requested_dry_run
        && ExecutionOutcomeFacts::from_result(result).execution_state()
            == Some(ExecutionState::Completed)
        && facts
            .confirmed_runner_change("edit_project_files", expected_change_count, false)
            .is_some()
        && facts.state_changed == facts.changed
}

#[cfg(test)]
#[path = "tests/edit_outcome.rs"]
mod tests;
