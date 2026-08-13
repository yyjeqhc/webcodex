use serde_json::{json, Value};

use super::common::{object_schema, with_optional_session_id, PATCH_FIELD_DESCRIPTION};

/// `timeout_secs` for read-only structured validation tools is the total
/// runtime budget of the command. Short validations return immediately; a
/// long validation continues as a Job and returns `job_id`. The tool call
/// itself blocks only a short internal sync window.
const VALIDATION_TIMEOUT_SECS_DESCRIPTION: &str =
    "Total runtime budget for the validation command in seconds (minimum 1, maximum 3600). Short validations return immediately; a longer validation continues as a Job and returns job_id. Defaults vary per tool. Out-of-range values are rejected before the command starts.";
const VALIDATION_TIMEOUT_MIN: u64 = 1;
const VALIDATION_TIMEOUT_MAX: u64 = 3600;

fn with_validation_timeout_bounds(mut schema: Value, default: u64) -> Value {
    schema["properties"]["timeout_secs"]["minimum"] = json!(VALIDATION_TIMEOUT_MIN);
    schema["properties"]["timeout_secs"]["maximum"] = json!(VALIDATION_TIMEOUT_MAX);
    schema["properties"]["timeout_secs"]["default"] = json!(default);
    schema["properties"]["timeout_secs"]["description"] =
        json!(VALIDATION_TIMEOUT_SECS_DESCRIPTION);
    schema
}

pub(crate) fn cargo_fmt_input_schema() -> Value {
    // `cargo_fmt(check=false)` mutates source and keeps the existing explicit
    // synchronous semantics: it never auto-promotes to a Job, so its
    // `timeout_secs` stays a synchronous command timeout. Only `check=true`
    // accepts the long read-only budget.
    let mut schema = object_schema(with_optional_session_id(vec![
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
            "Synchronous command timeout in seconds (minimum 1, maximum 120, default 120) for cargo fmt (mutating); out-of-range values are rejected before the command starts. When check=true, this is the total validation runtime budget (minimum 1, maximum 3600, default 120); a longer check continues as a Job and returns job_id.",
            false,
        ),
    ]));
    schema["properties"]["timeout_secs"]["minimum"] = json!(1);
    schema["properties"]["timeout_secs"]["maximum"] = json!(3600);
    schema["properties"]["timeout_secs"]["default"] = json!(120);
    schema["allOf"] = json!([{
        "if": {
            "required": ["check"],
            "properties": { "check": { "const": true } }
        },
        "then": {
            "properties": {
                "timeout_secs": { "type": "integer", "minimum": 1, "maximum": 3600 }
            }
        },
        "else": {
            "properties": {
                "timeout_secs": { "type": "integer", "minimum": 1, "maximum": 120 }
            }
        }
    }]);
    schema
}

pub(crate) fn cargo_check_input_schema() -> Value {
    with_validation_timeout_bounds(
        object_schema(with_optional_session_id(vec![
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
                VALIDATION_TIMEOUT_SECS_DESCRIPTION,
                false,
            ),
        ])),
        600,
    )
}

pub(crate) fn cargo_test_input_schema() -> Value {
    with_validation_timeout_bounds(
        object_schema(with_optional_session_id(vec![
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
                VALIDATION_TIMEOUT_SECS_DESCRIPTION,
                false,
            ),
        ])),
        1800,
    )
}

pub(crate) fn go_test_input_schema() -> Value {
    with_validation_timeout_bounds(
        object_schema(with_optional_session_id(vec![
            ("project", "string", "Agent-registered project id.", true),
            (
                "cwd",
                "string",
                "Optional project-relative working directory.",
                false,
            ),
            (
                "timeout_secs",
                "integer",
                VALIDATION_TIMEOUT_SECS_DESCRIPTION,
                false,
            ),
        ])),
        1800,
    )
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
