use serde_json::{json, Value};

use super::super::super::tool_spec::ToolSpec;
use super::common::object_schema;
use crate::tool_runtime::sessions::TOOL_CALL_RECORDING_SESSION_ID_FIELD;
use crate::tool_runtime::tool_definition::runtime_tool_extra_accepted_flattened_args;
use crate::tool_runtime::ALLOW_CROSS_PROJECT_SESSION_FIELD;

pub(crate) fn list_tools_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "category": {
                "type": "string",
                "description": "Optional tool_manifest category filter such as artifact, edit, session, git, or runtime."
            },
            "features": {
                "type": "string",
                "description": "Optional loose feature filter such as artifact_upload, upload, read, edit, session, git, or validation."
            },
            "summary_only": {
                "type": "boolean",
                "description": "When true, omit full input/output schemas and return compact tool summaries."
            },
            "limit": {
                "type": "integer",
                "description": "Maximum returned tools for focused discovery; capped at 100."
            }
        },
        "required": [],
        "additionalProperties": false,
    })
}

pub(crate) fn tool_manifest_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "category": {
                "type": "string",
                "description": "Optional category filter (e.g. session, edit, git, checkpoint, runtime, job, validation)."
            },
            "include_recommended_flows": {
                "type": "boolean",
                "description": "Include recommended_flows in the output (default true)."
            },
            "include_risk_summary": {
                "type": "boolean",
                "description": "Include risk_summary in the output (default true)."
            }
        },
        "required": [],
        "additionalProperties": false,
    })
}

pub(crate) fn runtime_status_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "compact": {
                "type": "boolean",
                "description": "When true, return compact runtime observability with service/version, build revision, tool/job counts, agent health summary, and project effective/server status. Defaults to false."
            },
            "summary_only": {
                "type": "boolean",
                "description": "Alias for compact=true. Returns the same compact runtime observability shape. Defaults to false."
            }
        },
        "required": [],
        "additionalProperties": false,
    })
}

pub(crate) fn empty_input_schema() -> Value {
    object_schema(vec![])
}

pub(crate) const ACCEPTED_FLATTENED_ARG_PREFERRED_ORDER: &[&str] = &[
    "project",
    "path",
    "title",
    "session_id",
    "bind_current",
    "include_runtime_status",
    "compact_startup",
    "compact",
    "include_git",
    "include_recent_commits",
    "include_rules",
    "include_tool_manifest",
    "tool_manifest_categories",
    "tool_manifest_limit",
    "include_diff",
    "include_hygiene",
    "include_handoff",
    "include_validation_summary",
    "include_validation",
    "include_workspace",
    "include_checkpoints",
    "category",
    "features",
    "summary_only",
    "limit",
    "allow_missing",
    "upload_id",
    ALLOW_CROSS_PROJECT_SESSION_FIELD,
    "offset",
    "content_base64",
    "expected_bytes",
    "expected_sha256",
    "mime_type",
    "overwrite",
];

pub(crate) fn accepted_flattened_args_for_spec(spec: &ToolSpec) -> Vec<String> {
    let Some(properties) = spec.input_schema["properties"].as_object() else {
        return vec![TOOL_CALL_RECORDING_SESSION_ID_FIELD.to_string()];
    };
    let mut names = Vec::new();
    for field in ACCEPTED_FLATTENED_ARG_PREFERRED_ORDER {
        if properties.contains_key(*field) {
            names.push((*field).to_string());
        }
    }
    let mut remaining: Vec<&str> = properties
        .keys()
        .map(String::as_str)
        .filter(|field| !ACCEPTED_FLATTENED_ARG_PREFERRED_ORDER.contains(field))
        .collect();
    remaining.sort_unstable();
    names.extend(remaining.into_iter().map(str::to_string));
    for field in runtime_tool_extra_accepted_flattened_args(&spec.name) {
        push_unique_flattened_arg(&mut names, field);
    }
    push_unique_flattened_arg(&mut names, TOOL_CALL_RECORDING_SESSION_ID_FIELD);
    names
}

fn push_unique_flattened_arg(names: &mut Vec<String>, field: &str) {
    if !names.iter().any(|name| name == field) {
        names.push(field.to_string());
    }
}
