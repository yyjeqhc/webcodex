use serde_json::{json, Value};

mod common;
mod discovery;
mod files;
mod git;
mod jobs;
mod patches;
mod projects;
mod validation;

use super::super::tool_inputs::{CHECKPOINT_KIND_VALUES, CHECKPOINT_VALIDATION_STATUS_VALUES};
use super::super::tool_spec::ToolSpec;
use common::{object_schema, with_optional_session_id, OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION};
pub(crate) use discovery::accepted_flattened_args_for_spec;
pub(super) use discovery::{
    empty_input_schema, list_tools_input_schema, tool_manifest_input_schema,
};
pub(super) use files::{
    list_project_files_input_schema, read_file_input_schema, search_project_text_input_schema,
};
pub(super) use git::{
    git_diff_hunks_input_schema, git_diff_input_schema, git_diff_summary_input_schema,
    git_log_input_schema, git_status_input_schema, show_changes_input_schema,
};
pub(super) use jobs::{
    job_log_input_schema, job_status_input_schema, job_tail_input_schema, list_jobs_input_schema,
    run_codex_input_schema, run_job_input_schema, run_shell_input_schema, stop_job_input_schema,
};
pub(super) use patches::{apply_patch_checked_input_schema, apply_patch_input_schema};
pub(super) use projects::{create_project_input_schema, register_project_input_schema};
pub(super) use validation::{
    cargo_check_input_schema, cargo_fmt_input_schema, cargo_test_input_schema,
    validate_patch_input_schema,
};

pub(super) fn delete_project_files_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "paths",
            "array",
            "Project-relative file paths to delete.",
            true,
        ),
    ]))
}

pub(super) fn git_restore_paths_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "paths",
            "array",
            "Project-relative tracked paths to restore.",
            true,
        ),
    ]))
}

pub(super) fn discard_untracked_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "paths",
            "array",
            "Project-relative untracked paths to remove.",
            true,
        ),
    ]))
}

pub(super) fn replace_in_file_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        ("old", "string", "Non-empty substring to replace.", true),
        ("new", "string", "Replacement string.", true),
        (
            "expected_replacements",
            "integer",
            "Expected occurrence count (default 1).",
            false,
        ),
        (
            "allow_multiple",
            "boolean",
            "Allow replacing multiple occurrences (default false).",
            false,
        ),
    ]))
}

pub(super) fn replace_exact_block_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        (
            "old_text",
            "string",
            "Non-empty literal block; must match exactly once.",
            true,
        ),
        (
            "new_text",
            "string",
            "Replacement text; may be empty to delete the block.",
            true,
        ),
        (
            "expected_old_sha256",
            "string",
            "Optional sha256 guard for current whole-file content.",
            false,
        ),
    ]))
}

pub(super) fn insert_before_pattern_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        (
            "pattern",
            "string",
            "Non-empty literal pattern; must match exactly once.",
            true,
        ),
        (
            "text",
            "string",
            "Non-empty text to insert, including intended newlines.",
            true,
        ),
    ]))
}

pub(super) fn insert_after_pattern_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        (
            "pattern",
            "string",
            "Non-empty literal pattern; must match exactly once.",
            true,
        ),
        (
            "text",
            "string",
            "Non-empty text to insert, including intended newlines.",
            true,
        ),
    ]))
}

pub(super) fn write_project_file_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        ("content", "string", "UTF-8 file content (no NUL).", true),
        (
            "overwrite",
            "boolean",
            "Allow overwriting an existing file (default false).",
            false,
        ),
        (
            "expected_sha256",
            "string",
            "Required sha256 of the current file when overwriting.",
            false,
        ),
        (
            "expected_content_prefix",
            "string",
            "Required prefix of the current file when overwriting.",
            false,
        ),
    ]))
}

pub(super) fn save_project_artifact_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative output path.", true),
        (
            "content_base64",
            "string",
            "Base64-encoded binary content.",
            true,
        ),
        ("mime_type", "string", "Optional MIME type.", false),
        (
            "overwrite",
            "boolean",
            "Allow overwriting an existing file (default false).",
            false,
        ),
    ]))
}

