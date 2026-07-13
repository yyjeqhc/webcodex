//! GPT Action response compact experiment.
//!
//! Applied only on the HTTP `POST /api/tools/call` response path after tool
//! execution completes. Does not change MCP, tools/list, OpenAPI schemas,
//! tool execution, session ledger events, or permission decisions.

use crate::tool_runtime::ToolResult;
use serde_json::{json, Value};

/// Optionally shrink a successful GPT Action tool result for the HTTP client.
///
/// Errors are returned unchanged so failure details stay identical.
pub(crate) fn compact_action_tool_result(tool: &str, result: ToolResult) -> ToolResult {
    if !result.success {
        return result;
    }
    match tool {
        "start_coding_task" => ToolResult {
            success: true,
            output: compact_start_coding_task_output(&result.output),
            error: None,
        },
        _ => result,
    }
}

/// Compact `start_coding_task` success output for GPT Actions.
///
/// Keeps identifiers and operator guidance needed to continue the coding loop.
/// Drops large startup aggregates (tool_manifest, full runtime_status, rules
/// content, git details, permissions profile, recommended_flow, etc.). Callers
/// can re-query those via focused tools.
pub(crate) fn compact_start_coding_task_output(output: &Value) -> Value {
    let session_id = output
        .pointer("/session/session_id")
        .cloned()
        .unwrap_or(Value::Null);
    // Coding-task identifier is the workflow session id; surface both names
    // so Action clients can treat either field as the task handle.
    let task_id = session_id.clone();
    let project = output.get("project").cloned().unwrap_or(Value::Null);
    let mode = output
        .pointer("/session/mode")
        .cloned()
        .unwrap_or(Value::Null);
    let resolved_project = compact_resolved_project(output.get("resolved_project"));
    let verdict = output.get("startup_verdict");
    let status = verdict
        .and_then(|v| v.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let blocking = verdict
        .and_then(|v| v.get("blocking"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let next_steps = verdict
        .and_then(|v| v.get("suggested_next_actions"))
        .cloned()
        .unwrap_or_else(|| json!([]));
    let summary = build_startup_summary(status, blocking, &project, &session_id);
    let semantic_navigation = compact_semantic_navigation(output.get("semantic_navigation"));
    let warnings = compact_warnings(output.get("warnings"));

    json!({
        "compact": true,
        "session_id": session_id,
        "task_id": task_id,
        "project": project,
        "resolved_project": resolved_project,
        "mode": mode,
        "summary": summary,
        "next_steps": next_steps.clone(),
        "startup_verdict": {
            "status": status,
            "blocking": blocking,
            "suggested_next_actions": next_steps,
        },
        "semantic_navigation": semantic_navigation,
        "warnings": warnings,
        "deterministic": true,
        "llm_summary": false,
    })
}

fn compact_resolved_project(resolved: Option<&Value>) -> Value {
    let Some(resolved) = resolved.filter(|v| v.is_object()) else {
        return Value::Null;
    };
    json!({
        "id": resolved.get("id").cloned().unwrap_or(Value::Null),
        "input": resolved.get("input").cloned().unwrap_or(Value::Null),
        "executor": resolved.get("executor").cloned().unwrap_or(Value::Null),
    })
}

fn compact_semantic_navigation(nav: Option<&Value>) -> Value {
    let Some(nav) = nav.filter(|v| v.is_object()) else {
        return Value::Null;
    };
    json!({
        "supported": nav.get("supported").cloned().unwrap_or(Value::Null),
        "available": nav.get("available").cloned().unwrap_or(Value::Null),
        "recommended": nav.get("recommended").cloned().unwrap_or(Value::Null),
        "status": nav.get("status").cloned().unwrap_or(Value::Null),
        "language": nav.get("language").cloned().unwrap_or(Value::Null),
        "server": nav.get("server").cloned().unwrap_or(Value::Null),
    })
}

fn compact_warnings(warnings: Option<&Value>) -> Value {
    match warnings {
        Some(Value::Array(items)) => {
            // Keep a short bound so compact mode never reintroduces bulk.
            let trimmed: Vec<Value> = items.iter().take(8).cloned().collect();
            Value::Array(trimmed)
        }
        Some(other) => other.clone(),
        None => json!([]),
    }
}

fn build_startup_summary(
    status: &str,
    blocking: bool,
    project: &Value,
    session_id: &Value,
) -> String {
    let project = project.as_str().unwrap_or("unknown project");
    let session = session_id.as_str().unwrap_or("unknown session");
    if blocking {
        format!("Startup {status} (blocking) for {project}; session {session}. Follow next_steps before editing.")
    } else {
        format!(
            "Startup {status} for {project}; session {session} ready. Use session_id on later tools."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_start_coding_task_output() -> Value {
        json!({
            "project": "demo",
            "resolved_project": {
                "input": "demo",
                "id": "agent:importer:demo",
                "path": "/tmp/demo",
                "executor": "agent",
                "client_id": "importer",
                "allow_patch": true
            },
            "session": {
                "session_id": "wc_sess_test123",
                "mode": "normal",
                "guards": {"deny_write_tools": false, "deny_shell_tools": false},
                "lifecycle": {"state": "open"},
                "explicit_session_id_recommended": true,
                "current_binding": {"bound": false}
            },
            "runtime_status": {
                "service": "webcodex",
                "tools": {"count": 75, "names": ["a", "b", "c"]},
                "agents": {"clients": [{"id": "x"}, {"id": "y"}]}
            },
            "permissions": {
                "policy": "dev_auto_approve",
                "auto_approve": true,
                "details": "large profile blob"
            },
            "rules": {
                "present": true,
                "sources": [{"path": "AGENTS.md", "first_lines": ["# Rules"]}]
            },
            "git": {
                "clean": true,
                "recent_commits": [{"subject": "init"}, {"subject": "more"}]
            },
            "semantic_navigation": {
                "supported": true,
                "available": true,
                "recommended": true,
                "status": "available",
                "language": "rust",
                "server": "rust-analyzer",
                "tools": ["lsp_definition", "lsp_references"]
            },
            "tool_manifest": {
                "schema_version": 1,
                "count": 75,
                "tools": [
                    {"name": "start_coding_task", "accepted_flattened_args": ["project", "title"]},
                    {"name": "read_file", "accepted_flattened_args": ["project", "path"]}
                ]
            },
            "recommended_flow": {
                "inspect": ["read_file", "search_project_text"],
                "edit": ["replace_line_range"]
            },
            "startup_verdict": {
                "status": "pass",
                "blocking": false,
                "checks": [
                    {"name": "runtime_status", "status": "pass"},
                    {"name": "workspace", "status": "pass"}
                ],
                "suggested_next_actions": [
                    "proceed with the coding task using the explicit session_id"
                ]
            },
            "warnings": [],
            "deterministic": true,
            "llm_summary": false
        })
    }

    #[test]
    fn compact_start_coding_task_keeps_ids_summary_and_next_steps() {
        let full = sample_start_coding_task_output();
        let compact = compact_start_coding_task_output(&full);

        assert_eq!(compact["compact"], true);
        assert_eq!(compact["session_id"], "wc_sess_test123");
        assert_eq!(compact["task_id"], "wc_sess_test123");
        assert_eq!(compact["project"], "demo");
        assert_eq!(compact["resolved_project"]["id"], "agent:importer:demo");
        assert_eq!(compact["mode"], "normal");
        assert!(compact["summary"]
            .as_str()
            .unwrap()
            .contains("wc_sess_test123"));
        assert_eq!(
            compact["next_steps"][0],
            "proceed with the coding task using the explicit session_id"
        );
        assert_eq!(compact["startup_verdict"]["status"], "pass");
        assert_eq!(compact["startup_verdict"]["blocking"], false);
        assert_eq!(compact["semantic_navigation"]["recommended"], true);
    }

    #[test]
    fn compact_start_coding_task_drops_large_optional_blocks() {
        let full = sample_start_coding_task_output();
        let compact = compact_start_coding_task_output(&full);

        for dropped in [
            "tool_manifest",
            "runtime_status",
            "permissions",
            "rules",
            "git",
            "recommended_flow",
        ] {
            assert!(
                compact.get(dropped).is_none(),
                "compact output must drop {dropped}"
            );
        }
        assert!(
            compact.pointer("/startup_verdict/checks").is_none(),
            "compact startup_verdict must omit verbose checks"
        );
        assert!(
            compact.pointer("/semantic_navigation/tools").is_none(),
            "compact semantic_navigation must omit tools list"
        );
        assert!(
            compact.pointer("/resolved_project/path").is_none(),
            "compact resolved_project must omit path"
        );
    }

    #[test]
    fn compact_start_coding_task_is_much_smaller() {
        let full = sample_start_coding_task_output();
        let compact = compact_start_coding_task_output(&full);
        let full_bytes = serde_json::to_vec(&full).unwrap().len();
        let compact_bytes = serde_json::to_vec(&compact).unwrap().len();
        assert!(
            compact_bytes < full_bytes / 2,
            "compact ({compact_bytes}) should be under half of full ({full_bytes})"
        );
    }

    #[test]
    fn compact_action_tool_result_preserves_errors() {
        let err = ToolResult::err_with_output(
            "project not found",
            json!({"code": "unknown_project", "project": "missing"}),
        );
        let out = compact_action_tool_result("start_coding_task", err);
        assert!(!out.success);
        assert_eq!(out.error.as_deref(), Some("project not found"));
        assert_eq!(out.output["code"], "unknown_project");
        assert!(out.output.get("compact").is_none());
    }

    #[test]
    fn compact_action_tool_result_leaves_other_tools_unchanged() {
        let result = ToolResult::ok(json!({
            "tools": [{"name": "read_file"}, {"name": "write_file"}],
            "count": 2
        }));
        let out = compact_action_tool_result("list_tools", result);
        assert_eq!(out.output["count"], 2);
        assert_eq!(out.output["tools"].as_array().unwrap().len(), 2);
        assert!(out.output.get("compact").is_none());
    }
}
