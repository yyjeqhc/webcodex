//! Permission decision layer for high-risk tool execution.
//!
//! Module layout:
//! - [`model`] — modes, outcomes, [`PermissionDecision`], constants
//! - [`evaluator`] — single evaluation entry ([`PermissionEvaluator`])
//! - [`policy`] — mode behavior (`dev_auto_approve`, `audit_only`, …)
//! - [`risk`] — coarse risk classification facade
//!
//! Hard safety (session guard, path policy, scopes) lives outside this module
//! and is never bypassed by permission mode. See
//! `docs/agent/permission-model.md`.

mod evaluator;
mod model;
mod policy;
mod risk;

#[cfg(test)]
mod tests;

pub(crate) use evaluator::PermissionEvaluator;
pub(crate) use model::{PermissionDecision, DEFAULT_PERMISSION_RECENT_LIMIT};
pub(crate) use policy::EffectivePermissionConfig;

// Test-facing surface: mode parsing, outcomes, constants, compatibility wrapper.
#[cfg(test)]
pub(crate) use evaluator::permission_decision_for_tool;
#[cfg(test)]
pub(crate) use model::PermissionMode;
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use model::{
    PermissionModeParseError, PermissionOutcome, DEFAULT_PERMISSION_POLICY, PERMISSION_MODE_ENV,
    RELEASE_RECOMMENDED_PERMISSION_POLICY,
};
#[cfg(test)]
pub(crate) use policy::resolve_permission_mode;

use serde_json::{json, Value};

use super::sessions::SessionEvent;
use super::tool_result::ToolResult;

/// Operator-facing permission profile (runtime_status / coding-task startup).
pub(crate) fn permission_profile_payload() -> Value {
    policy::permission_profile_payload_for(&EffectivePermissionConfig::from_env())
}

pub(crate) fn add_permission_to_result(result: &mut ToolResult, permission: &PermissionDecision) {
    let mut output = match std::mem::take(&mut result.output) {
        Value::Object(map) => map,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("value".to_string(), other);
            map
        }
    };
    output.insert(
        "permission".to_string(),
        serde_json::to_value(permission).unwrap_or(Value::Null),
    );
    result.output = Value::Object(output);
}

/// Structured denial when the permission layer blocks execution before mutation.
///
/// Stable, diagnostic messages without tool parameters or sensitive content.
/// Callers attach the same [`PermissionDecision`] via [`add_permission_to_result`].
pub(crate) fn permission_execution_denied_result(decision: &PermissionDecision) -> ToolResult {
    let message = match decision.reason.as_str() {
        "require_approval_not_implemented" => {
            "permission denied: require_approval is not implemented; tool execution blocked"
                .to_string()
        }
        reason if reason.starts_with("invalid_permission_mode:") => format!(
            "permission denied: invalid {env} configuration; tool execution blocked",
            env = model::PERMISSION_MODE_ENV
        ),
        other => format!("permission denied: {other}"),
    };
    ToolResult::err_with_output(
        message.clone(),
        json!({
            "error": message,
            "error_kind": "permission_denied",
            "failure_kind": "permission_denied",
            "permission_reason": decision.reason,
            "permission_policy": decision.policy,
            "permission_status": decision.status,
        }),
    )
}

/// Deserialize a permission decision previously attached to tool output.
///
/// Used to reuse the single authoritative decision (e.g. outer recording
/// session) without re-evaluating.
pub(crate) fn permission_decision_from_output(output: &Value) -> Option<PermissionDecision> {
    let value = output.get("permission")?;
    serde_json::from_value(value.clone()).ok()
}

/// Detect hard-safety denials on tool output. Independent of permission mode:
/// auto-approve must never suppress these outcomes.
pub(crate) fn is_hard_denied_output(output: &Value, error: Option<&str>) -> bool {
    let structured_hard_deny = [
        "policy_rejected",
        "session_guard_denied",
        "unknown_session_id",
        "session_project_mismatch",
        "confirmation_required",
        "job_not_found",
        "job_project_mismatch",
        "job_stop_forbidden",
    ];
    for key in ["error_kind", "failure_kind"] {
        if output
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|kind| structured_hard_deny.contains(&kind))
        {
            return true;
        }
    }
    let Some(error) = error else {
        return false;
    };
    let lower = error.to_lowercase();
    lower.contains("sensitive path")
        || lower.contains("sensitive artifact path")
        || lower.contains("path must be project-relative")
        || lower.contains("path cannot contain parent traversal")
        || lower.contains("absolute paths are not allowed")
        || lower.contains("path traversal")
}

pub(crate) fn permission_summary_from_events(events: &[SessionEvent], limit: usize) -> Value {
    let mut events_total = 0usize;
    let mut required_count = 0usize;
    let mut auto_approved_count = 0usize;
    let mut approved_count = 0usize;
    let mut denied_count = 0usize;
    let mut pending_count = 0usize;
    let mut hard_denied_count = 0usize;
    let mut recent = Vec::new();

    for event in events
        .iter()
        .rev()
        .filter(|event| event.kind == "tool_call_finished")
    {
        let Some(permission) = event.permission.as_ref() else {
            continue;
        };
        events_total += 1;
        if permission.required {
            required_count += 1;
        }
        match permission.status.as_str() {
            "auto_approved" => auto_approved_count += 1,
            "approved" => approved_count += 1,
            "denied" | "expired" => denied_count += 1,
            "requested" => pending_count += 1,
            "hard_denied" => hard_denied_count += 1,
            _ => {}
        }
        if recent.len() < limit {
            recent.push(json!({
                "tool_name": permission.tool_name.clone(),
                "status": permission.status.clone(),
                "risk": permission.risk.clone(),
                "project": permission.project.clone(),
            }));
        }
    }

    let manual_approved_count = approved_count;
    let total_approved_count = manual_approved_count + auto_approved_count;
    let config = EffectivePermissionConfig::from_env();

    json!({
        "policy": config.policy_name(),
        "events_total": events_total,
        "required_count": required_count,
        "auto_approved_count": auto_approved_count,
        "manual_approved_count": manual_approved_count,
        "total_approved_count": total_approved_count,
        "approved_count": approved_count,
        "denied_count": denied_count,
        "pending_count": pending_count,
        "hard_denied_count": hard_denied_count,
        "human_approval_required": config.human_approval_required(),
        "recent": recent,
    })
}

pub(crate) fn edit_path_policy_rejected_result(path: &str, message: String) -> ToolResult {
    ToolResult::err_with_output(
        message.clone(),
        json!({
            "path": path,
            "error": message,
            "failure_kind": "policy_rejected",
            "error_kind": "policy_rejected",
        }),
    )
}
