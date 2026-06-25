use salvo::prelude::*;
use serde_json::{json, Value};

fn public_url() -> String {
    std::env::var("DROP_PUBLIC_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://localhost:8080".to_string())
}

#[cfg(test)]
const GPT_ACTION_OPS: &[&str] = &[
    "listRuntimeTools",
    "callRuntimeTool",
    "runCodexTask",
    "getRuntimeJobStatus",
    "getRuntimeJobLog",
];

#[handler]
pub async fn openapi_json(res: &mut Response) {
    res.render(Json(build_openapi_spec()));
}

fn build_openapi_spec() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Private Drop Runtime API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Minimal GPT Actions API for invoking Private Drop runtime tools, Codex CLI jobs, and job inspection."
        },
        "servers": [
            {
                "url": public_url(),
                "description": "Private Drop server"
            }
        ],
        "paths": {
            "/api/tools/list": {
                "post": operation(
                    "listRuntimeTools",
                    "List runtime tools",
                    "Returns the MCP-compatible tool list exposed by the server.",
                    "EmptyRequest",
                    "ToolsListResponse"
                )
            },
            "/api/tools/call": {
                "post": operation(
                    "callRuntimeTool",
                    "Call runtime tool",
                    "Calls one tool by name. Use listRuntimeTools first when the available tools are unknown.",
                    "ToolCallRequest",
                    "ToolResult"
                )
            },
            "/api/codex/run": {
                "post": operation(
                    "runCodexTask",
                    "Run Codex CLI task",
                    "Starts Codex CLI asynchronously inside a configured project and returns a job_id. Poll with getRuntimeJobStatus and getRuntimeJobLog.",
                    "CodexRunRequest",
                    "ToolResult"
                )
            },
            "/api/jobs/status": {
                "post": operation(
                    "getRuntimeJobStatus",
                    "Get job status",
                    "Returns status, timing, and exit metadata for a runtime job.",
                    "JobStatusRequest",
                    "ToolResult"
                )
            },
            "/api/jobs/log": {
                "post": operation(
                    "getRuntimeJobLog",
                    "Get job log",
                    "Returns stdout/stderr text for a runtime job.",
                    "JobLogRequest",
                    "ToolResult"
                )
            }
        },
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer"
                }
            },
            "schemas": schemas()
        },
        "security": [
            {
                "bearerAuth": []
            }
        ]
    })
}

fn operation(
    operation_id: &str,
    summary: &str,
    description: &str,
    request_schema: &str,
    response_schema: &str,
) -> Value {
    json!({
        "operationId": operation_id,
        "summary": summary,
        "description": description,
        "requestBody": {
            "required": true,
            "content": {
                "application/json": {
                    "schema": {
                        "$ref": format!("#/components/schemas/{}", request_schema)
                    }
                }
            }
        },
        "responses": {
            "200": {
                "description": "Success",
                "content": {
                    "application/json": {
                        "schema": {
                            "$ref": format!("#/components/schemas/{}", response_schema)
                        }
                    }
                }
            },
            "400": {
                "description": "Bad request",
                "content": {
                    "application/json": {
                        "schema": {
                            "$ref": "#/components/schemas/ErrorResponse"
                        }
                    }
                }
            },
            "401": {
                "description": "Unauthorized"
            }
        }
    })
}

