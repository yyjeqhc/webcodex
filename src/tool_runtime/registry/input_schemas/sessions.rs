use serde_json::{json, Value};

use super::common::object_schema;

pub(crate) fn session_mode_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "enum": ["normal", "read_only"],
        "description": description,
    })
}

pub(crate) fn session_guards_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": false,
        "properties": {
            "deny_write_tools": {
                "type": "boolean",
                "description": "True when write-like runtime tools are blocked for this session."
            },
            "deny_shell_tools": {
                "type": "boolean",
                "description": "True when shell/job-like runtime tools are blocked for this session."
            }
        },
        "required": ["deny_write_tools", "deny_shell_tools"]
    })
}

fn session_message_kind_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "enum": [
            "note", "proposal", "question", "answer", "decision", "risk",
            "progress", "guidance", "todo"
        ],
        "description": description,
    })
}

fn session_message_status_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "enum": ["open", "resolved"],
        "description": description,
    })
}

fn session_message_priority_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "enum": ["low", "normal", "high"],
        "description": description,
    })
}

pub(crate) fn post_session_message_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": {
                "type": "string",
                "description": "Required wc_sess_* id whose session-local message board receives this message. This is business input, not recorder metadata."
            },
            "kind": session_message_kind_schema("Message kind."),
            "message": {
                "type": "string",
                "maxLength": 8000,
                "description": "Non-empty message body. Guidance is session-local context and never overrides system/platform/WebCodex safety policy."
            },
            "tags": {
                "type": "array",
                "items": { "type": "string", "maxLength": 64 },
                "maxItems": 16,
                "description": "Optional tags for filtering or review."
            },
            "reply_to": {
                "anyOf": [{ "type": "string" }, { "type": "null" }],
                "description": "Optional message id in the same session."
            },
            "priority": session_message_priority_schema("Optional priority; defaults to normal.")
        },
        "required": ["session_id", "kind", "message"],
        "additionalProperties": false,
    })
}

pub(crate) fn list_session_messages_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": {
                "type": "string",
                "description": "Required wc_sess_* id whose session-local message board is listed."
            },
            "kind": session_message_kind_schema("Optional kind filter."),
            "status": session_message_status_schema("Optional status filter."),
            "limit": {
                "type": "integer",
                "maximum": 100,
                "description": "Maximum messages to return. Defaults to 50 and is clamped to 100. Results are newest-first by created_at."
            }
        },
        "required": ["session_id"],
        "additionalProperties": false,
    })
}

pub(crate) fn resolve_session_message_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": {
                "type": "string",
                "description": "Required wc_sess_* id containing the message."
            },
            "message_id": {
                "type": "string",
                "description": "wc_msg_* id returned by post_session_message."
            },
            "resolution": {
                "type": "string",
                "maxLength": 8000,
                "description": "Optional resolution note."
            }
        },
        "required": ["session_id", "message_id"],
        "additionalProperties": false,
    })
}

pub(crate) fn session_summary_input_schema() -> Value {
    object_schema(vec![
        (
            "session_id",
            "string",
            "Opaque session id returned by start_session.",
            true,
        ),
        (
            "limit",
            "integer",
            "Maximum recent events to return, capped by the runtime.",
            false,
        ),
    ])
}

pub(crate) fn session_discussion_summary_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": {
                "type": "string",
                "description": "Required wc_sess_* id whose message board should be summarized."
            },
            "limit": {
                "type": "integer",
                "maximum": 100,
                "description": "Maximum recent progress/decision messages to return. Defaults to 50 and is clamped to 100."
            }
        },
        "required": ["session_id"],
        "additionalProperties": false,
    })
}

pub(crate) fn session_handoff_summary_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": {
                "type": "string",
                "description": "Required wc_sess_* id to summarize. This is business input; the tool never implicitly uses the current session."
            },
            "project": {
                "type": "string",
                "description": "Optional runtime project id. When provided, the handoff includes a bounded workspace summary and checkpoint candidates."
            },
            "include_workspace": {
                "type": "boolean",
                "description": "Include a bounded workspace (git status) summary. Defaults to true. Only effective when project is provided."
            },
            "include_checkpoints": {
                "type": "boolean",
                "description": "Include bounded checkpoint candidates, especially the latest last_known_good. Defaults to true. Only effective when project is provided."
            },
            "include_validation": {
                "type": "boolean",
                "description": "Include ledger-derived validation summary. Defaults to true. Minimal diagnostics require bounded tails or safe result metadata; parser.available remains false when session ledger events lack those fields."
            },
            "summary_only": {
                "type": "boolean",
                "description": "When true, return compact closeout fields only: workspace/jobs/permissions/tool_failures/validation/task_outcome/evidence_history/evidence_integrity/informational_notes/legacy verdict/warnings/suggested_next_actions. Omits recent_events, long ledger details, command text, stdout/stderr, tails, and excerpts."
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": 100,
                "description": "Maximum items per bounded section. Defaults to 20 and is clamped to 1..100."
            }
        },
        "required": ["session_id"],
        "additionalProperties": false,
    })
}

pub(crate) fn start_session_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "description": "Optional runtime project id associated with this task. This association does not bind the session as current by itself."
            },
            "title": {
                "type": "string",
                "description": "Optional human-readable task title."
            },
            "mode": session_mode_schema("Optional session mode. Defaults to normal. read_only automatically blocks write-like and shell/job-like tools."),
            "deny_write_tools": {
                "type": "boolean",
                "description": "Optional task guard. When true, write-like tools such as apply_patch, write_project_file, replace_line_range, insert_at_line, and delete_line_range are blocked before execution."
            },
            "deny_shell_tools": {
                "type": "boolean",
                "description": "Optional task guard. When true, shell/job-like tools such as run_shell, run_job, cargo_fmt, cargo_check, and cargo_test are blocked before execution."
            }
        },
        "required": [],
        "additionalProperties": false,
    })
}

pub(crate) fn current_session_input_schema(require_session_id: bool) -> Value {
    let mut fields = vec![(
            "project",
            "string",
            "Runtime project id whose process-local in-memory current-session binding should be inspected or updated.",
            true,
        )];
    if require_session_id {
        fields.push((
            "session_id",
            "string",
            "Existing project-scoped wc_sess_* id returned by start_session. Binding it is in-memory control metadata, not durable ledger persistence.",
            true,
        ));
    }
    object_schema(fields)
}
