use serde_json::{json, Value};

use super::common::{object_schema, with_optional_session_id};

pub fn git_diff_summary_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![(
        "project",
        "string",
        "Runner-registered project id.",
        true,
    )]))
}

pub fn git_review_summary_input_schema() -> Value {
    let mut schema = object_schema(with_optional_session_id(vec![
        ("project", "string", "Runner-registered project id.", true),
        (
            "base_commit",
            "string",
            "Exact 40-hex Git commit object id used to compute the merge-base.",
            true,
        ),
        (
            "head_commit",
            "string",
            "Exact 40-hex Git commit object id reviewed from merge-base to head.",
            true,
        ),
    ]));
    for field in ["base_commit", "head_commit"] {
        schema["properties"][field]["minLength"] = Value::from(40);
        schema["properties"][field]["maxLength"] = Value::from(40);
        schema["properties"][field]["pattern"] = Value::from("^[0-9A-Fa-f]{40}$");
    }
    schema
}

pub fn show_changes_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Runner-registered project id.", true),
        (
            "session_id",
            "string",
            "Optional wc_sess_* id to summarize with the git changes.",
            false,
        ),
        (
            "include_diff",
            "boolean",
            "Include bounded diff hunks (default false).",
            false,
        ),
        (
            "max_hunks",
            "integer",
            "Maximum hunks to return when include_diff=true (clamped).",
            false,
        ),
        (
            "max_hunk_lines",
            "integer",
            "Maximum lines per hunk when include_diff=true (clamped).",
            false,
        ),
        (
            "session_event_limit",
            "integer",
            "Maximum recent session events to include (clamped).",
            false,
        ),
    ]))
}

pub fn git_status_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![(
        "project",
        "string",
        "Configured project id.",
        true,
    )]))
}

pub fn git_diff_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Configured project id.", true),
        ("args", "array", "Optional path list.", false),
    ]))
}

pub fn git_diff_hunks_input_schema() -> Value {
    let mut schema = object_schema(with_optional_session_id(vec![
        ("project", "string", "Runner-registered project id.", true),
        (
            "paths",
            "array",
            "Optional project-relative paths to scope diff.",
            false,
        ),
        (
            "max_hunks",
            "integer",
            "Maximum hunks to return (clamped).",
            false,
        ),
        (
            "max_hunk_lines",
            "integer",
            "Maximum lines per hunk (clamped).",
            false,
        ),
        (
            "cached",
            "boolean",
            "Use staged diff via git diff --cached.",
            false,
        ),
        (
            "base_commit",
            "string",
            "Optional exact 40-hex Git commit object id; requires head_commit and committed-range mode.",
            false,
        ),
        (
            "head_commit",
            "string",
            "Optional exact 40-hex Git commit object id reviewed from the single merge-base; requires base_commit.",
            false,
        ),
        (
            "continuation",
            "string",
            "Opaque continuation returned by a previous git_diff_hunks page. When continuing, repeat the exact original diff scope and paging inputs unchanged (base_commit/head_commit for committed mode, cached/worktree mode, paths, max_hunks, and max_hunk_lines); the token is scope-bound and does not reconstruct omitted request fields.",
            false,
        ),
    ]));
    for field in ["base_commit", "head_commit"] {
        schema["properties"][field]["minLength"] = Value::from(40);
        schema["properties"][field]["maxLength"] = Value::from(40);
        schema["properties"][field]["pattern"] = Value::from("^[0-9A-Fa-f]{40}$");
    }
    schema["properties"]["continuation"]["maxLength"] =
        Value::from(webcodex_core::runtime_contract::GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES);
    schema["allOf"] = json!([
        {
            "if": { "required": ["base_commit"] },
            "then": {
                "required": ["head_commit"],
                "not": { "required": ["cached"] }
            }
        },
        {
            "if": { "required": ["head_commit"] },
            "then": {
                "required": ["base_commit"],
                "not": { "required": ["cached"] }
            }
        },
        {
            "if": { "required": ["cached"] },
            "then": {
                "not": {
                    "anyOf": [
                        { "required": ["base_commit"] },
                        { "required": ["head_commit"] }
                    ]
                }
            }
        }
    ]);
    schema
}

pub fn git_log_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Runner-registered project id.", true),
        (
            "limit",
            "integer",
            "Maximum commits to return (default 20, clamped to 1..100).",
            false,
        ),
        (
            "skip",
            "integer",
            "Number of recent commits to skip (default 0, clamped to 0..10000).",
            false,
        ),
    ]))
}
