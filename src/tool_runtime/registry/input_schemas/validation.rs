use serde_json::{json, Value};

use super::common::{object_schema, with_optional_session_id, PATCH_FIELD_DESCRIPTION};

/// `timeout_secs` for read-only structured validation tools is the total
/// runtime budget of the command. Short validations return immediately; a
/// long validation continues as the same execution and returns `job_id`. The tool call
/// itself blocks only a short internal sync window.
const VALIDATION_TIMEOUT_SECS_DESCRIPTION: &str =
    "Total validation runtime budget in seconds (1..=3600). Short validation returns immediately; longer validation keeps the same execution and returns job_id for observation. Defaults vary per tool; invalid values are rejected before start.";
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
            "For mutating format, synchronous timeout in seconds (1..=120, default 120). With check=true, total validation budget is 1..=3600 and a long check keeps the same execution and returns job_id.",
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
    let mut schema = with_validation_timeout_bounds(
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
                "require_tests",
                "boolean",
                "Require proof that at least one test executed. Compatible default is false.",
                false,
            ),
            (
                "min_tests",
                "integer",
                "Require proof that at least this many tests executed. Combined with require_tests using the stricter minimum.",
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
    );
    schema["properties"]["min_tests"]["minimum"] = json!(1);
    schema["properties"]["min_tests"]["maximum"] =
        json!(crate::shell_protocol::CARGO_TEST_MIN_TESTS_MAX);
    schema["allOf"] = json!([{
        "if": {
            "properties": { "no_run": { "const": true } },
            "required": ["no_run"]
        },
        "then": {
            "properties": {
                "require_tests": { "enum": [false] },
                "min_tests": { "enum": [] }
            }
        }
    }]);
    schema
}

pub(crate) fn go_test_input_schema() -> Value {
    let mut schema = with_validation_timeout_bounds(
        object_schema(with_optional_session_id(vec![
            ("project", "string", "Agent-registered project id.", true),
            (
                "cwd",
                "string",
                "Optional project-relative working directory.",
                false,
            ),
            (
                "packages",
                "array",
                "Optional 1..8 project-relative Go package patterns: '.', './path', './...', or './path/...'.",
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
    );
    schema["properties"]["packages"]["minItems"] = json!(1);
    schema["properties"]["packages"]["maxItems"] =
        json!(crate::shell_protocol::GO_TEST_PACKAGE_MAX_ITEMS);
    schema["properties"]["packages"]["items"]["minLength"] = json!(1);
    schema["properties"]["packages"]["items"]["maxLength"] =
        json!(crate::shell_protocol::GO_TEST_PACKAGE_MAX_BYTES);
    schema
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