pub(super) fn read_project_artifact_metadata_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative artifact path.", true),
        (
            "allow_missing",
            "boolean",
            "When true, a missing artifact returns exists=false instead of an error.",
            false,
        ),
    ]))
}

pub(super) fn read_project_artifact_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative artifact path.", true),
        (
            "encoding",
            "string",
            "Optional encoding; only base64 is supported (default base64).",
            false,
        ),
        (
            "offset",
            "integer",
            "Optional byte offset to start reading from; defaults to 0.",
            false,
        ),
        (
            "length",
            "integer",
            "Optional chunk length in bytes; defaults to 32768 and cannot exceed 65536.",
            false,
        ),
        (
            "max_bytes",
            "integer",
            "Compatibility alias/upper bound for length; cannot exceed 65536.",
            false,
        ),
    ]))
}

pub(super) fn artifact_upload_begin_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative output path.", true),
        (
            "expected_bytes",
            "integer",
            "Optional final byte count guard.",
            false,
        ),
        (
            "expected_sha256",
            "string",
            "Optional final sha256 guard.",
            false,
        ),
        ("mime_type", "string", "Optional MIME type.", false),
        (
            "overwrite",
            "boolean",
            "Allow overwriting an existing file at finish (default false).",
            false,
        ),
    ]))
}

pub(super) fn artifact_upload_chunk_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "path",
            "string",
            "Required project-relative path; must exactly match the path used in artifact_upload_begin to bind upload_id to the target.",
            true,
        ),
        (
            "upload_id",
            "string",
            "Opaque wc_upload_* id from artifact_upload_begin.",
            true,
        ),
        ("offset", "integer", "Expected current upload byte offset.", true),
        (
            "content_base64",
            "string",
            "Base64-encoded chunk; decoded chunk max is 65536 bytes.",
            true,
        ),
    ]))
}

pub(super) fn artifact_upload_finish_input_schema() -> Value {
    artifact_upload_followup_input_schema()
}

pub(super) fn artifact_upload_abort_input_schema() -> Value {
    artifact_upload_followup_input_schema()
}

fn artifact_upload_followup_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "path",
            "string",
            "Required project-relative path; must exactly match the path used in artifact_upload_begin to bind upload_id to the target.",
            true,
        ),
        (
            "upload_id",
            "string",
            "Opaque wc_upload_* id from artifact_upload_begin.",
            true,
        ),
    ]))
}

pub(super) fn replace_line_range_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        (
            "start_line",
            "integer",
            "1-based inclusive start line.",
            true,
        ),
        ("end_line", "integer", "1-based inclusive end line.", true),
        (
            "new_text",
            "string",
            "Replacement text; empty deletes the range.",
            true,
        ),
        (
            "expected_old_sha256",
            "string",
            "Optional sha256 guard for the original range text.",
            false,
        ),
        (
            "expected_old_prefix",
            "string",
            "Optional prefix guard for the original range text.",
            false,
        ),
    ]))
}

pub(super) fn insert_at_line_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        (
            "line",
            "integer",
            "1-based insertion line; total_lines+1 appends at EOF.",
            true,
        ),
        ("text", "string", "Text to insert.", true),
        (
            "expected_anchor_sha256",
            "string",
            "Optional sha256 guard for anchor line or empty EOF anchor.",
            false,
        ),
        (
            "expected_anchor_prefix",
            "string",
            "Optional prefix guard for anchor line or empty EOF anchor.",
            false,
        ),
    ]))
}

pub(super) fn delete_line_range_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative file path.", true),
        (
            "start_line",
            "integer",
            "1-based inclusive start line.",
            true,
        ),
        ("end_line", "integer", "1-based inclusive end line.", true),
        (
            "expected_old_sha256",
            "string",
            "Optional sha256 guard for the original range text.",
            false,
        ),
        (
            "expected_old_prefix",
            "string",
            "Optional prefix guard for the original range text.",
            false,
        ),
    ]))
}

