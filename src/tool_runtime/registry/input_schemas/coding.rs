use serde_json::{json, Value};

use super::sessions::session_mode_schema;

pub(crate) fn start_coding_task_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "description": "Required runtime project id. Use a full id from list_projects, such as agent:<client_id>:<project_id>."
            },
            "title": {
                "type": "string",
                "description": "Optional human-readable task title for the created session."
            },
            "mode": session_mode_schema("Optional session mode. Defaults to normal. read_only automatically blocks write-like and shell/job-like tools in the created session."),
            "deny_write_tools": {
                "type": "boolean",
                "description": "Optional task guard for the created session. Defaults to false unless mode=read_only."
            },
            "deny_shell_tools": {
                "type": "boolean",
                "description": "Optional task guard for the created session. Defaults to false unless mode=read_only."
            },
            "include_runtime_status": {
                "type": "boolean",
                "description": "Include runtime_status output. Defaults to true."
            },
            "compact_startup": {
                "type": "boolean",
                "description": "When include_runtime_status=true, return compact startup runtime observability instead of full runtime_status. Defaults to false."
            },
            "include_git": {
                "type": "boolean",
                "description": "Include structured git/worktree status derived from show_changes. Defaults to true."
            },
            "include_recent_commits": {
                "type": "boolean",
                "description": "Include recent commits from git_log. Defaults to true."
            },
            "include_rules": {
                "type": "boolean",
                "description": "Include a deterministic project instruction source summary. Defaults to true."
            },
            "include_tool_manifest": {
                "type": "boolean",
                "description": "Include compact tool_manifest output without full input/output schemas. Defaults to true."
            },
            "tool_manifest_categories": {
                "type": "array",
                "items": { "type": "string" },
                "description": "When include_tool_manifest=true, optionally return only compact manifest entries for these categories. For startup, prefer a bounded set such as workflow, session, git, edit, artifact, and cleanup instead of the full tool set."
            },
            "tool_manifest_limit": {
                "type": "integer",
                "description": "When include_tool_manifest=true, maximum compact manifest entries to return; clamped to 1..100."
            },
            "bind_current": {
                "type": "boolean",
                "description": "If true, bind the new session as the caller/transport/project current session. Defaults to false. Binding is process-local in-memory control metadata."
            }
        },
        "required": ["project"],
        "additionalProperties": false,
    })
}

pub(crate) fn finish_coding_task_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "description": "Required runtime project id. Use the same project used to start the task."
            },
            "session_id": {
                "type": "string",
                "description": "Required explicit wc_sess_* id returned by start_coding_task or start_session. This is business input, not current-session fallback."
            },
            "include_diff": {
                "type": "boolean",
                "description": "Include bounded diff hunks in show_changes. Defaults to true."
            },
            "include_workspace": {
                "type": "boolean",
                "description": "Compatibility flag matching session_handoff_summary.include_workspace. Defaults to true. When include_handoff=true, controls whether the nested handoff summary includes its workspace block; the top-level finish workspace/show_changes check remains unchanged."
            },
            "include_hygiene": {
                "type": "boolean",
                "description": "Include workspace_hygiene_check output. Defaults to true."
            },
            "include_handoff": {
                "type": "boolean",
                "description": "Include session_handoff_summary output. Defaults to true."
            },
            "include_validation_summary": {
                "type": "boolean",
                "description": "Include deterministic validation-like session ledger event summary when available. Defaults to true; minimal diagnostics require bounded tails or safe result metadata."
            },
            "summary_only": {
                "type": "boolean",
                "description": "When true, return compact verdict fields only: workspace_clean, hygiene_clean, jobs, permissions, tool_failures, validation, warnings, and suggested_next_actions. Omits show_changes payloads, handoff details, command text, stdout/stderr, tails, and excerpts."
            }
        },
        "required": ["project", "session_id"],
        "additionalProperties": false,
    })
}
