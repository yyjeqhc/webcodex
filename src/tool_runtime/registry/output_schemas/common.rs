use serde_json::{json, Value};

pub(crate) fn schema_type(kind: &str, description: &str) -> Value {
    json!({
        "type": kind,
        "description": description,
    })
}

pub(crate) fn nullable_schema(kind: &str, description: &str) -> Value {
    json!({
        "anyOf": [
            { "type": kind },
            { "type": "null" }
        ],
        "description": description,
    })
}

pub(crate) fn array_schema(items: Value, description: &str) -> Value {
    json!({
        "type": "array",
        "items": items,
        "description": description,
    })
}

pub(crate) fn open_object_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": true,
    })
}

pub(crate) fn task_outcome_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": false,
        "properties": {
            "status": {
                "type": "string",
                "enum": ["pass", "warn", "fail"]
            },
            "blocking": schema_type("boolean", "True only when the final task outcome is fail."),
            "blocking_reasons": array_schema(schema_type("string", "Task blocker reason identifier."), "Bounded task blocker reasons."),
            "warning_reasons": array_schema(schema_type("string", "Task warning reason identifier."), "Bounded task-only warning reasons.")
        },
        "required": ["status", "blocking", "blocking_reasons", "warning_reasons"]
    })
}

pub(crate) fn evidence_history_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": false,
        "properties": {
            "status": {
                "type": "string",
                "enum": ["clean", "mixed_resolved", "mixed_unresolved", "failed"]
            }
        },
        "required": ["status"]
    })
}

pub(crate) fn evidence_integrity_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": false,
        "properties": {
            "status": {
                "type": "string",
                "enum": ["clean", "warning", "error"]
            },
            "error_reasons": array_schema(schema_type("string", "Evidence integrity error reason identifier."), "Bounded integrity error reasons."),
            "warning_reasons": array_schema(schema_type("string", "Evidence integrity warning reason identifier."), "Bounded integrity warning reasons.")
        },
        "required": ["status", "error_reasons", "warning_reasons"]
    })
}

pub(crate) fn permission_profile_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": false,
        "properties": {
            "policy": {
                "type": "string",
                "enum": ["dev_auto_approve", "require_approval", "disabled", "off"],
                "description": "Current permission policy/profile."
            },
            "human_approval_required": {
                "type": "boolean",
                "description": "False for the self-hosted development dev_auto_approve profile."
            },
            "auto_approve": {
                "type": "boolean",
                "description": "True when high-risk tools are automatically approved after hard safety checks pass."
            },
            "release_recommended_policy": {
                "type": "string",
                "enum": ["require_approval"],
                "description": "Recommended future release policy."
            }
        },
        "required": [
            "policy",
            "human_approval_required",
            "auto_approve",
            "release_recommended_policy"
        ]
    })
}

pub(crate) fn permission_summary_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": true,
        "properties": {
            "policy": schema_type("string", "Effective permission policy."),
            "events_total": schema_type("integer", "Permission-bearing ledger events counted."),
            "required_count": schema_type("integer", "Permission decisions that required approval handling."),
            "approved_count": schema_type("integer", "Compatibility alias for manual_approved_count."),
            "manual_approved_count": schema_type("integer", "Manually approved decisions."),
            "auto_approved_count": schema_type("integer", "Automatically approved decisions."),
            "total_approved_count": schema_type("integer", "manual_approved_count plus auto_approved_count."),
            "denied_count": schema_type("integer", "Denied or expired decisions."),
            "pending_count": schema_type("integer", "Pending approval decisions."),
            "hard_denied_count": schema_type("integer", "Hard-denied decisions after safety guards."),
            "human_approval_required": schema_type("boolean", "Whether the active profile requires human approval."),
            "recent": array_schema(open_object_schema("Bounded recent permission decision."), "Newest-first bounded permission decisions.")
        }
    })
}

