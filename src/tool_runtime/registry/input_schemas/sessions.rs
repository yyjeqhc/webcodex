use serde_json::{json, Value};

use super::common::object_schema;

pub(crate) fn session_mode_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "enum": ["normal", "inspect", "read_only"],
        "description": description,
    })
}

/// Workflow session lifecycle wire values.
///
/// Phase 2: create yields `active`; explicit `close_session` yields `closed`.
/// `archived` remains reserved for later phases. Missing ledger field → active.
pub(crate) fn session_lifecycle_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "enum": ["active", "closed", "archived"],
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

pub(crate) fn session_execution_context_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": false,
        "properties": {
            "default_cwd": {
                "type": "string",
                "minLength": 1,
                "maxLength": 4096,
                "description": "Optional working-directory default for the closed set of Session-aware execution tools. Without resource it is project-relative. With a named SSH resource it becomes a remote-path default only for remote-capable shell execution; local structured process/script execution rejects that resource before start."
            },
            "default_shell": {
                "type": "string",
                "enum": ["sh", "bash"],
                "description": "Optional explicit shell dialect inherited by Session-aware shell execution when the per-call shell is omitted. It does not affect structured process or script execution."
            },
            "resource": {
                "type": "string",
                "minLength": 1,
                "maxLength": 80,
                "pattern": "^[A-Za-z0-9_.-]+$",
                "description": "Optional named SSH resource configured only on the Runner that owns this Session project. It routes supported one-shot, background, and persistent shell execution remotely; structured process/script and Cargo/Go validation reject it before execution. It never contains host, SSH configuration, key, password, or connection data."
            }
        }
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
            "message_id": {
                "type": "string",
                "description": "Optional exact wc_msg_* filter. Combined with kind/status/reply_to using deterministic AND semantics; returns exact 0/1 when this filter is supplied."
            },
            "reply_to": {
                "type": "string",
                "description": "Optional exact reply_to wc_msg_* filter, useful for finding replies to one todo. Combined with all other filters using AND semantics."
            },
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

pub(crate) fn observe_session_messages_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": {
                "type": "string",
                "pattern": "^wc_sess_[A-Za-z0-9_]+$",
                "description": "Required explicit Workflow Session whose message-state delta is observed."
            },
            "after_observation_token": {
                "type": "string",
                "maxLength": 192,
                "description": "Optional opaque Session-bound durable observation token returned by an earlier observe_session_messages call."
            },
            "wait_secs": {
                "type": "integer",
                "minimum": 1,
                "maximum": 60,
                "description": "Optional one-shot bounded wait in seconds. Allowed only with after_observation_token; never creates a subscription or stream."
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": 100,
                "description": "Maximum retained current-state message changes returned. Defaults to 50."
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

pub(crate) fn complete_session_message_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": {
                "type": "string",
                "description": "Required coordinator/business wc_sess_* id containing the exact open todo."
            },
            "message_id": {
                "type": "string",
                "description": "Exact open todo wc_msg_* id to answer and resolve atomically."
            },
            "answer": {
                "type": "string",
                "minLength": 1,
                "maxLength": 8000,
                "description": "Bounded answer body stored once as a kind=answer message replying to the todo."
            },
            "completion_key": {
                "type": "string",
                "minLength": 1,
                "maxLength": 128,
                "description": "Caller-generated idempotency key for this exact completion. Same key and same answer returns the original result; conflicting reuse fails closed."
            },
            "tags": {
                "type": "array",
                "items": { "type": "string", "maxLength": 64 },
                "maxItems": 16,
                "description": "Optional tags on the created answer."
            },
            "priority": session_message_priority_schema("Optional answer priority; defaults to normal.")
        },
        "required": ["session_id", "message_id", "answer", "completion_key"],
        "additionalProperties": false,
    })
}

pub(crate) fn session_summary_input_schema() -> Value {
    object_schema(vec![
        (
            "session_id",
            "string",
            "Opaque wc_sess_* id returned by work_on_project or another compatible Session bootstrap.",
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

pub(crate) fn close_session_input_schema() -> Value {
    object_schema(vec![(
        "session_id",
        "string",
        "Required explicit wc_sess_* id to close. Never falls back to current-session. Unknown ids fail without creating a session. Idempotent when already closed. finish_coding_task does not close.",
        true,
    )])
}

pub(crate) fn update_session_context_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "minLength": 1,
                "description": "Required complete runtime project id or unambiguous project input. The caller must be authorized for the resolved project, and it must exactly match the Session project."
            },
            "session_id": {
                "type": "string",
                "pattern": "^wc_sess_[A-Za-z0-9_]+$",
                "description": "Required explicit active, project-scoped Workflow Session id. Never falls back to a current binding and never creates an unknown Session."
            },
            "execution_context": session_execution_context_schema(
                "Complete replacement execution context. `{}` clears all defaults. The context cannot store environment variables, credentials, SSH host/configuration, keys, passwords, connections, or arbitrary options."
            )
        },
        "required": ["project", "session_id", "execution_context"],
        "additionalProperties": false
    })
}

pub(crate) fn validation_summary_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "minLength": 1,
                "description": "Required complete runtime project id from list_projects. Must match the project scoped to session_id."
            },
            "session_id": {
                "type": "string",
                "minLength": 1,
                "description": "Required explicit wc_sess_* business session id. The tool never falls back to current session."
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": 100,
                "default": 20,
                "description": "Maximum validation history events returned. Defaults to 20 and is clamped to 1..100; per-event parser evidence keeps its own fixed bounds."
            }
        },
        "required": ["project", "session_id"],
        "additionalProperties": false
    })
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

pub(crate) fn current_session_input_schema(require_session_id: bool) -> Value {
    let mut fields = vec![(
        "project",
        "string",
        "Runtime project id whose exact window/caller/transport/project/canonical-root current-session binding should be inspected or updated.",
        true,
    )];
    if require_session_id {
        fields.push((
            "session_id",
            "string",
            "Existing active project-scoped wc_sess_* id returned by work_on_project or another compatible Session bootstrap. Binding updates the process-local cache and hashed durable ledger projection without changing Session history.",
            true,
        ));
    }
    object_schema(fields)
}