fn schemas() -> Value {
    json!({
        "EmptyRequest": {
            "type": "object",
            "additionalProperties": false,
            "properties": {}
        },
        "ToolCallRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["tool"],
            "properties": {
                "tool": {
                    "type": "string",
                    "description": "Runtime tool name, for example run_shell, run_job, run_codex, read_file, git_status, git_diff, apply_patch, job_status, or job_log."
                },
                "params": {
                    "type": "object",
                    "description": "Tool-specific arguments object.",
                    "additionalProperties": true
                }
            }
        },
        "CodexRunRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project", "prompt"],
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Configured project id from projects.toml."
                },
                "prompt": {
                    "type": "string",
                    "description": "Instruction prompt passed to Codex CLI."
                },
                "approval_mode": {
                    "type": "string",
                    "description": "Codex approval mode. Defaults to CODEX_APPROVAL_MODE or full-auto."
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Maximum runtime in seconds."
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional project-relative working directory."
                },
                "extra_args": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Optional additional Codex CLI arguments."
                }
            }
        },
        "JobStatusRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["job_id"],
            "properties": {
                "job_id": {
                    "type": "string",
                    "description": "Runtime job id."
                }
            }
        },
        "JobLogRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["job_id"],
            "properties": {
                "job_id": {
                    "type": "string",
                    "description": "Runtime job id."
                },
                "offset": {
                    "type": "integer",
                    "description": "Optional 1-based stdout line cursor returned as next_stdout_line."
                },
                "tail_lines": {
                    "type": "integer",
                    "description": "Optional number of trailing stdout lines to return. Logs are always bounded."
                }
            }
        },
        "ToolSpec": {
            "type": "object",
            "required": ["name", "description", "inputSchema"],
            "properties": {
                "name": { "type": "string" },
                "description": { "type": "string" },
                "inputSchema": { "type": "object", "additionalProperties": true }
            }
        },
        "ToolsListResponse": {
            "type": "object",
            "required": ["success", "tools"],
            "properties": {
                "success": { "type": "boolean" },
                "tools": {
                    "type": "array",
                    "items": { "$ref": "#/components/schemas/ToolSpec" }
                }
            }
        },
        "ToolResult": {
            "type": "object",
            "required": ["success", "output"],
            "properties": {
                "success": { "type": "boolean" },
                "output": {
                    "description": "Tool-specific JSON output.",
                    "type": ["object", "array", "string", "number", "boolean", "null"]
                },
                "error": {
                    "type": "string",
                    "description": "Human-readable error when success is false."
                }
            }
        },
        "ErrorResponse": {
            "type": "object",
            "properties": {
                "status": { "type": "integer" },
                "error": { "type": "string" }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recursively collect every `$ref` string found anywhere in a JSON value.
    fn collect_refs(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (k, v) in map {
                    if k == "$ref" {
                        if let Some(s) = v.as_str() {
                            out.push(s.to_string());
                        }
                    }
                    collect_refs(v, out);
                }
            }
            Value::Array(arr) => {
                for v in arr {
                    collect_refs(v, out);
                }
            }
            _ => {}
        }
    }

    /// Resolve a local `#/components/schemas/<Name>` ref against the spec.
    fn resolve_local_ref<'a>(spec: &'a Value, reference: &str) -> Option<&'a Value> {
        let rest = reference.strip_prefix("#/")?;
        let mut current = spec;
        for segment in rest.split('/') {
            current = current.get(segment)?;
        }
        Some(current)
    }

    #[test]
    fn openapi_operation_ids_are_minimal() {
        let spec = build_openapi_spec();
        let mut ids = Vec::new();
        for methods in spec["paths"].as_object().unwrap().values() {
            for op in methods.as_object().unwrap().values() {
                ids.push(op["operationId"].as_str().unwrap().to_string());
            }
        }
        ids.sort();
        let mut expected = GPT_ACTION_OPS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        expected.sort();
        assert_eq!(ids, expected);
    }

    #[test]
    fn openapi_uses_bearer_auth() {
        let spec = build_openapi_spec();
        assert_eq!(
            spec["components"]["securitySchemes"]["bearerAuth"]["scheme"],
            "bearer"
        );
    }

    #[test]
    fn openapi_does_not_expose_legacy_drop_routes() {
        let spec = build_openapi_spec();
        let paths = spec["paths"].as_object().unwrap();
        assert!(!paths.contains_key("/api/messages"));
        assert!(!paths.contains_key("/api/files"));
        assert!(!paths.contains_key("/api/desktop/task_op"));
        assert!(!paths.contains_key("/api/codex/command_request_op"));
    }

    #[test]
    fn openapi_does_not_expose_legacy_openapi_variants() {
        let spec = build_openapi_spec();
        let paths = spec["paths"].as_object().unwrap();
        // Legacy endpoints that must not reappear in the GPT Actions schema.
        for legacy in [
            "/api/codex/job",
            "/api/codex/git",
            "/api/codex/edit",
            "/api/codex/apply_patch",
            "/api/codex/context",
            "/api/codex/context_batch",
            "/api/codex/artifact",
            "/api/codex/report",
            "/api/codex/projects",
            "/api/shell/run",
            "/api/shell/job",
            "/api/shell/file",
        ] {
            assert!(
                !paths.contains_key(legacy),
                "legacy path '{}' must not appear in openapi.json",
                legacy
            );
        }
    }

    #[test]
    fn openapi_all_local_refs_resolve() {
        let spec = build_openapi_spec();
        let mut refs = Vec::new();
        collect_refs(&spec, &mut refs);
        assert!(!refs.is_empty(), "expected at least one $ref in the spec");
        for reference in &refs {
            assert!(
                reference.starts_with("#/"),
                "only local refs are allowed, found: {}",
                reference
            );
            let resolved = resolve_local_ref(&spec, reference)
                .unwrap_or_else(|| panic!("unresolved $ref target: {}", reference));
            assert!(
                resolved.is_object(),
                "$ref target '{}' should resolve to a schema object",
                reference
            );
        }
    }

    #[test]
    fn openapi_operation_ids_match_expected_set_exactly() {
        let spec = build_openapi_spec();
        let mut ids: Vec<String> = Vec::new();
        for methods in spec["paths"].as_object().unwrap().values() {
            for op in methods.as_object().unwrap().values() {
                ids.push(op["operationId"].as_str().unwrap().to_string());
            }
        }
        let expected: Vec<String> = GPT_ACTION_OPS.iter().map(|s| s.to_string()).collect();
        assert_eq!(ids.len(), expected.len());
        for id in &expected {
            assert!(ids.contains(id), "missing operation id: {}", id);
        }
    }

    #[test]
    fn openapi_paths_only_use_post_method() {
        // GPT Actions surface is POST-only.
        let spec = build_openapi_spec();
        for (path, methods) in spec["paths"].as_object().unwrap() {
            let method_keys: Vec<&String> = methods.as_object().unwrap().keys().collect();
            assert_eq!(
                method_keys,
                vec!["post"],
                "path '{}' should only expose POST, got {:?}",
                path,
                method_keys
            );
        }
    }

    #[test]
    fn openapi_top_level_security_uses_bearer() {
        let spec = build_openapi_spec();
        let security = spec["security"].as_array().expect("security array");
        assert!(!security.is_empty());
        assert!(security[0]["bearerAuth"].is_array());
    }

    #[test]
    fn openapi_schemas_define_all_referenced_names() {
        let spec = build_openapi_spec();
        let schemas = spec["components"]["schemas"]
            .as_object()
            .expect("schemas object");
        // Every referenced schema name must exist as a key.
        let mut refs = Vec::new();
        collect_refs(&spec, &mut refs);
        for reference in &refs {
            if let Some(name) = reference.strip_prefix("#/components/schemas/") {
                assert!(
                    schemas.contains_key(name),
                    "referenced schema '{}' is not defined in components/schemas",
                    name
                );
            }
        }
    }
}