pub(crate) fn job_lifecycle_summary_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": true,
        "properties": {
            "active_count": schema_type("integer", "Compatibility broad active count: blocking active plus nonblocking terminal-pending jobs."),
            "running_count": schema_type("integer", "Blocking running-like jobs: queued, running, started, or agent_queued."),
            "stop_requested_count": schema_type("integer", "Jobs with status stop_requested."),
            "terminal_pending_count": schema_type("integer", "Nonblocking active jobs waiting for terminal status."),
            "blocking_active_count": schema_type("integer", "Jobs that should block finish/handoff closeout."),
            "nonblocking_active_count": schema_type("integer", "Active but nonblocking jobs, currently stop_requested."),
            "recent": array_schema(open_object_schema("Bounded recent job metadata; never stdout/stderr or command text."), "Bounded recent active job metadata."),
            "recent_limit": schema_type("integer", "Maximum recent jobs returned."),
            "truncated": schema_type("boolean", "True when more active jobs existed than recent_limit."),
            "warnings": array_schema(open_object_schema("Job lifecycle warning; active_jobs_present has blocking=true and jobs_terminal_pending has blocking=false."), "Bounded lifecycle warnings.")
        }
    })
}

fn permission_decision_schema() -> Value {
    open_object_schema("Permission decision metadata for high-risk tools after hard safety checks pass. Never includes stdout, stderr, env, tokens, secrets, or raw input content.")
}

pub(crate) fn search_context_line_schema() -> Value {
    json!({
        "type": "object",
        "description": "A context line adjacent to a search match.",
        "properties": {
            "line": {
                "type": "integer",
                "description": "1-based line number."
            },
            "text": {
                "type": "string",
                "description": "Line text."
            }
        },
        "required": ["line", "text"],
        "additionalProperties": true
    })
}

pub(crate) fn search_match_schema() -> Value {
    let context_lines = array_schema(search_context_line_schema(), "Context lines.");
    json!({
        "type": "object",
        "description": "Search match with path, 1-based line, preview, and bounded context lines.",
        "properties": {
            "path": {
                "type": "string",
                "description": "Project-relative file path."
            },
            "line": {
                "type": "integer",
                "description": "1-based match line number."
            },
            "preview": {
                "type": "string",
                "description": "Matched line preview."
            },
            "context_before": context_lines.clone(),
            "context_after": context_lines,
        },
        "required": ["path", "line", "preview", "context_before", "context_after"],
        "additionalProperties": true
    })
}

fn session_hint_schema() -> Value {
    json!({
        "type": "object",
        "description": "Optional lightweight hint that the recorder session has open guidance, question, todo, or risk messages. Counts only; never includes message text.",
        "properties": {
            "has_open_messages": {
                "type": "boolean",
                "description": "True when any counted open session-local message exists."
            },
            "open_counts": {
                "type": "object",
                "description": "Open message counts by counted kind.",
                "properties": {
                    "guidance": { "type": "integer", "minimum": 0 },
                    "question": { "type": "integer", "minimum": 0 },
                    "todo": { "type": "integer", "minimum": 0 },
                    "risk": { "type": "integer", "minimum": 0 }
                },
                "required": ["guidance", "question", "todo", "risk"],
                "additionalProperties": false
            },
            "highest_priority": {
                "type": "string",
                "enum": ["low", "normal", "high"],
                "description": "Highest priority among counted open messages."
            },
            "suggested_next_tool": {
                "type": "string",
                "enum": ["session_discussion_summary"],
                "description": "Tool to call when the model needs the bounded message details."
            }
        },
        "required": [
            "has_open_messages",
            "open_counts",
            "highest_priority",
            "suggested_next_tool"
        ],
        "additionalProperties": false
    })
}

pub(crate) fn wrapped_output_schema(output_properties: Vec<(&str, Value)>) -> Value {
    let mut output_properties = output_properties;
    output_properties.extend([
        (
            "session_recorded",
            schema_type(
                "boolean",
                "True when this tool call was recorded in a provided session_id.",
            ),
        ),
        (
            "session_id",
            schema_type(
                "string",
                "Session id used for telemetry recording, when provided.",
            ),
        ),
        (
            "session_event_id",
            schema_type(
                "string",
                "Session event id for the recorded finished tool call.",
            ),
        ),
        ("session_hint", session_hint_schema()),
        ("permission", permission_decision_schema()),
    ]);
    let properties = output_properties
        .into_iter()
        .map(|(name, schema)| (name.to_string(), schema))
        .collect::<serde_json::Map<_, _>>();
    json!({
        "type": "object",
        "properties": {
            "success": { "type": "boolean" },
            "output": {
                "type": "object",
                "properties": properties,
                "additionalProperties": true
            },
            "error": {
                "anyOf": [
                    { "type": "string" },
                    { "type": "null" }
                ]
            }
        },
        "required": ["success"],
        "additionalProperties": true,
    })
}

pub(crate) fn default_output_schema() -> Value {
    wrapped_output_schema(vec![])
}
