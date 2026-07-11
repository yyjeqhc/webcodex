use serde_json::{json, Value};

use super::common::{object_schema, with_optional_session_id, PATCH_FIELD_DESCRIPTION};

const SYNC_TIMEOUT_SECS_DESCRIPTION: &str = "Synchronous command timeout in seconds (minimum 1, maximum 120, default 120). Out-of-range values are rejected before the command starts; use run_job for longer work.";

fn with_sync_timeout_bounds(mut schema: Value) -> Value {
    schema["properties"]["timeout_secs"]["minimum"] = json!(1);
    schema["properties"]["timeout_secs"]["maximum"] = json!(120);
    schema["properties"]["timeout_secs"]["default"] = json!(120);
    schema
}

pub(crate) fn cargo_fmt_input_schema() -> Value {
    with_sync_timeout_bounds(object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "cwd",
            "string",
            "Optional project-relative working directory.",
            false,
        ),
        (
            "check",
            "boolean",
            "Run cargo fmt -- --check instead of formatting.",
            false,
        ),
        (
            "timeout_secs",
            "integer",
            SYNC_TIMEOUT_SECS_DESCRIPTION,
            false,
        ),
    ])))
}

pub(crate) fn cargo_check_input_schema() -> Value {
    with_sync_timeout_bounds(object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "cwd",
            "string",
            "Optional project-relative working directory.",
            false,
        ),
        (
            "all_targets",
            "boolean",
            "Include --all-targets (default true).",
            false,
        ),
        ("all_features", "boolean", "Include --all-features.", false),
        (
            "no_default_features",
            "boolean",
            "Include --no-default-features.",
            false,
        ),
        (
            "features",
            "string",
            "Feature list passed to --features.",
            false,
        ),
        ("package", "string", "Package passed to -p.", false),
        (
            "timeout_secs",
            "integer",
            SYNC_TIMEOUT_SECS_DESCRIPTION,
            false,
        ),
    ])))
}

pub(crate) fn cargo_test_input_schema() -> Value {
    with_sync_timeout_bounds(object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "cwd",
            "string",
            "Optional project-relative working directory.",
            false,
        ),
        ("filter", "string", "Optional cargo test filter.", false),
        ("all_targets", "boolean", "Include --all-targets.", false),
        ("all_features", "boolean", "Include --all-features.", false),
        (
            "no_default_features",
            "boolean",
            "Include --no-default-features.",
            false,
        ),
        (
            "features",
            "string",
            "Feature list passed to --features.",
            false,
        ),
        ("package", "string", "Package passed to -p.", false),
        ("no_run", "boolean", "Include --no-run.", false),
        (
            "timeout_secs",
            "integer",
            SYNC_TIMEOUT_SECS_DESCRIPTION,
            false,
        ),
    ])))
}

pub(crate) fn validate_patch_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("patch", "string", PATCH_FIELD_DESCRIPTION, true),
        (
            "deny_sensitive_paths",
            "boolean",
            "Block sensitive path warnings.",
            false,
        ),
    ]))
}
