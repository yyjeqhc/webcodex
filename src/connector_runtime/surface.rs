//! Canonical connector capability registry.
//!
//! MCP discovery and hosted OpenAPI are projections of this single list. The
//! names describe user intent; the legacy runtime tool selected underneath is
//! an adapter detail and never appears in this surface.

use crate::tool_runtime::ToolSpec;
use serde_json::{json, Map, Value};

pub(crate) const CAPABILITY_NAMES: &[&str] = &[
    "task_start",
    "files_read",
    "files_search",
    "edits_apply",
    "checks_run",
    "commands_run",
    "task_review",
    "task_finish",
];

pub(crate) fn capability_specs() -> Vec<ToolSpec> {
    vec![
        spec(
            "task_start",
            "Start one bounded coding task and return a compact Project Brief with Git state, language/manifests, instruction paths, and recommended checks. Normal tasks use the reusable writable workspace; read_only never permits mutation.",
            json!({
                "type": "object",
                "properties": {
                    "goal": { "type": "string", "minLength": 1, "maxLength": 4000, "description": "Concrete outcome requested by the user." },
                    "mode": { "type": "string", "enum": ["normal", "read_only"], "default": "normal" }
                },
                "required": ["goal"],
                "additionalProperties": false
            }),
            false,
            false,
        ),
        spec(
            "files_read",
            "Read one small, coherent batch of project files for an active task. Every result includes the complete-file sha256 required by edits_apply, even for a line range.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": task_id_schema(),
                    "files": {
                        "type": "array", "minItems": 1, "maxItems": 8,
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": path_schema(),
                                "start_line": { "type": "integer", "minimum": 1 },
                                "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 200 },
                                "with_line_numbers": { "type": "boolean", "default": true }
                            },
                            "required": ["path"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["task_id", "files"],
                "additionalProperties": false
            }),
            true,
            true,
        ),
        spec(
            "files_search",
            "Search project text in deterministic path order with a query-bound cursor inside a 200-record live window. Sensitive/build directories remain excluded; restart after edits.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": task_id_schema(),
                    "pattern": { "type": "string", "minLength": 1, "maxLength": 500 },
                    "path": path_schema(),
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 50 },
                    "context_before": { "type": "integer", "minimum": 0, "maximum": 5, "default": 0 },
                    "context_after": { "type": "integer", "minimum": 0, "maximum": 5, "default": 0 },
                    "include_globs": string_array_schema(20),
                    "exclude_globs": string_array_schema(20),
                    "result_mode": { "type": "string", "enum": ["matches", "files_with_matches", "count"], "default": "matches" },
                    "cursor": {
                        "type": "string",
                        "description": "Opaque query-bound cursor returned by the previous page. Restart the search after editing the workspace."
                    }
                },
                "required": ["task_id", "pattern"],
                "additionalProperties": false
            }),
            true,
            true,
        ),
        spec(
            "edits_apply",
            "Transactionally apply up to 16 edit/create/delete/rename file changes. Existing files require sha256 values returned by files_read; the full batch is preflighted before mutation. Reuse operation_id only for an exact retry.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": task_id_schema(),
                    "operation_id": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 100,
                        "pattern": "^[A-Za-z0-9][A-Za-z0-9._:-]{0,99}$",
                        "description": "Caller-generated idempotency key. Reuse only with byte-identical changes and preconditions."
                    },
                    "changes": {
                        "type": "array", "minItems": 1, "maxItems": 16,
                        "items": {
                            "type": "object",
                            "properties": {
                                "kind": { "type": "string", "enum": ["edit", "create", "delete", "rename"] },
                                "path": path_schema(),
                                "to_path": path_schema(),
                                "content": { "type": "string" },
                                "expected_sha256": { "type": "string", "pattern": "^[a-f0-9]{64}$" },
                                "edits": {
                                    "type": "array", "minItems": 1, "maxItems": 20,
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "kind": { "type": "string", "enum": ["replace_exact", "insert_after", "insert_before", "delete_exact"] },
                                            "old_text": { "type": "string" },
                                            "new_text": { "type": "string" },
                                            "anchor_text": { "type": "string" }
                                        },
                                        "required": ["kind"],
                                        "additionalProperties": false
                                    }
                                }
                            },
                            "required": ["kind", "path"],
                            "additionalProperties": false
                        }
                    },
                    "dry_run": { "type": "boolean", "default": false }
                },
                "required": ["task_id", "operation_id", "changes"],
                "additionalProperties": false
            }),
            false,
            true,
        ),
        spec(
            "checks_run",
            "Run a bounded set of standard Rust checks for an active task. This is the normal validation path; arbitrary commands belong in commands_run.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": task_id_schema(),
                    "checks": {
                        "type": "array", "minItems": 1, "maxItems": 3,
                        "items": { "type": "string", "enum": ["format", "check", "test"] }
                    },
                    "cwd": path_schema(),
                    "test_filter": { "type": "string", "maxLength": 500 },
                    "timeout_secs": { "type": "integer", "minimum": 1, "maximum": 120, "default": 120 }
                },
                "required": ["task_id", "checks"],
                "additionalProperties": false
            }),
            false,
            false,
        ),
        spec(
            "commands_run",
            "Explicit escape hatch for one bounded project command. The exact action must receive one-time approval through the host-local WebCodex CLI before it is dispatched; approval cannot be granted or replayed by this connector.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": task_id_schema(),
                    "command": { "type": "string", "minLength": 1, "maxLength": 32768 },
                    "cwd": path_schema(),
                    "timeout_secs": { "type": "integer", "minimum": 1, "maximum": 120, "default": 120 }
                },
                "required": ["task_id", "command"],
                "additionalProperties": false
            }),
            false,
            false,
        ),
        spec(
            "task_review",
            "Return the current bounded change summary or stable result preview plus its validation/action timeline. Use this instead of separately aggregating git, job, agent, and session status.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": task_id_schema(),
                    "include_diff": { "type": "boolean", "default": true }
                },
                "required": ["task_id"],
                "additionalProperties": false
            }),
            true,
            true,
        ),
        spec(
            "task_finish",
            "Finish the active run, capture a content-addressed result patch, and mark the task ready for host-local human review. It never writes the target checkout, commits, pushes, deploys, or accepts the result.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": task_id_schema(),
                    "summary": { "type": "string", "minLength": 1, "maxLength": 4000 }
                },
                "required": ["task_id", "summary"],
                "additionalProperties": false
            }),
            false,
            false,
        ),
    ]
}

