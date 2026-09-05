use serde_json::{json, Value};

use super::common::{nullable_schema, wrapped_output_schema};
use webcodex_core::runner_protocol::RUNNER_CONFIG_RESTART_REQUIRED_FIELDS;

const ERROR_CODES: &[&str] = &[
    "invalid_request",
    "config_read_failed",
    "config_parse_failed",
    "config_validation_failed",
    "provider_config_invalid",
    "config_generation_conflict",
    "runner_unavailable",
    "runner_replaced",
    "capability_unavailable",
    "invalid_runner_response",
    "outcome_unknown",
];

const ERROR_FIELDS: &[&str] = &[
    "max_concurrent_jobs",
    "shell.max_persistent_shells",
    "shell.persistent_shell_idle_timeout_secs",
    "acp.max_concurrent_runs",
    "acp.permission_timeout_secs",
    "mcp.request_timeout_secs",
];

fn nullable_enum(values: &[&str], description: &str) -> Value {
    json!({
        "anyOf": [
            {"type": "string", "enum": values},
            {"type": "null"}
        ],
        "description": description,
    })
}

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    if !matches!(name, "runner_config_check" | "runner_config_reload") {
        return None;
    }
    Some(wrapped_output_schema(vec![
        (
            "action",
            json!({
                "type": "string",
                "enum": ["check", "reload"],
                "description": "Requested Runner config operation."
            }),
        ),
        (
            "execution_state",
            json!({
                "type": "string",
                "enum": ["not_started", "completed", "outcome_unknown"],
                "description": "Whether the exact Runner operation was proven not dispatched, completed with a typed result, or may have executed without a trustworthy response."
            }),
        ),
        (
            "valid",
            nullable_schema(
                "boolean",
                "Candidate validity when the Runner completed check/validation. null when the operation did not start or its outcome is unknown.",
            ),
        ),
        (
            "current_generation",
            json!({
                "anyOf": [
                    {"type": "integer", "minimum": 1},
                    {"type": "null"}
                ],
                "description": "Authoritative active-config generation when known. null after uncertain delivery or when no trustworthy Runner response exists."
            }),
        ),
        (
            "error_code",
            nullable_enum(ERROR_CODES, "Closed sanitized config-operation error code; never raw config, path, token, environment, or credential content."),
        ),
        (
            "error_field",
            nullable_enum(ERROR_FIELDS, "Allowlisted structural validation field when safely identifiable; otherwise null."),
        ),
        (
            "error_reason",
            nullable_enum(&["out_of_range"], "Allowlisted structural validation reason; otherwise null."),
        ),
        (
            "restart_required",
            json!({
                "type": "boolean",
                "description": "True exactly when restart_required_fields is non-empty."
            }),
        ),
        (
            "restart_required_fields",
            json!({
                "type": "array",
                "maxItems": RUNNER_CONFIG_RESTART_REQUIRED_FIELDS.len(),
                "uniqueItems": true,
                "items": {"type": "string", "enum": RUNNER_CONFIG_RESTART_REQUIRED_FIELDS},
                "description": "Exact allowlisted restart-only fields changed in the disk candidate. Hot fields may already be active after a successful partial reload; these fields are not claimed live until restart."
            }),
        ),
    ]))
}
