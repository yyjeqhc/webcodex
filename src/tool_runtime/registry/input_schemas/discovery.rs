use serde_json::{json, Value};

use super::super::super::tool_spec::ToolSpec;
use super::common::object_schema;
use crate::tool_runtime::sessions::{
    is_tool_call_expectation_metadata_field, TOOL_CALL_RECORDING_SESSION_ID_FIELD,
};
use crate::tool_runtime::tool_definition::runtime_tool_extra_accepted_flattened_args;

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
                "description": "Maximum returned tools for focused discovery; capped at 128."
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
                "description": "Optional category filter (e.g. session, edit, git, checkpoint, runtime, job, validation). Distinct from intent."
            },
            "intent": {
                "type": "string",
                "description": "Optional task-intent view for compact discovery: coding, audit, exploration, release, or discovery. Distinct from category; unknown values return a structured error. Intent views only filter and rank discovery output; they do not change tool behavior, policy, permissions, execution, or finish verdict semantics."
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

pub(crate) fn list_projects_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "client_id": {
                "type": "string",
                "minLength": 1,
                "maxLength": 128,
                "description": "Exact Runner client_id. Filters only caller-visible Projects on that Runner."
            },
            "project": {
                "type": "string",
                "minLength": 1,
                "maxLength": 512,
                "description": "Exact full runtime Project id (agent:<client_id>:<project_id>)."
            },
            "query": {
                "type": "string",
                "minLength": 1,
                "maxLength": 200,
                "description": "Bounded deterministic case-insensitive text filter over already-visible Project metadata."
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": 100,
                "description": "Maximum Projects returned after all filters. Omit to preserve the legacy full visible registry result."
            },
            "summary_only": {
                "type": "boolean",
                "description": "Return a compact workspace-selection projection without paths, revisions, or broad smoke metadata."
            }
        },
        "required": [],
        "additionalProperties": false,
    })
}

pub(crate) fn list_agents_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "client_id": {
                "type": "string",
                "minLength": 1,
                "maxLength": 128,
                "description": "Exact single Runner client_id. Mutually exclusive with client_ids."
            },
            "client_ids": {
                "type": "array",
                "maxItems": 8,
                "minItems": 1,
                "uniqueItems": true,
                "description": "Bounded exact Runner client_ids. Mutually exclusive with client_id; duplicates are rejected.",
                "items": {"type": "string", "minLength": 1, "maxLength": 128}
            },
            "include_projects": {
                "type": "boolean",
                "description": "When false, omit Project bodies while retaining each Runner project count. Defaults to true for compatibility."
            },
            "summary_only": {
                "type": "boolean",
                "description": "Return compact Runner identity, health, build, project-count, and shared Job-concurrency facts."
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
            "client_id": {
                "type": "string",
                "minLength": 1,
                "maxLength": 128,
                "description": "Exact caller-visible Runner client_id. Focused source alignment evaluates only this Runner; omit for fleet-wide status."
            },
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
    "client_id",
    "client_ids",
    "query",
    "path",
    "title",
    "instruction",
    "session_id",
    "execution_context",
    "compact",
    "purpose",
    "shell",
    "include_diff",
    "include_hygiene",
    "include_handoff",
    "include_validation_summary",
    "include_validation",
    "include_workspace",
    "include_checkpoints",
    "category",
    "intent",
    "features",
    "summary_only",
    "include_projects",
    "limit",
    "allow_missing",
    "upload_id",
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

pub(crate) fn generic_tool_call_flattened_args_for_spec(spec: &ToolSpec) -> Vec<String> {
    accepted_flattened_args_for_spec(spec)
        .into_iter()
        .filter(|field| !is_tool_call_expectation_metadata_field(field))
        .collect()
}

fn push_unique_flattened_arg(names: &mut Vec<String>, field: &str) {
    if !names.iter().any(|name| name == field) {
        names.push(field.to_string());
    }
}