pub(super) fn apply_text_edits_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "description": "Agent-registered project id."
            },
            "path": {
                "type": "string",
                "description": "Project-relative file path."
            },
            "edits": {
                "type": "array",
                "minItems": 1,
                "maxItems": 20,
                "description": "Ordered list of 1..20 atomic edits.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "kind": {
                            "type": "string",
                            "enum": [
                                "replace_exact",
                                "insert_after",
                                "insert_before",
                                "delete_exact"
                            ],
                            "description": "Atomic edit kind."
                        },
                        "old_text": {
                            "type": "string",
                            "description": "Exact text to replace or delete, required by replace_exact/delete_exact."
                        },
                        "new_text": {
                            "type": "string",
                            "description": "Replacement or inserted text, required by replace_exact/insert_before/insert_after."
                        },
                        "anchor_text": {
                            "type": "string",
                            "description": "Unique anchor text required by insert_before/insert_after."
                        }
                    },
                    "required": ["kind"]
                }
            },
            "dry_run": {
                "type": "boolean",
                "description": "If true, compute the plan without writing."
            },
            "expected_file_sha256": {
                "type": "string",
                "description": "Optional sha256 guard for the whole original file."
            },
            "session_id": {
                "type": "string",
                "description": OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION
            }
        },
        "required": ["project", "path", "edits"],
        "additionalProperties": false
    })
}

pub(super) fn session_mode_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "enum": ["normal", "read_only"],
        "description": description,
    })
}