pub(crate) fn capability_spec(name: &str) -> Option<ToolSpec> {
    capability_specs()
        .into_iter()
        .find(|spec| spec.name == name)
}

pub(crate) fn route_for(name: &str) -> Option<&'static str> {
    match name {
        "task_start" => Some("/api/connector/task/start"),
        "files_read" => Some("/api/connector/files/read"),
        "files_search" => Some("/api/connector/files/search"),
        "edits_apply" => Some("/api/connector/edits/apply"),
        "checks_run" => Some("/api/connector/checks/run"),
        "commands_run" => Some("/api/connector/commands/run"),
        "task_review" => Some("/api/connector/task/review"),
        "task_finish" => Some("/api/connector/task/finish"),
        _ => None,
    }
}

pub(crate) fn build_openapi_spec(public_url: String) -> Value {
    let mut paths = Map::new();
    for spec in capability_specs() {
        let route = route_for(&spec.name).expect("registered connector capability has a route");
        let consequential = spec
            .annotations
            .get("readOnlyHint")
            .and_then(Value::as_bool)
            != Some(true);
        paths.insert(
            route.to_string(),
            json!({
                "post": {
                    "operationId": spec.name,
                    "summary": spec.description,
                    "x-openai-isConsequential": consequential,
                    "security": [{ "bearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": { "schema": spec.input_schema }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Capability completed",
                            "content": { "application/json": { "schema": spec.output_schema } }
                        },
                        "400": { "description": "Invalid input or task operation failed" },
                        "403": { "description": "Authentication scope or task mode denied the capability" },
                        "404": { "description": "Task is not visible in this project and identity context" }
                    }
                }
            }),
        );
    }

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "WebCodex Project Connector",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "A project-bound coding capability surface for hosted chat clients. Start a task, inspect, edit, validate, review, and finish. Project and executor routing are connector context and are never model input."
        },
        "servers": [{ "url": public_url, "description": "WebCodex connector" }],
        "paths": Value::Object(paths),
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer"
                }
            }
        }
    })
}

fn spec(
    name: &str,
    description: &str,
    input_schema: Value,
    read_only: bool,
    idempotent: bool,
) -> ToolSpec {
    ToolSpec {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
        output_schema: connector_output_schema(),
        annotations: json!({
            "title": name,
            "readOnlyHint": read_only,
            "destructiveHint": false,
            "idempotentHint": idempotent,
            "openWorldHint": false
        }),
    }
}

fn connector_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "ok": { "type": "boolean" },
            "task_id": { "type": ["string", "null"] },
            "run_id": { "type": ["string", "null"] },
            "event_cursor": { "type": ["integer", "null"] },
            "data": {},
            "warnings": { "type": "array", "items": { "type": "string" } },
            "blocking": { "type": "boolean" },
            "error": {
                "type": "object",
                "properties": {
                    "code": { "type": "string" },
                    "message": { "type": "string" },
                    "retryable": { "type": "boolean" },
                    "user_action_required": { "type": "boolean" },
                    "suggested_action": { "type": ["string", "null"] }
                },
                "required": ["code", "message", "retryable", "user_action_required", "suggested_action"],
                "additionalProperties": false
            }
        },
        "required": ["ok", "task_id", "run_id", "event_cursor", "data", "warnings", "blocking"],
        "additionalProperties": false
    })
}

fn task_id_schema() -> Value {
    json!({
        "type": "string",
        "pattern": "^wc_task_[a-f0-9]{32}$",
        "description": "Opaque id returned by task_start."
    })
}

fn path_schema() -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "maxLength": 1024,
        "description": "Project-relative path; omit only where the field is optional."
    })
}

fn string_array_schema(max_items: usize) -> Value {
    json!({
        "type": "array",
        "maxItems": max_items,
        "items": { "type": "string", "minLength": 1, "maxLength": 200 }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn registry_is_exact_small_surface_with_strict_inputs() {
        let specs = capability_specs();
        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.name.as_str())
                .collect::<Vec<_>>(),
            CAPABILITY_NAMES
        );
        for spec in specs {
            assert_eq!(
                spec.input_schema["additionalProperties"], false,
                "{}",
                spec.name
            );
            assert_eq!(
                spec.output_schema["additionalProperties"], false,
                "{}",
                spec.name
            );
            assert!(route_for(&spec.name).is_some());
        }
    }

    #[test]
    fn hosted_openapi_is_generated_from_same_eight_capabilities() {
        let spec = build_openapi_spec("https://connector.example".to_string());
        let operations = spec["paths"]
            .as_object()
            .unwrap()
            .values()
            .map(|path| path["post"]["operationId"].as_str().unwrap().to_string())
            .collect::<BTreeSet<_>>();
        let expected = CAPABILITY_NAMES
            .iter()
            .map(|name| name.to_string())
            .collect::<BTreeSet<_>>();
        assert_eq!(operations, expected);
        assert_eq!(spec["paths"].as_object().unwrap().len(), 8);
    }
}
