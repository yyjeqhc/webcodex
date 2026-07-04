use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::metadata::{self, ToolPathHint, ToolRisk};
use super::sessions::SessionEvent;
use super::types::ToolResult;

pub(crate) const DEFAULT_PERMISSION_POLICY: &str = "dev_auto_approve";
pub(crate) const RELEASE_RECOMMENDED_PERMISSION_POLICY: &str = "require_approval";
pub(crate) const DEFAULT_PERMISSION_RECENT_LIMIT: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct PermissionDecision {
    pub(crate) required: bool,
    pub(crate) policy: String,
    pub(crate) request_id: String,
    pub(crate) status: String,
    pub(crate) reason: String,
    pub(crate) risk: String,
    pub(crate) tool_name: String,
    pub(crate) project: Option<String>,
}

pub(crate) fn permission_profile_payload() -> Value {
    json!({
        "policy": DEFAULT_PERMISSION_POLICY,
        "human_approval_required": false,
        "auto_approve": true,
        "release_recommended_policy": RELEASE_RECOMMENDED_PERMISSION_POLICY,
    })
}

pub(crate) fn permission_decision_for_tool(
    tool_name: &str,
    project: Option<&str>,
) -> Option<PermissionDecision> {
    let metadata = metadata::tool_metadata(tool_name);
    let required = !metadata.read_only || metadata.destructive || metadata.shell_like;
    if !required {
        return None;
    }
    Some(PermissionDecision {
        required: true,
        policy: DEFAULT_PERMISSION_POLICY.to_string(),
        request_id: format!("wc_perm_{}", uuid::Uuid::new_v4().simple()),
        status: "auto_approved".to_string(),
        reason: DEFAULT_PERMISSION_POLICY.to_string(),
        risk: permission_risk(tool_name),
        tool_name: tool_name.to_string(),
        project: project.map(str::to_string),
    })
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

    json!({
        "policy": DEFAULT_PERMISSION_POLICY,
        "events_total": events_total,
        "required_count": required_count,
        "auto_approved_count": auto_approved_count,
        "approved_count": approved_count,
        "denied_count": denied_count,
        "pending_count": pending_count,
        "hard_denied_count": hard_denied_count,
        "human_approval_required": false,
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

fn permission_risk(tool_name: &str) -> String {
    let metadata = metadata::tool_metadata(tool_name);
    if matches!(tool_name, "cargo_fmt" | "cargo_check" | "cargo_test") {
        return "validation".to_string();
    }
    if matches!(tool_name, "run_job" | "stop_job" | "run_codex") {
        return "job".to_string();
    }
    if metadata.shell_like {
        return "shell".to_string();
    }
    if metadata.destructive {
        return "destructive".to_string();
    }
    if metadata.path_hint == ToolPathHint::Artifact {
        return "artifact_write".to_string();
    }
    if metadata.path_hint == ToolPathHint::Patch || tool_name.contains("patch") {
        return "patch".to_string();
    }
    if matches!(
        metadata.risk,
        ToolRisk::ProjectWrite | ToolRisk::AccountManage
    ) {
        return "write".to_string();
    }
    "write".to_string()
}