pub(super) fn session_guards_schema(description: &str) -> Value {
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

pub(super) fn post_session_message_input_schema() -> Value {
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

pub(super) fn list_session_messages_input_schema() -> Value {
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

pub(super) fn resolve_session_message_input_schema() -> Value {
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

pub(super) fn session_summary_input_schema() -> Value {
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

pub(super) fn session_discussion_summary_input_schema() -> Value {
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

pub(super) fn session_handoff_summary_input_schema() -> Value {
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
                "description": "When true, return compact verdict fields only: workspace/jobs/permissions/tool_failures/validation/warnings/suggested_next_actions. Omits recent_events, long ledger details, command text, stdout/stderr, tails, and excerpts."
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

pub(super) fn workspace_hygiene_check_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "description": "Runtime project id."
            },
            "max_findings": {
                "type": "integer",
                "minimum": 1,
                "maximum": 200,
                "description": "Maximum findings to return (default 50, clamped to 1..200)."
            },
            "include_tracked": {
                "type": "boolean",
                "description": "Also report tracked suspicious path names (default false). When false, only untracked entries and the dirty-worktree summary are reported. Never reads file contents."
            },
            "session_id": {
                "type": "string",
                "description": OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION
            }
        },
        "required": ["project"],
        "additionalProperties": false,
    })
}

pub(super) fn start_session_input_schema() -> Value {
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

pub(super) fn start_coding_task_input_schema() -> Value {
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

pub(super) fn finish_coding_task_input_schema() -> Value {
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

pub(super) fn current_session_input_schema(require_session_id: bool) -> Value {
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

pub(super) fn checkpoint_project_input_schema(
    fields: Vec<(&'static str, &'static str, &'static str, bool)>,
) -> Value {
    object_schema(with_optional_session_id(fields))
}

pub(super) fn checkpoint_list_input_schema() -> Value {
    checkpoint_project_input_schema(vec![
        ("project", "string", "Runtime project id.", true),
        (
            "limit",
            "integer",
            "Maximum checkpoints to return (default 20, max 100).",
            false,
        ),
    ])
}

pub(super) fn checkpoint_show_input_schema() -> Value {
    checkpoint_project_input_schema(vec![
        ("project", "string", "Runtime project id.", true),
        (
            "checkpoint_id",
            "string",
            "wc_ckpt_* id returned by workspace_checkpoint_create.",
            true,
        ),
        (
            "include_diff_stat",
            "boolean",
            "Include tracked/staged diff stat strings (default false).",
            false,
        ),
    ])
}

pub(super) fn checkpoint_restore_input_schema() -> Value {
    checkpoint_project_input_schema(vec![
        ("project", "string", "Runtime project id.", true),
        ("checkpoint_id", "string", "wc_ckpt_* id to restore.", true),
        ("confirm", "boolean", "Must be true to restore.", true),
    ])
}

pub(super) fn checkpoint_delete_input_schema() -> Value {
    checkpoint_project_input_schema(vec![
        ("project", "string", "Runtime project id.", true),
        ("checkpoint_id", "string", "wc_ckpt_* id to delete.", true),
        ("confirm", "boolean", "Must be true to delete.", true),
    ])
}

pub(super) fn checkpoint_validation_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": false,
        "properties": {
            "status": {
                "type": "string",
                "enum": CHECKPOINT_VALIDATION_STATUS_VALUES,
                "description": "Validation result supplied by the caller. The runtime records metadata only and never runs these commands."
            },
            "commands": {
                "type": "array",
                "items": { "type": "string", "maxLength": 200 },
                "maxItems": 20,
                "description": "Command summaries supplied by the caller. Stdout/stderr and env values are not stored."
            },
            "summary": {
                "anyOf": [
                    { "type": "string" },
                    { "type": "null" }
                ],
                "maxLength": 500,
                "description": "Short validation summary supplied by the caller."
            }
        },
        "required": [],
    })
}

pub(super) fn checkpoint_labels_schema(description: &str) -> Value {
    json!({
        "type": "array",
        "items": {
            "type": "string",
            "maxLength": 64,
            "pattern": "^[A-Za-z0-9._-]+$"
        },
        "maxItems": 20,
        "description": description,
    })
}

pub(super) fn checkpoint_create_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "description": "Runtime project id."
            },
            "title": {
                "type": "string",
                "description": "Optional human-readable title."
            },
            "note": {
                "type": "string",
                "description": "Optional note; not used by restore."
            },
            "include_untracked": {
                "type": "boolean",
                "description": "Include small non-secret UTF-8 untracked files (default false)."
            },
            "kind": {
                "type": "string",
                "enum": CHECKPOINT_KIND_VALUES,
                "description": "Optional semantic checkpoint kind. Defaults to snapshot."
            },
            "labels": checkpoint_labels_schema("Optional simple ASCII labels for handoff, filtering, or recovery hints."),
            "validation": checkpoint_validation_schema("Optional bounded validation metadata supplied by the caller."),
            "session_id": {
                "type": "string",
                "description": OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION
            }
        },
        "required": ["project"],
        "additionalProperties": false,
    })
}

pub(super) fn with_common_testing_metadata(mut spec: ToolSpec) -> ToolSpec {
    let Some(properties) = spec
        .input_schema
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    else {
        return spec;
    };
    properties.entry("expected_failure".to_string()).or_insert_with(|| {
        json!({
            "type": "boolean",
            "description": "Optional testing/smoke metadata only. When true, a failed call is classified as an expected failure in session handoff/finish summaries. Does not change authorization, permission, execution, hard guards, command_started, or the immediate success/error result."
        })
    });
    properties
        .entry("expected_failure_kind".to_string())
        .or_insert_with(|| {
            json!({
                "type": "string",
                "description": "Optional testing/smoke metadata only. Expected structured failure_kind or error_kind for an expected failure. Does not change tool behavior or safety decisions."
            })
        });
    properties
        .entry("test_expect_failure_kind".to_string())
        .or_insert_with(|| {
            json!({
                "type": "string",
                "description": "Alias for expected_failure_kind for testing/smoke callers. Matches structured failure_kind or error_kind and does not change tool behavior."
            })
        });
    properties.entry("assertion_name".to_string()).or_insert_with(|| {
        json!({
            "type": "string",
            "description": "Optional testing/smoke assertion label recorded in the session ledger. Does not change authorization, permission, execution, or immediate tool output."
        })
    });
    spec
}
