use serde_json::Value;

use super::common::{object_schema, with_optional_session_id};

pub(crate) fn git_diff_summary_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![(
        "project",
        "string",
        "Agent-registered project id.",
        true,
    )]))
}

pub(crate) fn show_changes_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
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

pub(crate) fn git_status_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![(
        "project",
        "string",
        "Configured project id.",
        true,
    )]))
}

pub(crate) fn git_diff_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Configured project id.", true),
        ("args", "array", "Optional path list.", false),
    ]))
}

pub(crate) fn git_diff_hunks_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
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
    ]))
}

pub(crate) fn git_log_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
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
