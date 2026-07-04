use serde_json::{json, Value};

mod artifacts;
mod cleanup;
mod coding;
mod common;
mod discovery;
mod files;
mod git;
mod hygiene;
mod jobs;
mod line_edits;
mod patches;
mod projects;
mod sessions;
mod text_edits;
mod validation;

use super::super::tool_inputs::{CHECKPOINT_KIND_VALUES, CHECKPOINT_VALIDATION_STATUS_VALUES};
use super::super::tool_spec::ToolSpec;
pub(super) use artifacts::{
    artifact_upload_abort_input_schema, artifact_upload_begin_input_schema,
    artifact_upload_chunk_input_schema, artifact_upload_finish_input_schema,
    read_project_artifact_input_schema, read_project_artifact_metadata_input_schema,
    save_project_artifact_input_schema,
};
pub(super) use cleanup::{
    delete_project_files_input_schema, discard_untracked_input_schema,
    git_restore_paths_input_schema,
};
pub(super) use coding::{finish_coding_task_input_schema, start_coding_task_input_schema};
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
pub(super) use hygiene::workspace_hygiene_check_input_schema;
pub(super) use jobs::{
    job_log_input_schema, job_status_input_schema, job_tail_input_schema, list_jobs_input_schema,
    run_codex_input_schema, run_job_input_schema, run_shell_input_schema, stop_job_input_schema,
};
pub(super) use line_edits::{
    apply_text_edits_input_schema, delete_line_range_input_schema, insert_at_line_input_schema,
    replace_line_range_input_schema,
};
pub(super) use patches::{apply_patch_checked_input_schema, apply_patch_input_schema};
pub(super) use projects::{create_project_input_schema, register_project_input_schema};
pub(super) use sessions::{
    current_session_input_schema, list_session_messages_input_schema,
    post_session_message_input_schema, resolve_session_message_input_schema,
    session_discussion_summary_input_schema, session_guards_schema,
    session_handoff_summary_input_schema, session_mode_schema, session_summary_input_schema,
    start_session_input_schema,
};
pub(super) use text_edits::{
    insert_after_pattern_input_schema, insert_before_pattern_input_schema,
    replace_exact_block_input_schema, replace_in_file_input_schema,
    write_project_file_input_schema,
};
pub(super) use validation::{
    cargo_check_input_schema, cargo_fmt_input_schema, cargo_test_input_schema,
    validate_patch_input_schema,
};

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
