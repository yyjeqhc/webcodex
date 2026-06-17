use crate::action_sessions::ACTION_SESSION_GUIDANCE;
use crate::json_error;
use salvo::prelude::*;

fn public_url() -> String {
    std::env::var("DROP_PUBLIC_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://localhost:8080".to_string())
}

const PROJECT_SCHEMA_DESCRIPTION: &str = "Runtime-validated project name. Add or remove projects in projects.toml and restart the service; the OpenAPI schema intentionally does not enumerate project names.";

#[handler]
pub async fn openapi_json(res: &mut Response) {
    match serde_json::from_str::<serde_json::Value>(include_str!("../data/openapi.json")) {
        Ok(mut spec) => {
            spec["openapi"] = serde_json::Value::String("3.1.0".to_string());
            spec["servers"] = serde_json::json!([{
                "url": public_url(),
                "description": "Public server"
            }]);
            res.render(Json(spec));
        }
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Invalid OpenAPI schema: {}", e),
            ));
        }
    }
}

fn apply_project_description_to_schema(spec: &mut serde_json::Value, schema_names: &[&str]) {
    for name in schema_names {
        if let Some(project) = spec["components"]["schemas"][*name]["properties"].get_mut("project")
        {
            if let Some(obj) = project.as_object_mut() {
                obj.remove("enum");
                obj.insert(
                    "description".to_string(),
                    serde_json::json!(PROJECT_SCHEMA_DESCRIPTION),
                );
            }
        }
    }
}

fn apply_edit_timeout_guidance(spec: &mut serde_json::Value) {
    spec["paths"]["/api/codex/edit"]["post"]["description"] = serde_json::json!(
        "Apply edits. Use post_check to run a configured check and auto-rollback touched files on failure."
    );
    spec["components"]["schemas"]["EditRequest"]["properties"]["response_mode"]["description"] = serde_json::json!(
        "Response detail. For larger or multi-file edits, use summary to reduce timeout risk."
    );
}

fn apply_job_recovery_guidance(spec: &mut serde_json::Value) {
    // The description is already set in data/openapi.json; this function
    // adds the detail field description programmatically in case the static
    // schema is missing it.
    if spec["components"]["schemas"]["JobOpRequest"]["properties"]["detail"].is_null() {
        spec["components"]["schemas"]["JobOpRequest"]["properties"]["detail"] = serde_json::json!({
            "type": "string",
            "enum": ["basic", "logs"],
            "description": "For op=status: basic (default, lightweight, no logs) or logs (include stdout/stderr tails). tail_lines only affects detail=logs or op=log."
        });
    }
}

fn apply_context_batch_guidance(spec: &mut serde_json::Value) {
    // Ensure context_batch endpoint description includes batch-size guidance.
    // The static openapi.json already has the full description; this is a fallback
    // in case the static schema is missing it.
    let endpoint = &mut spec["paths"]["/api/codex/context_batch"]["post"]["description"];
    if endpoint
        .as_str()
        .map_or(true, |s| !s.contains("preflight_rejected"))
    {
        *endpoint = serde_json::json!(
            "Batch project context reads. For SSH keep batches small; if preflight_rejected=true, split the request."
        );
    }
}

fn append_action_session_guidance(description: &str) -> String {
    if description.contains("X-Action-Session-Id") {
        description.to_string()
    } else {
        format!("{} {}", description.trim(), ACTION_SESSION_GUIDANCE)
    }
}

fn apply_action_session_openapi(spec: &mut serde_json::Value) {
    let paths = [
        "/api/codex/projects",
        "/api/codex/context_batch",
        "/api/codex/edit",
        "/api/codex/artifact",
        "/api/codex/git",
        "/api/codex/command",
        "/api/codex/command_request_op",
        "/api/codex/job",
        "/api/codex/check",
        "/api/codex/report",
        "/api/desktop/task_op",
    ];
    for path in paths {
        if let Some(description) = spec["paths"][path]["post"]["description"].as_str() {
            spec["paths"][path]["post"]["description"] =
                serde_json::json!(append_action_session_guidance(description));
        }
    }
    spec["paths"]["/api/codex/action_sessions"] = serde_json::json!({
        "post": {
            "operationId": "runActionSessionOp",
            "summary": "Query or manage action sessions",
            "description": "List, inspect, rename, close, or summarize recorded action sessions.",
            "tags": ["codex"],
            "requestBody": {
                "required": true,
                "content": {
                    "application/json": {
                        "schema": { "$ref": "#/components/schemas/ActionSessionOpRequest" }
                    }
                }
            },
            "responses": {
                "200": {
                    "description": "Action session operation result",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ActionSessionOpResponse" }
                        }
                    }
                }
            }
        }
    });
    spec["components"]["schemas"]["ActionSessionRecord"] = serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": {"type": "string"},
            "title": {"type": "string", "nullable": true},
            "note": {"type": "string", "nullable": true},
            "status": {"type": "string"},
            "created_at": {"type": "integer"},
            "updated_at": {"type": "integer"},
            "closed_at": {"type": "integer", "nullable": true},
            "first_event_at": {"type": "integer", "nullable": true},
            "last_event_at": {"type": "integer", "nullable": true},
            "total_actions": {"type": "integer"},
            "success_count": {"type": "integer"},
            "failed_count": {"type": "integer"},
            "timeout_or_unknown_count": {"type": "integer"},
            "warning_count": {"type": "integer"},
            "total_duration_ms": {"type": "integer"},
            "changed_files_count": {"type": "integer"},
            "job_ids_count": {"type": "integer"}
        },
        "required": ["session_id", "status", "created_at", "updated_at", "total_actions", "success_count", "failed_count", "timeout_or_unknown_count", "warning_count", "total_duration_ms", "changed_files_count", "job_ids_count"]
    });
    spec["components"]["schemas"]["ActionSessionStats"] = serde_json::json!({
        "type": "object",
        "properties": {
            "by_endpoint": {"type": "object", "additionalProperties": {"type": "integer"}},
            "by_project": {"type": "object", "additionalProperties": {"type": "integer"}},
            "by_status": {"type": "object", "additionalProperties": {"type": "integer"}},
            "edit_count": {"type": "integer"},
            "context_count": {"type": "integer"},
            "job_count": {"type": "integer"},
            "command_count": {"type": "integer"},
            "report_count": {"type": "integer"},
            "artifact_count": {"type": "integer"},
            "git_count": {"type": "integer"},
            "shell_count": {"type": "integer"},
            "desktop_count": {"type": "integer"},
            "changed_files_distinct_count": {"type": "integer"},
            "job_ids_distinct_count": {"type": "integer"}
        },
        "required": ["by_endpoint", "by_project", "by_status", "edit_count", "context_count", "job_count", "command_count", "report_count", "artifact_count", "git_count", "shell_count", "desktop_count", "changed_files_distinct_count", "job_ids_distinct_count"]
    });
    spec["components"]["schemas"]["ActionEventView"] = serde_json::json!({
        "type": "object",
        "properties": {
            "event_id": {"type": "string"},
            "session_id": {"type": "string"},
            "started_at": {"type": "integer"},
            "ended_at": {"type": "integer"},
            "duration_ms": {"type": "integer"},
            "endpoint": {"type": "string"},
            "operation": {"type": "string", "nullable": true},
            "action_name": {"type": "string"},
            "project": {"type": "string", "nullable": true},
            "status": {"type": "string"},
            "http_status": {"type": "integer", "nullable": true},
            "error_summary": {"type": "string", "nullable": true},
            "warning_summary": {"type": "string", "nullable": true},
            "changed_files": {"type": "array", "items": {"type": "string"}},
            "ids": {"type": "object"},
            "summary": {"type": "object"},
            "request_bytes": {"type": "integer", "nullable": true},
            "response_bytes": {"type": "integer", "nullable": true}
        },
        "required": ["event_id", "session_id", "started_at", "ended_at", "duration_ms", "endpoint", "action_name", "status", "changed_files", "ids", "summary"]
    });
    spec["components"]["schemas"]["ActionSessionListItem"] = serde_json::json!({
        "type": "object",
        "properties": {
            "session": {"$ref": "#/components/schemas/ActionSessionRecord"},
            "stats": {"$ref": "#/components/schemas/ActionSessionStats"},
            "top_endpoints": {"type": "array", "items": {"type": "string"}},
            "top_projects": {"type": "array", "items": {"type": "string"}}
        },
        "required": ["session", "stats", "top_endpoints", "top_projects"]
    });
    spec["components"]["schemas"]["ActionSessionOpRequest"] = serde_json::json!({
        "type": "object",
        "properties": {
            "op": {"type": "string", "enum": ["list", "get", "events", "stats", "rename", "close"]},
            "session_id": {"type": "string", "nullable": true},
            "status": {"type": "string", "nullable": true},
            "title": {"type": "string", "nullable": true},
            "note": {"type": "string", "nullable": true},
            "limit": {"type": "integer", "nullable": true}
        },
        "required": ["op"]
    });
    spec["components"]["schemas"]["ActionSessionOpResponse"] = serde_json::json!({
        "type": "object",
        "properties": {
            "success": {"type": "boolean"},
            "op": {"type": "string"},
            "sessions": {"type": "array", "items": {"$ref": "#/components/schemas/ActionSessionListItem"}},
            "session": {"$ref": "#/components/schemas/ActionSessionRecord", "nullable": true},
            "stats": {"$ref": "#/components/schemas/ActionSessionStats", "nullable": true},
            "events": {"type": "array", "items": {"$ref": "#/components/schemas/ActionEventView"}},
            "error": {"type": "string", "nullable": true}
        },
        "required": ["success", "op", "sessions", "events"]
    });
}

fn apply_shell_client_openapi(spec: &mut serde_json::Value) {
    spec["paths"]["/api/shell/clients"] = serde_json::json!({
        "post": {
            "operationId": "listShellClients",
            "summary": "List shell clients",
            "description": "List private-drop-agent shell clients and their online status.",
            "tags": ["shell"],
            "requestBody": {
                "required": true,
                "content": { "application/json": { "schema": { "type": "object", "properties": {} } } }
            },
            "responses": {
                "200": {
                    "description": "Shell clients response",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ShellClientsResponse" }
                        }
                    }
                }
            }
        }
    });
    spec["paths"]["/api/shell/projects"] = serde_json::json!({
        "post": {
            "operationId": "listShellClientProjects",
            "summary": "List shell client projects",
            "description": "List project summaries most recently reported by one private-drop-agent client.",
            "tags": ["shell"],
            "requestBody": {
                "required": true,
                "content": {
                    "application/json": {
                        "schema": { "$ref": "#/components/schemas/ShellClientProjectsRequest" }
                    }
                }
            },
            "responses": {
                "200": {
                    "description": "Shell client projects response",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ShellClientProjectsResponse" }
                        }
                    }
                },
                "403": { "description": "Client owner mismatch" }
            }
        }
    });
    spec["paths"]["/api/shell/projects/create"] = serde_json::json!({
        "post": {
            "operationId": "createShellClientProject",
            "summary": "Create an agent-owned project",
            "description": "Ask a private-drop-agent client to create a local project and write its projects.d registry file.",
            "tags": ["shell"],
            "requestBody": {
                "required": true,
                "content": {
                    "application/json": {
                        "schema": { "$ref": "#/components/schemas/ShellClientProjectCreateRequest" }
                    }
                }
            },
            "responses": {
                "200": {
                    "description": "Project create response",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ShellClientProjectCreateResponse" }
                        }
                    }
                },
                "403": { "description": "Client owner mismatch" }
            }
        }
    });
    spec["paths"]["/api/shell/run"] = serde_json::json!({
        "post": {
            "operationId": "runShell",
            "summary": "Run shell on a client",
            "description": "Run a shell command through a registered private-drop-agent client. Use short waits; use future job APIs for long tasks.",
            "tags": ["shell"],
            "requestBody": {
                "required": true,
                "content": {
                    "application/json": {
                        "schema": { "$ref": "#/components/schemas/ShellRunRequest" }
                    }
                }
            },
            "responses": {
                "200": {
                    "description": "Shell run response",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ShellRunResponse" }
                        }
                    }
                }
            }
        }
    });
    spec["paths"]["/api/shell/job"] = serde_json::json!({
        "post": {
            "operationId": "runShellJob",
            "summary": "Manage shell jobs",
            "description": "Start, inspect, read logs, stop, or list async shell jobs running through private-drop-agent.",
            "tags": ["shell"],
            "requestBody": {
                "required": true,
                "content": {
                    "application/json": {
                        "schema": { "$ref": "#/components/schemas/ShellJobOpRequest" }
                    }
                }
            },
            "responses": {
                "200": {
                    "description": "Shell job operation response",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ShellJobOpResponse" }
                        }
                    }
                }
            }
        }
    });
    spec["paths"]["/api/shell/file"] = serde_json::json!({
        "post": {
            "operationId": "shellFileOp",
            "summary": "Read, write, or list files",
            "description": "Read, write, or list files through a registered private-drop-agent client.",
            "tags": ["shell"],
            "requestBody": {
                "required": true,
                "content": {
                    "application/json": {
                        "schema": { "$ref": "#/components/schemas/ShellFileOpRequest" }
                    }
                }
            },
            "responses": {
                "200": {
                    "description": "Shell file operation response",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ShellFileOpResponse" }
                        }
                    }
                }
            }
        }
    });
    spec["components"]["schemas"]["ShellAgentProjectSummary"] = serde_json::json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" },
            "name": { "type": "string", "nullable": true },
            "path": { "type": "string" },
            "kind": { "type": "string", "nullable": true },
            "description": { "type": "string", "nullable": true },
            "hooks": { "type": "array", "items": { "type": "string" } },
            "disabled": { "type": "boolean" },
            "git_branch": { "type": "string", "nullable": true },
            "git_head": { "type": "string", "nullable": true },
            "git_dirty": { "type": "boolean", "nullable": true },
            "updated_at": { "type": "integer" }
        },
        "required": ["id", "path", "hooks", "disabled", "updated_at"]
    });
    spec["components"]["schemas"]["ShellClientCapabilities"] = serde_json::json!({
        "type": "object",
        "properties": {
            "shell": { "type": "boolean" },
            "file_read": { "type": "boolean" },
            "file_write": { "type": "boolean" },
            "git": { "type": "boolean" },
            "jobs": { "type": "boolean" },
            "project_create": { "type": "boolean" }
        }
    });
    spec["components"]["schemas"]["ShellClientView"] = serde_json::json!({
        "type": "object",
        "properties": {
            "client_id": { "type": "string" },
            "display_name": { "type": "string", "nullable": true },
            "owner": { "type": "string", "nullable": true },
            "hostname": { "type": "string", "nullable": true },
            "status": { "type": "string" },
            "connected": { "type": "boolean" },
            "last_seen": { "type": "integer" },
            "capabilities": { "$ref": "#/components/schemas/ShellClientCapabilities" },
            "pending_requests": { "type": "integer" },
            "projects": { "type": "array", "items": { "$ref": "#/components/schemas/ShellAgentProjectSummary" } }
        },
        "required": ["client_id", "status", "connected", "last_seen", "capabilities", "pending_requests", "projects"]
    });
    spec["components"]["schemas"]["ShellClientsResponse"] = serde_json::json!({
        "type": "object",
        "properties": {
            "success": { "type": "boolean" },
            "clients": { "type": "array", "items": { "$ref": "#/components/schemas/ShellClientView" } },
            "error": { "type": "string", "nullable": true }
        },
        "required": ["success", "clients"]
    });
    spec["components"]["schemas"]["ShellClientProjectsRequest"] = serde_json::json!({
        "type": "object",
        "properties": {
            "client_id": { "type": "string" }
        },
        "required": ["client_id"]
    });
    spec["components"]["schemas"]["ShellClientProjectsResponse"] = serde_json::json!({
        "type": "object",
        "properties": {
            "success": { "type": "boolean" },
            "client_id": { "type": "string" },
            "projects": { "type": "array", "items": { "$ref": "#/components/schemas/ShellAgentProjectSummary" } },
            "error": { "type": "string", "nullable": true }
        },
        "required": ["success", "client_id", "projects"]
    });
    spec["components"]["schemas"]["ShellClientProjectCreateRequest"] = serde_json::json!({
        "type": "object",
        "properties": {
            "client_id": { "type": "string" },
            "project_id": { "type": "string" },
            "path": { "type": "string" },
            "name": { "type": "string", "nullable": true },
            "kind": { "type": "string", "nullable": true },
            "description": { "type": "string", "nullable": true },
            "template": { "type": "string", "enum": ["empty", "rust", "python", "docs"], "default": "empty" },
            "git_init": { "type": "boolean", "default": true },
            "allow_existing": { "type": "boolean", "default": false },
            "hooks": {
                "type": "object",
                "nullable": true,
                "additionalProperties": { "type": "array", "items": { "type": "string" } }
            },
            "timeout_secs": { "type": "integer", "default": 120 },
            "wait_timeout_secs": { "type": "integer", "default": 30 }
        },
        "required": ["client_id", "project_id", "path"]
    });
    spec["components"]["schemas"]["ShellClientProjectCreateResponse"] = serde_json::json!({
        "type": "object",
        "properties": {
            "success": { "type": "boolean" },
            "client_id": { "type": "string" },
            "project": { "$ref": "#/components/schemas/ShellAgentProjectSummary", "nullable": true },
            "created_paths": { "type": "array", "items": { "type": "string" } },
            "registry_file": { "type": "string", "nullable": true },
            "git_initialized": { "type": "boolean" },
            "warnings": { "type": "array", "items": { "type": "string" } },
            "error": { "type": "string", "nullable": true }
        },
        "required": ["success", "client_id", "created_paths", "git_initialized", "warnings"]
    });
    spec["components"]["schemas"]["ShellRunRequest"] = serde_json::json!({
        "type": "object",
        "properties": {
            "client_id": { "type": "string" },
            "cwd": { "type": "string", "nullable": true },
            "command": { "type": "string", "maxLength": 8000 },
            "timeout_secs": { "type": "integer", "default": 120 },
            "wait_timeout_secs": { "type": "integer", "default": 30 }
        },
        "required": ["client_id", "command"]
    });
    spec["components"]["schemas"]["ShellRunResponse"] = serde_json::json!({
        "type": "object",
        "properties": {
            "success": { "type": "boolean" },
            "request_id": { "type": "string" },
            "client_id": { "type": "string" },
            "cwd": { "type": "string", "nullable": true },
            "command_preview": { "type": "string" },
            "exit_code": { "type": "integer", "nullable": true },
            "stdout": { "type": "string", "nullable": true },
            "stderr": { "type": "string", "nullable": true },
            "duration_ms": { "type": "integer", "nullable": true },
            "error": { "type": "string", "nullable": true }
        },
        "required": ["success", "request_id", "client_id", "command_preview"]
    });
    spec["components"]["schemas"]["ShellFileOpRequest"] = serde_json::json!({
        "type": "object",
        "properties": {
            "op": { "type": "string", "enum": ["read", "write", "list"] },
            "client_id": { "type": "string" },
            "path": { "type": "string" },
            "cwd": { "type": "string", "nullable": true },
            "content": { "type": "string", "nullable": true },
            "max_bytes": { "type": "integer", "nullable": true },
            "expected_sha256": { "type": "string", "nullable": true },
            "create_dirs": { "type": "boolean", "default": false },
            "wait_timeout_secs": { "type": "integer", "default": 30 }
        },
        "required": ["op", "client_id", "path"]
    });
    spec["components"]["schemas"]["ShellFileOpResponse"] = serde_json::json!({
        "type": "object",
        "properties": {
            "success": { "type": "boolean" },
            "op": { "type": "string" },
            "request_id": { "type": "string" },
            "client_id": { "type": "string" },
            "path": { "type": "string" },
            "cwd": { "type": "string", "nullable": true },
            "content": { "type": "string", "nullable": true },
            "entries": { "type": "array", "items": { "type": "string" } },
            "bytes": { "type": "integer", "nullable": true },
            "sha256": { "type": "string", "nullable": true },
            "stderr": { "type": "string", "nullable": true },
            "error": { "type": "string", "nullable": true }
        },
        "required": ["success", "op", "request_id", "client_id", "path"]
    });
    spec["components"]["schemas"]["ShellJobOpRequest"] = serde_json::json!({
        "type": "object",
        "properties": {
            "op": { "type": "string", "enum": ["start", "status", "log", "stop", "list"] },
            "client_id": { "type": "string", "nullable": true },
            "cwd": { "type": "string", "nullable": true },
            "command": { "type": "string", "nullable": true, "maxLength": 8000 },
            "timeout_secs": { "type": "integer", "nullable": true },
            "job_id": { "type": "string", "nullable": true },
            "since_stdout_line": { "type": "integer", "nullable": true },
            "since_stderr_line": { "type": "integer", "nullable": true },
            "tail_lines": { "type": "integer", "nullable": true },
            "limit": { "type": "integer", "nullable": true },
            "codex": { "$ref": "#/components/schemas/ShellJobCodexMetadata", "nullable": true }
        },
        "required": ["op"]
    });
    spec["components"]["schemas"]["ShellJobCodexMetadata"] = serde_json::json!({
        "type": "object",
        "properties": {
            "project": { "type": "string", "nullable": true },
            "goal_id": { "type": "string", "nullable": true },
            "client_request_id": { "type": "string", "nullable": true },
            "command": { "type": "string", "nullable": true },
            "kind": { "type": "string", "nullable": true },
            "suite": { "type": "string", "nullable": true },
            "script_path": { "type": "string", "nullable": true },
            "reason": { "type": "string", "nullable": true },
            "max_runtime_secs": { "type": "integer", "nullable": true }
        }
    });
    spec["components"]["schemas"]["ShellJobInfo"] = serde_json::json!({
        "type": "object",
        "properties": {
            "job_id": { "type": "string" },
            "request_id": { "type": "string", "nullable": true },
            "client_id": { "type": "string" },
            "cwd": { "type": "string", "nullable": true },
            "command_preview": { "type": "string" },
            "status": { "type": "string" },
            "created_at": { "type": "integer" },
            "started_at": { "type": "integer", "nullable": true },
            "ended_at": { "type": "integer", "nullable": true },
            "exit_code": { "type": "integer", "nullable": true },
            "duration_ms": { "type": "integer", "nullable": true },
            "error": { "type": "string", "nullable": true },
            "codex": { "$ref": "#/components/schemas/ShellJobCodexMetadata", "nullable": true }
        },
        "required": ["job_id", "client_id", "command_preview", "status", "created_at"]
    });
    spec["components"]["schemas"]["ShellJobOpResponse"] = serde_json::json!({
        "type": "object",
        "properties": {
            "success": { "type": "boolean" },
            "op": { "type": "string" },
            "job": { "$ref": "#/components/schemas/ShellJobInfo", "nullable": true },
            "jobs": { "type": "array", "items": { "$ref": "#/components/schemas/ShellJobInfo" } },
            "stdout": { "type": "string", "nullable": true },
            "stderr": { "type": "string", "nullable": true },
            "next_stdout_line": { "type": "integer", "nullable": true },
            "next_stderr_line": { "type": "integer", "nullable": true },
            "error": { "type": "string", "nullable": true }
        },
        "required": ["success", "op"]
    });
}

fn apply_trusted_command_guidance(spec: &mut serde_json::Value) {
    // Update command_request_op description to mention trusted raw mode
    let cr_op_desc = &mut spec["paths"]["/api/codex/command_request_op"]["post"]["description"];
    if cr_op_desc
        .as_str()
        .map_or(true, |s| !s.contains("create_trusted_raw"))
    {
        *cr_op_desc = serde_json::json!(
            "Command requests: create_trusted_raw_and_approve for short scripts; create_trusted_raw unsupported; runJobOp for long jobs."
        );
    }

    // Update job description to mention trusted script_text
    let job_desc = &mut spec["paths"]["/api/codex/job"]["post"]["description"];
    if job_desc
        .as_str()
        .map_or(true, |s| !s.contains("script_text"))
    {
        *job_desc = serde_json::json!(
            "Create/manage async active-goal jobs. Use recover after timeout. detail=basic is lightweight; detail=logs includes tails."
        );
    }

    // Add fields to CommandRequestOpRequest
    {
        let cr_props = &mut spec["components"]["schemas"]["CommandRequestOpRequest"]["properties"];
        if cr_props["script_text"].is_null() {
            cr_props["script_text"] = serde_json::json!({
                "type": "string",
                "description": "For create_trusted_raw_and_approve: multi-line shell script content. Supports grep, python one-liners, file stats. create_trusted_raw (non-approve) is currently unsupported."
            });
        }
        if cr_props["timeout_secs"].is_null() {
            cr_props["timeout_secs"] = serde_json::json!({
                "type": "integer",
                "description": "For create_trusted_raw_and_approve: timeout in seconds. Default 120, max 1800."
            });
        }
        if cr_props["response_mode"].is_null() {
            cr_props["response_mode"] = serde_json::json!({
                "type": "string",
                "enum": ["summary", "full", "minimal"],
                "description": "For create_trusted_raw_and_approve: summary (default, tail only), full (more output, still truncated), minimal (success/exit_code/cwd only)."
            });
        }
        // Add create_trusted_raw and create_trusted_raw_and_approve to op enum
        if let Some(op_enum) = cr_props["op"]["enum"].as_array_mut() {
            let ops: Vec<String> = op_enum
                .iter()
                .map(|v| v.as_str().unwrap_or("").to_string())
                .collect();
            if !ops.contains(&"create_trusted_raw".to_string()) {
                op_enum.push(serde_json::json!("create_trusted_raw"));
            }
            if !ops.contains(&"create_trusted_raw_and_approve".to_string()) {
                op_enum.push(serde_json::json!("create_trusted_raw_and_approve"));
            }
        }
    }

    // Add fields to JobOpRequest
    {
        let job_props = &mut spec["components"]["schemas"]["JobOpRequest"]["properties"];
        if job_props["script_text"].is_null() {
            job_props["script_text"] = serde_json::json!({
                "type": "string",
                "description": "For trusted job creation: multi-line script content. Requires trusted=true. Local writes script.sh; agent runs via agent shell; SSH is not yet supported."
            });
        }
        if job_props["trusted"].is_null() {
            job_props["trusted"] = serde_json::json!({
                "type": "boolean",
                "description": "For trusted job creation: must be true when script_text is provided. Local and agent executors are supported; SSH is not yet supported."
            });
        }
    }

    // Add trusted_result to CommandRequestOpResponse
    {
        let cr_resp_props =
            &mut spec["components"]["schemas"]["CommandRequestOpResponse"]["properties"];
        if cr_resp_props["trusted_result"].is_null() {
            cr_resp_props["trusted_result"] = serde_json::json!({
                "type": "object",
                "description": "For create_trusted_raw_and_approve: structured execution result.",
                "properties": {
                    "exit_code": { "type": "integer" },
                    "duration_ms": { "type": "integer" },
                    "cwd": { "type": "string" },
                    "stdout_tail": { "type": "string" },
                    "stderr_tail": { "type": "string" },
                    "stdout_truncated": { "type": "boolean" },
                    "stderr_truncated": { "type": "boolean" },
                    "audit_log_path": { "type": "string" },
                    "blocked_by_denylist": { "type": "boolean" }
                }
            });
        }
    }
}

#[handler]
pub async fn codex_openapi_json(res: &mut Response) {
    let mut spec =
        match serde_json::from_str::<serde_json::Value>(include_str!("../data/openapi.json")) {
            Ok(spec) => spec,
            Err(e) => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &e.to_string(),
                ));
                return;
            }
        };
    spec["openapi"] = serde_json::json!("3.1.0");
    spec["servers"] = serde_json::json!([{ "url": public_url(), "description": "Public server" }]);
    spec["info"] = serde_json::json!({"title":"Private Drop Codex API","version":env!("CARGO_PKG_VERSION"),"description":"Codex-only project API. Message, file, and channel APIs are excluded."});
    spec["paths"] = serde_json::json!({
        "/api/codex/context": spec["paths"]["/api/codex/context"].clone(),
        "/api/codex/projects": spec["paths"]["/api/codex/projects"].clone(),
        "/api/codex/context_batch": spec["paths"]["/api/codex/context_batch"].clone(),
        "/api/codex/apply_patch": spec["paths"]["/api/codex/apply_patch"].clone(),
        "/api/codex/edit": spec["paths"]["/api/codex/edit"].clone(),
        "/api/codex/artifact": spec["paths"]["/api/codex/artifact"].clone(),
        "/api/codex/git": spec["paths"]["/api/codex/git"].clone(),
        "/api/codex/project_hook": spec["paths"]["/api/codex/project_hook"].clone(),
        "/api/codex/project_doctor": spec["paths"]["/api/codex/project_doctor"].clone(),
        "/api/codex/project_workflow": spec["paths"]["/api/codex/project_workflow"].clone(),
        "/api/codex/command": spec["paths"]["/api/codex/command"].clone(),
        "/api/codex/command_request": spec["paths"]["/api/codex/command_request"].clone(),
        "/api/codex/command_request_op": spec["paths"]["/api/codex/command_request_op"].clone(),
        "/api/codex/job": spec["paths"]["/api/codex/job"].clone(),
        "/api/codex/command_request_raw": spec["paths"]["/api/codex/command_request_raw"].clone(),
        "/api/codex/command_requests": spec["paths"]["/api/codex/command_requests"].clone(),
        "/api/codex/command_request_batch": spec["paths"]["/api/codex/command_request_batch"].clone(),
        "/api/codex/command_approve": spec["paths"]["/api/codex/command_approve"].clone(),
        "/api/codex/command_reject": spec["paths"]["/api/codex/command_reject"].clone(),
        "/api/codex/check": spec["paths"]["/api/codex/check"].clone(),
        "/api/codex/report": spec["paths"]["/api/codex/report"].clone()
    });
    spec["components"]["schemas"] = serde_json::json!({
        "ContextRequest": spec["components"]["schemas"]["ContextRequest"].clone(),
        "ContextResponse": spec["components"]["schemas"]["ContextResponse"].clone(),
        "ContextBatchItem": spec["components"]["schemas"]["ContextBatchItem"].clone(),
        "ContextBatchRequest": spec["components"]["schemas"]["ContextBatchRequest"].clone(),
        "ContextBatchResultMetadata": spec["components"]["schemas"]["ContextBatchResultMetadata"].clone(),
        "ContextBatchResponse": spec["components"]["schemas"]["ContextBatchResponse"].clone(),
        "PatchRequest": spec["components"]["schemas"]["PatchRequest"].clone(),
        "PatchResponse": spec["components"]["schemas"]["PatchResponse"].clone(),
        "ReplaceTextEdit": spec["components"]["schemas"]["ReplaceTextEdit"].clone(),
        "ReplaceRangeEdit": spec["components"]["schemas"]["ReplaceRangeEdit"].clone(),
        "AppendFileEdit": spec["components"]["schemas"]["AppendFileEdit"].clone(),
        "CreateFileEdit": spec["components"]["schemas"]["CreateFileEdit"].clone(),
        "WriteFileEdit": spec["components"]["schemas"]["WriteFileEdit"].clone(),
        "CreateBinaryFileEdit": spec["components"]["schemas"]["CreateBinaryFileEdit"].clone(),
        "WriteBinaryFileEdit": spec["components"]["schemas"]["WriteBinaryFileEdit"].clone(),
        "CreateBinaryArtifactEdit": spec["components"]["schemas"]["CreateBinaryArtifactEdit"].clone(),
        "WriteBinaryArtifactEdit": spec["components"]["schemas"]["WriteBinaryArtifactEdit"].clone(),
        "CreateBinaryFileFromUploadEdit": spec["components"]["schemas"]["CreateBinaryFileFromUploadEdit"].clone(),
        "WriteBinaryFileFromUploadEdit": spec["components"]["schemas"]["WriteBinaryFileFromUploadEdit"].clone(),
        "CreateBinaryFileFromUrlEdit": spec["components"]["schemas"]["CreateBinaryFileFromUrlEdit"].clone(),
        "WriteBinaryFileFromUrlEdit": spec["components"]["schemas"]["WriteBinaryFileFromUrlEdit"].clone(),
        "EditRequest": spec["components"]["schemas"]["EditRequest"].clone(),
        "EditResponse": spec["components"]["schemas"]["EditResponse"].clone(),
        "EditPostCheckResult": spec["components"]["schemas"]["EditPostCheckResult"].clone(),
        "ArtifactRequest": spec["components"]["schemas"]["ArtifactRequest"].clone(),
        "ArtifactResponse": spec["components"]["schemas"]["ArtifactResponse"].clone(),
        "GitRequest": spec["components"]["schemas"]["GitRequest"].clone(),
        "GitResponse": spec["components"]["schemas"]["GitResponse"].clone(),
        "ProjectHookRequest": spec["components"]["schemas"]["ProjectHookRequest"].clone(),
        "ProjectHookStep": spec["components"]["schemas"]["ProjectHookStep"].clone(),
        "ProjectHookResponse": spec["components"]["schemas"]["ProjectHookResponse"].clone(),
        "ProjectDoctorRequest": spec["components"]["schemas"]["ProjectDoctorRequest"].clone(),
        "ProjectDoctorAgentCapabilities": spec["components"]["schemas"]["ProjectDoctorAgentCapabilities"].clone(),
        "ProjectDoctorAgentInfo": spec["components"]["schemas"]["ProjectDoctorAgentInfo"].clone(),
        "ProjectDoctorGitInfo": spec["components"]["schemas"]["ProjectDoctorGitInfo"].clone(),
        "ProjectDoctorHooksInfo": spec["components"]["schemas"]["ProjectDoctorHooksInfo"].clone(),
        "ProjectDoctorRecentJob": spec["components"]["schemas"]["ProjectDoctorRecentJob"].clone(),
        "ProjectDoctorResponse": spec["components"]["schemas"]["ProjectDoctorResponse"].clone(),
        "ProjectWorkflowRequest": spec["components"]["schemas"]["ProjectWorkflowRequest"].clone(),
        "ProjectWorkflowGitSnapshot": spec["components"]["schemas"]["ProjectWorkflowGitSnapshot"].clone(),
        "ProjectWorkflowResponse": spec["components"]["schemas"]["ProjectWorkflowResponse"].clone(),
        "CommandRequest": spec["components"]["schemas"]["CommandRequest"].clone(),
        "CommandResponse": spec["components"]["schemas"]["CommandResponse"].clone(),
        "CommandRequestCreate": spec["components"]["schemas"]["CommandRequestCreate"].clone(),
        "RawCommandRequestCreate": spec["components"]["schemas"]["RawCommandRequestCreate"].clone(),
        "CommandRequestBatchItem": spec["components"]["schemas"]["CommandRequestBatchItem"].clone(),
        "CommandRequestBatchCreate": spec["components"]["schemas"]["CommandRequestBatchCreate"].clone(),
        "CommandRequestsListRequest": spec["components"]["schemas"]["CommandRequestsListRequest"].clone(),
        "CommandApproveRequest": spec["components"]["schemas"]["CommandApproveRequest"].clone(),
        "CommandRejectRequest": spec["components"]["schemas"]["CommandRejectRequest"].clone(),
        "CommandRequestOpRequest": spec["components"]["schemas"]["CommandRequestOpRequest"].clone(),
        "CommandRequestOpResponse": spec["components"]["schemas"]["CommandRequestOpResponse"].clone(),
        "JobOpRequest": spec["components"]["schemas"]["JobOpRequest"].clone(),
        "JobInfo": spec["components"]["schemas"]["JobInfo"].clone(),
        "JobOpResponse": spec["components"]["schemas"]["JobOpResponse"].clone(),
        "ProjectCapabilities": spec["components"]["schemas"]["ProjectCapabilities"].clone(),
        "ProjectCapabilityInfo": spec["components"]["schemas"]["ProjectCapabilityInfo"].clone(),
        "InstanceInfo": spec["components"]["schemas"]["InstanceInfo"].clone(),
        "ProjectsResponse": spec["components"]["schemas"]["ProjectsResponse"].clone(),
        "CommandRequestResponse": spec["components"]["schemas"]["CommandRequestResponse"].clone(),
        "CommandRequestsListResponse": spec["components"]["schemas"]["CommandRequestsListResponse"].clone(),
        "CommandRequestBatchResponse": spec["components"]["schemas"]["CommandRequestBatchResponse"].clone(),
        "CheckRequest": spec["components"]["schemas"]["CheckRequest"].clone(),
        "CheckResponse": spec["components"]["schemas"]["CheckResponse"].clone(),
        "ReportRequest": spec["components"]["schemas"]["ReportRequest"].clone(),
        "ReportResponse": spec["components"]["schemas"]["ReportResponse"].clone()
    });
    apply_project_description_to_schema(
        &mut spec,
        &[
            "ContextRequest",
            "ContextBatchRequest",
            "PatchRequest",
            "EditRequest",
            "ArtifactRequest",
            "GitRequest",
            "ProjectHookRequest",
            "ProjectDoctorRequest",
            "ProjectWorkflowRequest",
            "CommandRequest",
            "CommandRequestCreate",
            "RawCommandRequestCreate",
            "CommandRequestBatchCreate",
            "CommandRequestsListRequest",
            "CommandRequestOpRequest",
            "JobOpRequest",
            "CommandApproveRequest",
            "CommandRejectRequest",
            "CheckRequest",
            "ReportRequest",
        ],
    );
    apply_edit_timeout_guidance(&mut spec);
    apply_job_recovery_guidance(&mut spec);
    apply_context_batch_guidance(&mut spec);
    apply_trusted_command_guidance(&mut spec);
    apply_action_session_openapi(&mut spec);
    spec["components"]["schemas"]["ReportRequest"]["properties"]["channel"]["description"] =
        serde_json::json!("Report channel; not the project field.");
    res.render(Json(spec));
}

#[cfg(test)]
mod tests {
    use super::{
        apply_action_session_openapi, apply_edit_timeout_guidance, apply_shell_client_openapi,
        apply_trusted_command_guidance,
    };

    fn assert_operation_descriptions_within_limit(spec: &serde_json::Value) {
        let paths = spec["paths"].as_object().unwrap();
        for (path, operations) in paths {
            let operations = operations.as_object().unwrap();
            for (method, operation) in operations {
                if let Some(description) = operation["description"].as_str() {
                    assert!(
                        description.len() <= 300,
                        "{} {} description length {} > 300",
                        method,
                        path,
                        description.len()
                    );
                }
            }
        }
    }

    fn assert_all_descriptions_within_limit(spec: &serde_json::Value) {
        fn walk(path: String, value: &serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    for (key, child) in map {
                        let child_path = if path.is_empty() {
                            key.clone()
                        } else {
                            format!("{}.{}", path, key)
                        };
                        if key == "description" {
                            if let Some(description) = child.as_str() {
                                assert!(
                                    description.len() <= 300,
                                    "{} description length {} > 300",
                                    child_path,
                                    description.len()
                                );
                            }
                        }
                        walk(child_path, child);
                    }
                }
                serde_json::Value::Array(items) => {
                    for (idx, child) in items.iter().enumerate() {
                        walk(format!("{}[{}]", path, idx), child);
                    }
                }
                _ => {}
            }
        }

        walk(String::new(), spec);
    }

    fn compact_paths_from_spec(spec: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "/api/codex/projects": spec["paths"]["/api/codex/projects"].clone(),
            "/api/codex/context_batch": spec["paths"]["/api/codex/context_batch"].clone(),
            "/api/codex/edit": spec["paths"]["/api/codex/edit"].clone(),
            "/api/codex/artifact": spec["paths"]["/api/codex/artifact"].clone(),
            "/api/codex/git": spec["paths"]["/api/codex/git"].clone(),
            "/api/codex/command": spec["paths"]["/api/codex/command"].clone(),
            "/api/codex/command_request_op": spec["paths"]["/api/codex/command_request_op"].clone(),
            "/api/codex/job": spec["paths"]["/api/codex/job"].clone(),
            "/api/codex/check": spec["paths"]["/api/codex/check"].clone(),
            "/api/codex/report": spec["paths"]["/api/codex/report"].clone(),
            "/api/codex/action_sessions": spec["paths"]["/api/codex/action_sessions"].clone(),
            "/api/shell/clients": spec["paths"]["/api/shell/clients"].clone(),
            "/api/shell/projects": spec["paths"]["/api/shell/projects"].clone(),
            "/api/shell/projects/create": spec["paths"]["/api/shell/projects/create"].clone(),
            "/api/shell/run": spec["paths"]["/api/shell/run"].clone(),
            "/api/shell/file": spec["paths"]["/api/shell/file"].clone()
        })
    }

    fn gpt_paths_from_spec(spec: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "/api/codex/projects": spec["paths"]["/api/codex/projects"].clone(),
            "/api/codex/context_batch": spec["paths"]["/api/codex/context_batch"].clone(),
            "/api/codex/edit": spec["paths"]["/api/codex/edit"].clone(),
            "/api/codex/artifact": spec["paths"]["/api/codex/artifact"].clone(),
            "/api/codex/git": spec["paths"]["/api/codex/git"].clone(),
            "/api/codex/command_request_op": spec["paths"]["/api/codex/command_request_op"].clone(),
            "/api/codex/job": spec["paths"]["/api/codex/job"].clone(),
            "/api/codex/check": spec["paths"]["/api/codex/check"].clone(),
            "/api/codex/report": spec["paths"]["/api/codex/report"].clone(),
            "/api/codex/action_sessions": spec["paths"]["/api/codex/action_sessions"].clone(),
            "/api/shell/clients": spec["paths"]["/api/shell/clients"].clone(),
            "/api/shell/projects": spec["paths"]["/api/shell/projects"].clone(),
            "/api/shell/projects/create": spec["paths"]["/api/shell/projects/create"].clone(),
            "/api/shell/run": spec["paths"]["/api/shell/run"].clone()
        })
    }

    fn count_operations(paths: &serde_json::Value) -> usize {
        paths
            .as_object()
            .unwrap()
            .values()
            .flat_map(|path_item| path_item.as_object().unwrap().values())
            .filter(|operation| !operation["operationId"].is_null())
            .count()
    }

    fn assert_unique_operation_ids(paths: &serde_json::Value) {
        let mut seen = std::collections::BTreeSet::new();
        for (path, path_item) in paths.as_object().unwrap() {
            for (method, operation) in path_item.as_object().unwrap() {
                let Some(operation_id) = operation["operationId"].as_str() else {
                    continue;
                };
                assert!(
                    seen.insert(operation_id.to_string()),
                    "duplicate operationId {} at {} {}",
                    operation_id,
                    method,
                    path
                );
            }
        }
    }

    fn assert_refs_resolve(spec: &serde_json::Value) {
        fn walk(path: String, root: &serde_json::Value, value: &serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    if let Some(reference) = map.get("$ref").and_then(|v| v.as_str()) {
                        let name = reference
                            .strip_prefix("#/components/schemas/")
                            .unwrap_or_else(|| panic!("unsupported ref {} at {}", reference, path));
                        assert!(
                            !root["components"]["schemas"][name].is_null(),
                            "unresolved ref {} at {}",
                            reference,
                            path
                        );
                    }
                    for (key, child) in map {
                        let child_path = if path.is_empty() {
                            key.clone()
                        } else {
                            format!("{}.{}", path, key)
                        };
                        walk(child_path, root, child);
                    }
                }
                serde_json::Value::Array(items) => {
                    for (idx, child) in items.iter().enumerate() {
                        walk(format!("{}[{}]", path, idx), root, child);
                    }
                }
                _ => {}
            }
        }

        walk(String::new(), spec, spec);
    }

    #[test]
    fn apply_project_edit_description_stays_under_300_chars() {
        let spec: serde_json::Value =
            serde_json::from_str(include_str!("../data/openapi.json")).unwrap();
        let description = spec["paths"]["/api/codex/edit"]["post"]["description"]
            .as_str()
            .unwrap();
        assert!(description.len() <= 300, "{}", description.len());
    }

    #[test]
    fn compact_schema_mentions_post_check_for_edits() {
        let mut spec: serde_json::Value =
            serde_json::from_str(include_str!("../data/openapi.json")).unwrap();
        apply_edit_timeout_guidance(&mut spec);
        let description = spec["paths"]["/api/codex/edit"]["post"]["description"]
            .as_str()
            .unwrap();
        let response_mode_description = spec["components"]["schemas"]["EditRequest"]["properties"]
            ["response_mode"]["description"]
            .as_str()
            .unwrap();
        assert!(description.len() <= 300, "{}", description.len());
        assert!(description.contains("post_check"));
        assert!(description.contains("auto-rollback"));
        assert!(response_mode_description.contains("multi-file edits"));
        assert!(response_mode_description.contains("use summary"));
    }

    #[test]
    fn context_batch_description_mentions_split_and_ssh() {
        let spec: serde_json::Value =
            serde_json::from_str(include_str!("../data/openapi.json")).unwrap();
        let description = spec["paths"]["/api/codex/context_batch"]["post"]["description"]
            .as_str()
            .unwrap();
        assert!(
            description.contains("split") || description.contains("Split"),
            "context_batch description should mention split: {}",
            description
        );
        assert!(
            description.contains("SSH"),
            "context_batch description should mention SSH: {}",
            description
        );
        assert!(
            description.contains("preflight_rejected"),
            "context_batch description should mention preflight_rejected: {}",
            description
        );
    }

    #[test]
    fn context_batch_max_total_chars_description_mentions_limit() {
        let spec: serde_json::Value =
            serde_json::from_str(include_str!("../data/openapi.json")).unwrap();
        let description = spec["components"]["schemas"]["ContextBatchRequest"]["properties"]
            ["max_total_chars"]["description"]
            .as_str()
            .unwrap();
        assert!(
            description.contains("80000"),
            "max_total_chars description should mention 80000: {}",
            description
        );
    }

    #[test]
    fn context_batch_response_has_preflight_fields() {
        let spec: serde_json::Value =
            serde_json::from_str(include_str!("../data/openapi.json")).unwrap();
        let item_props = &spec["components"]["schemas"]["ContextBatchItem"]["properties"];
        assert!(
            !item_props["if_fingerprint"].is_null(),
            "ContextBatchItem should have if_fingerprint"
        );
        let edit_props = &spec["components"]["schemas"]["EditRequest"]["properties"];
        assert!(
            !edit_props["expected_fingerprints"].is_null(),
            "EditRequest should have expected_fingerprints"
        );
        let props = &spec["components"]["schemas"]["ContextBatchResponse"]["properties"];
        assert!(
            !props["preflight_rejected"].is_null(),
            "ContextBatchResponse should have preflight_rejected"
        );
        assert!(
            !props["estimated_chars"].is_null(),
            "ContextBatchResponse should have estimated_chars"
        );
        assert!(
            !props["suggestion"].is_null(),
            "ContextBatchResponse should have suggestion"
        );
        assert!(
            !props["warnings"].is_null(),
            "ContextBatchResponse should have warnings"
        );
        assert!(
            !props["result_metadata"].is_null(),
            "ContextBatchResponse should have result_metadata"
        );
        assert!(
            !props["cache_hits"].is_null(),
            "ContextBatchResponse should have cache_hits"
        );
        assert!(
            !props["recommended_next_action"].is_null(),
            "ContextBatchResponse should have recommended_next_action"
        );
        assert!(
            !spec["components"]["schemas"]["ContextBatchResultMetadata"].is_null(),
            "ContextBatchResultMetadata should exist"
        );
    }

    #[test]
    fn trusted_command_guidance_adds_fields() {
        let mut spec: serde_json::Value =
            serde_json::from_str(include_str!("../data/openapi.json")).unwrap();
        apply_trusted_command_guidance(&mut spec);
        let cr_desc = spec["paths"]["/api/codex/command_request_op"]["post"]["description"]
            .as_str()
            .unwrap();
        assert!(
            cr_desc.contains("create_trusted_raw"),
            "command_request_op description should mention create_trusted_raw: {}",
            cr_desc
        );
        let cr_props = &spec["components"]["schemas"]["CommandRequestOpRequest"]["properties"];
        assert!(
            !cr_props["script_text"].is_null(),
            "script_text should be added"
        );
        assert!(
            !cr_props["timeout_secs"].is_null(),
            "timeout_secs should be added"
        );
        assert!(
            !cr_props["response_mode"].is_null(),
            "response_mode should be added"
        );
        let job_props = &spec["components"]["schemas"]["JobOpRequest"]["properties"];
        assert!(
            !job_props["script_text"].is_null(),
            "JobOpRequest script_text should be added"
        );
        assert!(
            !job_props["trusted"].is_null(),
            "JobOpRequest trusted should be added"
        );
        let resp_props = &spec["components"]["schemas"]["CommandRequestOpResponse"]["properties"];
        assert!(
            !resp_props["trusted_result"].is_null(),
            "CommandRequestOpResponse trusted_result should be added"
        );
    }

    #[test]
    fn action_sessions_openapi_is_exposed() {
        let mut spec: serde_json::Value =
            serde_json::from_str(include_str!("../data/openapi.json")).unwrap();
        apply_action_session_openapi(&mut spec);
        assert!(
            !spec["paths"]["/api/codex/action_sessions"]["post"].is_null(),
            "action_sessions endpoint should exist"
        );
        assert!(
            !spec["components"]["schemas"]["ActionSessionOpRequest"].is_null(),
            "ActionSessionOpRequest schema should exist"
        );
        assert!(
            !spec["components"]["schemas"]["ActionSessionOpResponse"].is_null(),
            "ActionSessionOpResponse schema should exist"
        );
    }

    #[test]
    fn action_session_guidance_mentions_header() {
        let mut spec: serde_json::Value =
            serde_json::from_str(include_str!("../data/openapi.json")).unwrap();
        apply_action_session_openapi(&mut spec);
        let description = spec["paths"]["/api/codex/edit"]["post"]["description"]
            .as_str()
            .unwrap();
        assert!(description.contains("X-Action-Session-Id"));
        assert!(description.contains("rolling action sessions"));
    }

    #[test]
    fn compact_openapi_includes_action_sessions_path() {
        let mut spec: serde_json::Value =
            serde_json::from_str(include_str!("../data/openapi.json")).unwrap();
        apply_action_session_openapi(&mut spec);
        apply_shell_client_openapi(&mut spec);
        let compact_paths = compact_paths_from_spec(&spec);
        assert!(
            !compact_paths["/api/codex/action_sessions"]["post"].is_null(),
            "compact schema should include action_sessions"
        );
        assert!(
            !compact_paths["/api/shell/clients"]["post"].is_null(),
            "compact schema should include shell clients"
        );
        assert!(
            !compact_paths["/api/shell/projects"]["post"].is_null(),
            "compact schema should include shell projects"
        );
        assert!(
            !compact_paths["/api/shell/projects/create"]["post"].is_null(),
            "compact schema should include shell project creation"
        );
        assert!(
            !compact_paths["/api/shell/run"]["post"].is_null(),
            "compact schema should include a basic shell operation"
        );
        assert!(
            compact_paths["/api/shell/job"].is_null(),
            "compact schema should omit shell job to stay within the action budget"
        );
        assert!(
            compact_paths["/api/desktop/task_op"].is_null(),
            "compact schema should omit desktop task to stay within the action budget"
        );
    }

    #[test]
    fn compact_openapi_operation_count_stays_under_gpt_limit() {
        let mut spec: serde_json::Value =
            serde_json::from_str(include_str!("../data/openapi.json")).unwrap();
        apply_action_session_openapi(&mut spec);
        apply_shell_client_openapi(&mut spec);
        let compact_paths = compact_paths_from_spec(&spec);
        assert!(
            count_operations(&compact_paths) <= 16,
            "compact schema must stay within the 15-action GPT limit"
        );
    }

    #[test]
    fn compact_openapi_gpt_lint_stays_strict() {
        let mut spec: serde_json::Value =
            serde_json::from_str(include_str!("../data/openapi.json")).unwrap();
        apply_edit_timeout_guidance(&mut spec);
        apply_trusted_command_guidance(&mut spec);
        apply_action_session_openapi(&mut spec);
        apply_shell_client_openapi(&mut spec);
        spec["paths"] = compact_paths_from_spec(&spec);
        spec["components"]["schemas"] = serde_json::json!({
            "ContextResponse": spec["components"]["schemas"]["ContextResponse"].clone(),
            "ContextBatchItem": spec["components"]["schemas"]["ContextBatchItem"].clone(),
            "ContextBatchRequest": spec["components"]["schemas"]["ContextBatchRequest"].clone(),
            "ContextBatchResultMetadata": spec["components"]["schemas"]["ContextBatchResultMetadata"].clone(),
            "ContextBatchResponse": spec["components"]["schemas"]["ContextBatchResponse"].clone(),
            "ReplaceTextEdit": spec["components"]["schemas"]["ReplaceTextEdit"].clone(),
            "ReplaceRangeEdit": spec["components"]["schemas"]["ReplaceRangeEdit"].clone(),
            "AppendFileEdit": spec["components"]["schemas"]["AppendFileEdit"].clone(),
            "CreateFileEdit": spec["components"]["schemas"]["CreateFileEdit"].clone(),
            "WriteFileEdit": spec["components"]["schemas"]["WriteFileEdit"].clone(),
            "CreateBinaryFileEdit": spec["components"]["schemas"]["CreateBinaryFileEdit"].clone(),
            "WriteBinaryFileEdit": spec["components"]["schemas"]["WriteBinaryFileEdit"].clone(),
            "CreateBinaryArtifactEdit": spec["components"]["schemas"]["CreateBinaryArtifactEdit"].clone(),
            "WriteBinaryArtifactEdit": spec["components"]["schemas"]["WriteBinaryArtifactEdit"].clone(),
            "CreateBinaryFileFromUploadEdit": spec["components"]["schemas"]["CreateBinaryFileFromUploadEdit"].clone(),
            "WriteBinaryFileFromUploadEdit": spec["components"]["schemas"]["WriteBinaryFileFromUploadEdit"].clone(),
            "CreateBinaryFileFromUrlEdit": spec["components"]["schemas"]["CreateBinaryFileFromUrlEdit"].clone(),
            "WriteBinaryFileFromUrlEdit": spec["components"]["schemas"]["WriteBinaryFileFromUrlEdit"].clone(),
            "EditRequest": spec["components"]["schemas"]["EditRequest"].clone(),
            "EditResponse": spec["components"]["schemas"]["EditResponse"].clone(),
            "EditPostCheckResult": spec["components"]["schemas"]["EditPostCheckResult"].clone(),
            "ArtifactRequest": spec["components"]["schemas"]["ArtifactRequest"].clone(),
            "ArtifactResponse": spec["components"]["schemas"]["ArtifactResponse"].clone(),
            "GitRequest": spec["components"]["schemas"]["GitRequest"].clone(),
            "GitResponse": spec["components"]["schemas"]["GitResponse"].clone(),
            "CommandRequest": spec["components"]["schemas"]["CommandRequest"].clone(),
            "CommandResponse": spec["components"]["schemas"]["CommandResponse"].clone(),
            "CommandRequestBatchItem": spec["components"]["schemas"]["CommandRequestBatchItem"].clone(),
            "CommandRequestOpRequest": spec["components"]["schemas"]["CommandRequestOpRequest"].clone(),
            "CommandRequestOpResponse": spec["components"]["schemas"]["CommandRequestOpResponse"].clone(),
            "JobOpRequest": spec["components"]["schemas"]["JobOpRequest"].clone(),
            "JobInfo": spec["components"]["schemas"]["JobInfo"].clone(),
            "JobOpResponse": spec["components"]["schemas"]["JobOpResponse"].clone(),
            "ProjectCapabilities": spec["components"]["schemas"]["ProjectCapabilities"].clone(),
            "ProjectCapabilityInfo": spec["components"]["schemas"]["ProjectCapabilityInfo"].clone(),
            "InstanceInfo": spec["components"]["schemas"]["InstanceInfo"].clone(),
            "ProjectsResponse": spec["components"]["schemas"]["ProjectsResponse"].clone(),
            "CheckRequest": spec["components"]["schemas"]["CheckRequest"].clone(),
            "CheckResponse": spec["components"]["schemas"]["CheckResponse"].clone(),
            "ReportRequest": spec["components"]["schemas"]["ReportRequest"].clone(),
            "ReportResponse": spec["components"]["schemas"]["ReportResponse"].clone(),
            "DesktopTask": spec["components"]["schemas"]["DesktopTask"].clone(),
            "DesktopTaskEvent": spec["components"]["schemas"]["DesktopTaskEvent"].clone(),
            "DesktopTaskOpRequest": spec["components"]["schemas"]["DesktopTaskOpRequest"].clone(),
            "DesktopTaskOpResponse": spec["components"]["schemas"]["DesktopTaskOpResponse"].clone(),
            "ActionSessionRecord": spec["components"]["schemas"]["ActionSessionRecord"].clone(),
            "ActionSessionStats": spec["components"]["schemas"]["ActionSessionStats"].clone(),
            "ActionEventView": spec["components"]["schemas"]["ActionEventView"].clone(),
            "ActionSessionListItem": spec["components"]["schemas"]["ActionSessionListItem"].clone(),
            "ActionSessionOpRequest": spec["components"]["schemas"]["ActionSessionOpRequest"].clone(),
            "ActionSessionOpResponse": spec["components"]["schemas"]["ActionSessionOpResponse"].clone(),
            "ShellAgentProjectSummary": spec["components"]["schemas"]["ShellAgentProjectSummary"].clone(),
            "ShellClientCapabilities": spec["components"]["schemas"]["ShellClientCapabilities"].clone(),
            "ShellClientView": spec["components"]["schemas"]["ShellClientView"].clone(),
            "ShellClientsResponse": spec["components"]["schemas"]["ShellClientsResponse"].clone(),
            "ShellClientProjectsRequest": spec["components"]["schemas"]["ShellClientProjectsRequest"].clone(),
            "ShellClientProjectsResponse": spec["components"]["schemas"]["ShellClientProjectsResponse"].clone(),
            "ShellClientProjectCreateRequest": spec["components"]["schemas"]["ShellClientProjectCreateRequest"].clone(),
            "ShellClientProjectCreateResponse": spec["components"]["schemas"]["ShellClientProjectCreateResponse"].clone(),
            "ShellRunRequest": spec["components"]["schemas"]["ShellRunRequest"].clone(),
            "ShellRunResponse": spec["components"]["schemas"]["ShellRunResponse"].clone(),
            "ShellFileOpRequest": spec["components"]["schemas"]["ShellFileOpRequest"].clone(),
            "ShellFileOpResponse": spec["components"]["schemas"]["ShellFileOpResponse"].clone(),
            "ShellJobOpRequest": spec["components"]["schemas"]["ShellJobOpRequest"].clone(),
            "ShellJobCodexMetadata": spec["components"]["schemas"]["ShellJobCodexMetadata"].clone(),
            "ShellJobInfo": spec["components"]["schemas"]["ShellJobInfo"].clone(),
            "ShellJobOpResponse": spec["components"]["schemas"]["ShellJobOpResponse"].clone()
        });
        assert!(count_operations(&spec["paths"]) <= 16);
        assert_operation_descriptions_within_limit(&spec);
        assert_all_descriptions_within_limit(&spec);
        assert_unique_operation_ids(&spec["paths"]);
        assert_refs_resolve(&spec);
    }

    #[test]
    fn gpt_openapi_schema_is_slimmer_and_valid() {
        let mut spec: serde_json::Value =
            serde_json::from_str(include_str!("../data/openapi.json")).unwrap();
        apply_edit_timeout_guidance(&mut spec);
        apply_trusted_command_guidance(&mut spec);
        apply_action_session_openapi(&mut spec);
        apply_shell_client_openapi(&mut spec);
        spec["paths"] = gpt_paths_from_spec(&spec);
        spec["components"]["schemas"] = serde_json::json!({
            "ContextResponse": spec["components"]["schemas"]["ContextResponse"].clone(),
            "ContextBatchItem": spec["components"]["schemas"]["ContextBatchItem"].clone(),
            "ContextBatchRequest": spec["components"]["schemas"]["ContextBatchRequest"].clone(),
            "ContextBatchResultMetadata": spec["components"]["schemas"]["ContextBatchResultMetadata"].clone(),
            "ContextBatchResponse": spec["components"]["schemas"]["ContextBatchResponse"].clone(),
            "ReplaceTextEdit": spec["components"]["schemas"]["ReplaceTextEdit"].clone(),
            "ReplaceRangeEdit": spec["components"]["schemas"]["ReplaceRangeEdit"].clone(),
            "AppendFileEdit": spec["components"]["schemas"]["AppendFileEdit"].clone(),
            "CreateFileEdit": spec["components"]["schemas"]["CreateFileEdit"].clone(),
            "WriteFileEdit": spec["components"]["schemas"]["WriteFileEdit"].clone(),
            "CreateBinaryFileEdit": spec["components"]["schemas"]["CreateBinaryFileEdit"].clone(),
            "WriteBinaryFileEdit": spec["components"]["schemas"]["WriteBinaryFileEdit"].clone(),
            "CreateBinaryArtifactEdit": spec["components"]["schemas"]["CreateBinaryArtifactEdit"].clone(),
            "WriteBinaryArtifactEdit": spec["components"]["schemas"]["WriteBinaryArtifactEdit"].clone(),
            "CreateBinaryFileFromUploadEdit": spec["components"]["schemas"]["CreateBinaryFileFromUploadEdit"].clone(),
            "WriteBinaryFileFromUploadEdit": spec["components"]["schemas"]["WriteBinaryFileFromUploadEdit"].clone(),
            "CreateBinaryFileFromUrlEdit": spec["components"]["schemas"]["CreateBinaryFileFromUrlEdit"].clone(),
            "WriteBinaryFileFromUrlEdit": spec["components"]["schemas"]["WriteBinaryFileFromUrlEdit"].clone(),
            "EditRequest": spec["components"]["schemas"]["EditRequest"].clone(),
            "EditResponse": spec["components"]["schemas"]["EditResponse"].clone(),
            "EditPostCheckResult": spec["components"]["schemas"]["EditPostCheckResult"].clone(),
            "ArtifactRequest": spec["components"]["schemas"]["ArtifactRequest"].clone(),
            "ArtifactResponse": spec["components"]["schemas"]["ArtifactResponse"].clone(),
            "GitRequest": spec["components"]["schemas"]["GitRequest"].clone(),
            "GitResponse": spec["components"]["schemas"]["GitResponse"].clone(),
            "CommandRequestBatchItem": spec["components"]["schemas"]["CommandRequestBatchItem"].clone(),
            "CommandRequestOpRequest": spec["components"]["schemas"]["CommandRequestOpRequest"].clone(),
            "CommandRequestOpResponse": spec["components"]["schemas"]["CommandRequestOpResponse"].clone(),
            "JobOpRequest": spec["components"]["schemas"]["JobOpRequest"].clone(),
            "JobInfo": spec["components"]["schemas"]["JobInfo"].clone(),
            "JobOpResponse": spec["components"]["schemas"]["JobOpResponse"].clone(),
            "ProjectCapabilities": spec["components"]["schemas"]["ProjectCapabilities"].clone(),
            "ProjectCapabilityInfo": spec["components"]["schemas"]["ProjectCapabilityInfo"].clone(),
            "InstanceInfo": spec["components"]["schemas"]["InstanceInfo"].clone(),
            "ProjectsResponse": spec["components"]["schemas"]["ProjectsResponse"].clone(),
            "CheckRequest": spec["components"]["schemas"]["CheckRequest"].clone(),
            "CheckResponse": spec["components"]["schemas"]["CheckResponse"].clone(),
            "ReportRequest": spec["components"]["schemas"]["ReportRequest"].clone(),
            "ReportResponse": spec["components"]["schemas"]["ReportResponse"].clone(),
            "ActionSessionRecord": spec["components"]["schemas"]["ActionSessionRecord"].clone(),
            "ActionSessionStats": spec["components"]["schemas"]["ActionSessionStats"].clone(),
            "ActionEventView": spec["components"]["schemas"]["ActionEventView"].clone(),
            "ActionSessionListItem": spec["components"]["schemas"]["ActionSessionListItem"].clone(),
            "ActionSessionOpRequest": spec["components"]["schemas"]["ActionSessionOpRequest"].clone(),
            "ActionSessionOpResponse": spec["components"]["schemas"]["ActionSessionOpResponse"].clone(),
            "ShellAgentProjectSummary": spec["components"]["schemas"]["ShellAgentProjectSummary"].clone(),
            "ShellClientCapabilities": spec["components"]["schemas"]["ShellClientCapabilities"].clone(),
            "ShellClientView": spec["components"]["schemas"]["ShellClientView"].clone(),
            "ShellClientsResponse": spec["components"]["schemas"]["ShellClientsResponse"].clone(),
            "ShellClientProjectsRequest": spec["components"]["schemas"]["ShellClientProjectsRequest"].clone(),
            "ShellClientProjectsResponse": spec["components"]["schemas"]["ShellClientProjectsResponse"].clone(),
            "ShellClientProjectCreateRequest": spec["components"]["schemas"]["ShellClientProjectCreateRequest"].clone(),
            "ShellClientProjectCreateResponse": spec["components"]["schemas"]["ShellClientProjectCreateResponse"].clone(),
            "ShellRunRequest": spec["components"]["schemas"]["ShellRunRequest"].clone(),
            "ShellRunResponse": spec["components"]["schemas"]["ShellRunResponse"].clone()
        });
        assert!(count_operations(&spec["paths"]) <= 16);
        assert!(spec["paths"]["/api/codex/command"].is_null());
        assert!(spec["paths"]["/api/desktop/task_op"].is_null());
        assert!(!spec["paths"]["/api/shell/clients"]["post"].is_null());
        assert!(!spec["paths"]["/api/shell/projects"]["post"].is_null());
        assert!(!spec["paths"]["/api/shell/projects/create"]["post"].is_null());
        assert!(!spec["paths"]["/api/shell/run"]["post"].is_null());
        assert_operation_descriptions_within_limit(&spec);
        assert_all_descriptions_within_limit(&spec);
        assert_unique_operation_ids(&spec["paths"]);
        assert_refs_resolve(&spec);
    }

    #[test]
    fn openapi_operation_descriptions_stay_under_300_chars() {
        let mut spec: serde_json::Value =
            serde_json::from_str(include_str!("../data/openapi.json")).unwrap();
        apply_edit_timeout_guidance(&mut spec);
        apply_trusted_command_guidance(&mut spec);
        apply_action_session_openapi(&mut spec);
        apply_shell_client_openapi(&mut spec);
        assert_operation_descriptions_within_limit(&spec);
        assert_all_descriptions_within_limit(&spec);
    }
}

#[handler]
pub async fn codex_openapi_compact_json(res: &mut Response) {
    let mut spec =
        match serde_json::from_str::<serde_json::Value>(include_str!("../data/openapi.json")) {
            Ok(spec) => spec,
            Err(e) => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &e.to_string(),
                ));
                return;
            }
        };
    spec["openapi"] = serde_json::json!("3.1.0");
    spec["servers"] = serde_json::json!([{ "url": public_url(), "description": "Public server" }]);
    spec["info"] = serde_json::json!({"title":"Private Drop Compact Codex API","version":env!("CARGO_PKG_VERSION"),"description":"Compact Codex project API for GPT Actions. Uses aggregate endpoints to reduce action count."});
    spec["paths"] = serde_json::json!({
        "/api/codex/projects": spec["paths"]["/api/codex/projects"].clone(),
        "/api/codex/context_batch": spec["paths"]["/api/codex/context_batch"].clone(),
        "/api/codex/edit": spec["paths"]["/api/codex/edit"].clone(),
        "/api/codex/artifact": spec["paths"]["/api/codex/artifact"].clone(),
        "/api/codex/git": spec["paths"]["/api/codex/git"].clone(),
        "/api/codex/project_hook": spec["paths"]["/api/codex/project_hook"].clone(),
        "/api/codex/project_doctor": spec["paths"]["/api/codex/project_doctor"].clone(),
        "/api/codex/project_workflow": spec["paths"]["/api/codex/project_workflow"].clone(),
        "/api/codex/command": spec["paths"]["/api/codex/command"].clone(),
        "/api/codex/command_request_op": spec["paths"]["/api/codex/command_request_op"].clone(),
        "/api/codex/job": spec["paths"]["/api/codex/job"].clone(),
        "/api/codex/check": spec["paths"]["/api/codex/check"].clone(),
        "/api/codex/report": spec["paths"]["/api/codex/report"].clone(),
        "/api/desktop/task_op": spec["paths"]["/api/desktop/task_op"].clone(),
        "/api/codex/action_sessions": spec["paths"]["/api/codex/action_sessions"].clone()
    });
    spec["components"]["schemas"] = serde_json::json!({
        "ContextResponse": spec["components"]["schemas"]["ContextResponse"].clone(),
        "ContextBatchItem": spec["components"]["schemas"]["ContextBatchItem"].clone(),
        "ContextBatchRequest": spec["components"]["schemas"]["ContextBatchRequest"].clone(),
        "ContextBatchResultMetadata": spec["components"]["schemas"]["ContextBatchResultMetadata"].clone(),
        "ContextBatchResponse": spec["components"]["schemas"]["ContextBatchResponse"].clone(),
        "ReplaceTextEdit": spec["components"]["schemas"]["ReplaceTextEdit"].clone(),
        "ReplaceRangeEdit": spec["components"]["schemas"]["ReplaceRangeEdit"].clone(),
        "AppendFileEdit": spec["components"]["schemas"]["AppendFileEdit"].clone(),
        "CreateFileEdit": spec["components"]["schemas"]["CreateFileEdit"].clone(),
        "WriteFileEdit": spec["components"]["schemas"]["WriteFileEdit"].clone(),
        "CreateBinaryFileEdit": spec["components"]["schemas"]["CreateBinaryFileEdit"].clone(),
        "WriteBinaryFileEdit": spec["components"]["schemas"]["WriteBinaryFileEdit"].clone(),
        "CreateBinaryArtifactEdit": spec["components"]["schemas"]["CreateBinaryArtifactEdit"].clone(),
        "WriteBinaryArtifactEdit": spec["components"]["schemas"]["WriteBinaryArtifactEdit"].clone(),
        "CreateBinaryFileFromUploadEdit": spec["components"]["schemas"]["CreateBinaryFileFromUploadEdit"].clone(),
        "WriteBinaryFileFromUploadEdit": spec["components"]["schemas"]["WriteBinaryFileFromUploadEdit"].clone(),
        "CreateBinaryFileFromUrlEdit": spec["components"]["schemas"]["CreateBinaryFileFromUrlEdit"].clone(),
        "WriteBinaryFileFromUrlEdit": spec["components"]["schemas"]["WriteBinaryFileFromUrlEdit"].clone(),
        "EditRequest": spec["components"]["schemas"]["EditRequest"].clone(),
        "EditResponse": spec["components"]["schemas"]["EditResponse"].clone(),
        "EditPostCheckResult": spec["components"]["schemas"]["EditPostCheckResult"].clone(),
        "ArtifactRequest": spec["components"]["schemas"]["ArtifactRequest"].clone(),
        "ArtifactResponse": spec["components"]["schemas"]["ArtifactResponse"].clone(),
        "GitRequest": spec["components"]["schemas"]["GitRequest"].clone(),
        "GitResponse": spec["components"]["schemas"]["GitResponse"].clone(),
        "ProjectHookRequest": spec["components"]["schemas"]["ProjectHookRequest"].clone(),
        "ProjectHookStep": spec["components"]["schemas"]["ProjectHookStep"].clone(),
        "ProjectHookResponse": spec["components"]["schemas"]["ProjectHookResponse"].clone(),
        "ProjectDoctorRequest": spec["components"]["schemas"]["ProjectDoctorRequest"].clone(),
        "ProjectDoctorAgentCapabilities": spec["components"]["schemas"]["ProjectDoctorAgentCapabilities"].clone(),
        "ProjectDoctorAgentInfo": spec["components"]["schemas"]["ProjectDoctorAgentInfo"].clone(),
        "ProjectDoctorGitInfo": spec["components"]["schemas"]["ProjectDoctorGitInfo"].clone(),
        "ProjectDoctorHooksInfo": spec["components"]["schemas"]["ProjectDoctorHooksInfo"].clone(),
        "ProjectDoctorRecentJob": spec["components"]["schemas"]["ProjectDoctorRecentJob"].clone(),
        "ProjectDoctorResponse": spec["components"]["schemas"]["ProjectDoctorResponse"].clone(),
        "ProjectWorkflowRequest": spec["components"]["schemas"]["ProjectWorkflowRequest"].clone(),
        "ProjectWorkflowGitSnapshot": spec["components"]["schemas"]["ProjectWorkflowGitSnapshot"].clone(),
        "ProjectWorkflowResponse": spec["components"]["schemas"]["ProjectWorkflowResponse"].clone(),
        "CommandRequest": spec["components"]["schemas"]["CommandRequest"].clone(),
        "CommandResponse": spec["components"]["schemas"]["CommandResponse"].clone(),
        "CommandRequestBatchItem": spec["components"]["schemas"]["CommandRequestBatchItem"].clone(),
        "CommandRequestOpRequest": spec["components"]["schemas"]["CommandRequestOpRequest"].clone(),
        "CommandRequestOpResponse": spec["components"]["schemas"]["CommandRequestOpResponse"].clone(),
        "JobOpRequest": spec["components"]["schemas"]["JobOpRequest"].clone(),
        "JobInfo": spec["components"]["schemas"]["JobInfo"].clone(),
        "JobOpResponse": spec["components"]["schemas"]["JobOpResponse"].clone(),
        "ProjectCapabilities": spec["components"]["schemas"]["ProjectCapabilities"].clone(),
        "ProjectCapabilityInfo": spec["components"]["schemas"]["ProjectCapabilityInfo"].clone(),
        "InstanceInfo": spec["components"]["schemas"]["InstanceInfo"].clone(),
        "ProjectsResponse": spec["components"]["schemas"]["ProjectsResponse"].clone(),
        "CheckRequest": spec["components"]["schemas"]["CheckRequest"].clone(),
        "CheckResponse": spec["components"]["schemas"]["CheckResponse"].clone(),
        "ReportRequest": spec["components"]["schemas"]["ReportRequest"].clone(),
        "ReportResponse": spec["components"]["schemas"]["ReportResponse"].clone(),
        "DesktopTask": spec["components"]["schemas"]["DesktopTask"].clone(),
        "DesktopTaskEvent": spec["components"]["schemas"]["DesktopTaskEvent"].clone(),
        "DesktopTaskOpRequest": spec["components"]["schemas"]["DesktopTaskOpRequest"].clone(),
        "DesktopTaskOpResponse": spec["components"]["schemas"]["DesktopTaskOpResponse"].clone(),
        "ActionSessionRecord": spec["components"]["schemas"]["ActionSessionRecord"].clone(),
        "ActionSessionStats": spec["components"]["schemas"]["ActionSessionStats"].clone(),
        "ActionEventView": spec["components"]["schemas"]["ActionEventView"].clone(),
        "ActionSessionListItem": spec["components"]["schemas"]["ActionSessionListItem"].clone(),
        "ActionSessionOpRequest": spec["components"]["schemas"]["ActionSessionOpRequest"].clone(),
        "ActionSessionOpResponse": spec["components"]["schemas"]["ActionSessionOpResponse"].clone()
    });
    apply_project_description_to_schema(
        &mut spec,
        &[
            "ContextBatchRequest",
            "EditRequest",
            "ArtifactRequest",
            "GitRequest",
            "ProjectDoctorRequest",
            "CommandRequest",
            "CommandRequestOpRequest",
            "JobOpRequest",
            "CheckRequest",
            "ReportRequest",
        ],
    );
    apply_edit_timeout_guidance(&mut spec);
    apply_job_recovery_guidance(&mut spec);
    apply_context_batch_guidance(&mut spec);
    apply_trusted_command_guidance(&mut spec);
    apply_action_session_openapi(&mut spec);
    apply_shell_client_openapi(&mut spec);
    spec["paths"] = serde_json::json!({
        "/api/codex/projects": spec["paths"]["/api/codex/projects"].clone(),
        "/api/codex/context_batch": spec["paths"]["/api/codex/context_batch"].clone(),
        "/api/codex/edit": spec["paths"]["/api/codex/edit"].clone(),
        "/api/codex/artifact": spec["paths"]["/api/codex/artifact"].clone(),
        "/api/codex/git": spec["paths"]["/api/codex/git"].clone(),
        "/api/codex/command": spec["paths"]["/api/codex/command"].clone(),
        "/api/codex/command_request_op": spec["paths"]["/api/codex/command_request_op"].clone(),
        "/api/codex/job": spec["paths"]["/api/codex/job"].clone(),
        "/api/codex/check": spec["paths"]["/api/codex/check"].clone(),
        "/api/codex/report": spec["paths"]["/api/codex/report"].clone(),
        "/api/codex/action_sessions": spec["paths"]["/api/codex/action_sessions"].clone(),
        "/api/shell/clients": spec["paths"]["/api/shell/clients"].clone(),
        "/api/shell/projects": spec["paths"]["/api/shell/projects"].clone(),
        "/api/shell/projects/create": spec["paths"]["/api/shell/projects/create"].clone(),
        "/api/shell/run": spec["paths"]["/api/shell/run"].clone(),
        "/api/shell/file": spec["paths"]["/api/shell/file"].clone()
    });
    spec["components"]["schemas"]["ReportRequest"]["properties"]["channel"]["description"] =
        serde_json::json!("Report channel; not the project field.");
    res.render(Json(spec));
}

#[handler]
pub async fn codex_openapi_gpt_json(res: &mut Response) {
    let mut spec =
        match serde_json::from_str::<serde_json::Value>(include_str!("../data/openapi.json")) {
            Ok(spec) => spec,
            Err(e) => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &e.to_string(),
                ));
                return;
            }
        };
    spec["openapi"] = serde_json::json!("3.1.0");
    spec["servers"] = serde_json::json!([{ "url": public_url(), "description": "Public server" }]);
    spec["info"] = serde_json::json!({"title":"Private Drop GPT Codex API","version":env!("CARGO_PKG_VERSION"),"description":"Slim Codex API for online GPT Actions. Keeps core project workflow and shell project bootstrap operations."});
    apply_shell_client_openapi(&mut spec);
    spec["paths"] = serde_json::json!({
        "/api/codex/projects": spec["paths"]["/api/codex/projects"].clone(),
        "/api/codex/context_batch": spec["paths"]["/api/codex/context_batch"].clone(),
        "/api/codex/edit": spec["paths"]["/api/codex/edit"].clone(),
        "/api/codex/artifact": spec["paths"]["/api/codex/artifact"].clone(),
        "/api/codex/git": spec["paths"]["/api/codex/git"].clone(),
        "/api/codex/project_doctor": spec["paths"]["/api/codex/project_doctor"].clone(),
        "/api/codex/project_workflow": spec["paths"]["/api/codex/project_workflow"].clone(),
        "/api/codex/command_request_op": spec["paths"]["/api/codex/command_request_op"].clone(),
        "/api/codex/job": spec["paths"]["/api/codex/job"].clone(),
        "/api/codex/check": spec["paths"]["/api/codex/check"].clone(),
        "/api/codex/report": spec["paths"]["/api/codex/report"].clone(),
        "/api/codex/action_sessions": spec["paths"]["/api/codex/action_sessions"].clone(),
        "/api/shell/clients": spec["paths"]["/api/shell/clients"].clone(),
        "/api/shell/projects": spec["paths"]["/api/shell/projects"].clone(),
        "/api/shell/projects/create": spec["paths"]["/api/shell/projects/create"].clone(),
        "/api/shell/run": spec["paths"]["/api/shell/run"].clone()
    });
    spec["components"]["schemas"] = serde_json::json!({
        "ContextResponse": spec["components"]["schemas"]["ContextResponse"].clone(),
        "ContextBatchItem": spec["components"]["schemas"]["ContextBatchItem"].clone(),
        "ContextBatchRequest": spec["components"]["schemas"]["ContextBatchRequest"].clone(),
        "ContextBatchResultMetadata": spec["components"]["schemas"]["ContextBatchResultMetadata"].clone(),
        "ContextBatchResponse": spec["components"]["schemas"]["ContextBatchResponse"].clone(),
        "ReplaceTextEdit": spec["components"]["schemas"]["ReplaceTextEdit"].clone(),
        "ReplaceRangeEdit": spec["components"]["schemas"]["ReplaceRangeEdit"].clone(),
        "AppendFileEdit": spec["components"]["schemas"]["AppendFileEdit"].clone(),
        "CreateFileEdit": spec["components"]["schemas"]["CreateFileEdit"].clone(),
        "WriteFileEdit": spec["components"]["schemas"]["WriteFileEdit"].clone(),
        "CreateBinaryFileEdit": spec["components"]["schemas"]["CreateBinaryFileEdit"].clone(),
        "WriteBinaryFileEdit": spec["components"]["schemas"]["WriteBinaryFileEdit"].clone(),
        "CreateBinaryArtifactEdit": spec["components"]["schemas"]["CreateBinaryArtifactEdit"].clone(),
        "WriteBinaryArtifactEdit": spec["components"]["schemas"]["WriteBinaryArtifactEdit"].clone(),
        "CreateBinaryFileFromUploadEdit": spec["components"]["schemas"]["CreateBinaryFileFromUploadEdit"].clone(),
        "WriteBinaryFileFromUploadEdit": spec["components"]["schemas"]["WriteBinaryFileFromUploadEdit"].clone(),
        "CreateBinaryFileFromUrlEdit": spec["components"]["schemas"]["CreateBinaryFileFromUrlEdit"].clone(),
        "WriteBinaryFileFromUrlEdit": spec["components"]["schemas"]["WriteBinaryFileFromUrlEdit"].clone(),
        "EditRequest": spec["components"]["schemas"]["EditRequest"].clone(),
        "EditResponse": spec["components"]["schemas"]["EditResponse"].clone(),
        "EditPostCheckResult": spec["components"]["schemas"]["EditPostCheckResult"].clone(),
        "ArtifactRequest": spec["components"]["schemas"]["ArtifactRequest"].clone(),
        "ArtifactResponse": spec["components"]["schemas"]["ArtifactResponse"].clone(),
        "GitRequest": spec["components"]["schemas"]["GitRequest"].clone(),
        "GitResponse": spec["components"]["schemas"]["GitResponse"].clone(),
        "ProjectHookRequest": spec["components"]["schemas"]["ProjectHookRequest"].clone(),
        "ProjectHookStep": spec["components"]["schemas"]["ProjectHookStep"].clone(),
        "ProjectHookResponse": spec["components"]["schemas"]["ProjectHookResponse"].clone(),
        "ProjectDoctorRequest": spec["components"]["schemas"]["ProjectDoctorRequest"].clone(),
        "ProjectDoctorAgentCapabilities": spec["components"]["schemas"]["ProjectDoctorAgentCapabilities"].clone(),
        "ProjectDoctorAgentInfo": spec["components"]["schemas"]["ProjectDoctorAgentInfo"].clone(),
        "ProjectDoctorGitInfo": spec["components"]["schemas"]["ProjectDoctorGitInfo"].clone(),
        "ProjectDoctorHooksInfo": spec["components"]["schemas"]["ProjectDoctorHooksInfo"].clone(),
        "ProjectDoctorRecentJob": spec["components"]["schemas"]["ProjectDoctorRecentJob"].clone(),
        "ProjectDoctorResponse": spec["components"]["schemas"]["ProjectDoctorResponse"].clone(),
        "ProjectWorkflowRequest": spec["components"]["schemas"]["ProjectWorkflowRequest"].clone(),
        "ProjectWorkflowGitSnapshot": spec["components"]["schemas"]["ProjectWorkflowGitSnapshot"].clone(),
        "ProjectWorkflowResponse": spec["components"]["schemas"]["ProjectWorkflowResponse"].clone(),
        "CommandRequestBatchItem": spec["components"]["schemas"]["CommandRequestBatchItem"].clone(),
        "CommandRequestOpRequest": spec["components"]["schemas"]["CommandRequestOpRequest"].clone(),
        "CommandRequestOpResponse": spec["components"]["schemas"]["CommandRequestOpResponse"].clone(),
        "JobOpRequest": spec["components"]["schemas"]["JobOpRequest"].clone(),
        "JobInfo": spec["components"]["schemas"]["JobInfo"].clone(),
        "JobOpResponse": spec["components"]["schemas"]["JobOpResponse"].clone(),
        "ProjectCapabilities": spec["components"]["schemas"]["ProjectCapabilities"].clone(),
        "ProjectCapabilityInfo": spec["components"]["schemas"]["ProjectCapabilityInfo"].clone(),
        "InstanceInfo": spec["components"]["schemas"]["InstanceInfo"].clone(),
        "ProjectsResponse": spec["components"]["schemas"]["ProjectsResponse"].clone(),
        "CheckRequest": spec["components"]["schemas"]["CheckRequest"].clone(),
        "CheckResponse": spec["components"]["schemas"]["CheckResponse"].clone(),
        "ReportRequest": spec["components"]["schemas"]["ReportRequest"].clone(),
        "ReportResponse": spec["components"]["schemas"]["ReportResponse"].clone(),
        "ActionSessionRecord": spec["components"]["schemas"]["ActionSessionRecord"].clone(),
        "ActionSessionStats": spec["components"]["schemas"]["ActionSessionStats"].clone(),
        "ActionEventView": spec["components"]["schemas"]["ActionEventView"].clone(),
        "ActionSessionListItem": spec["components"]["schemas"]["ActionSessionListItem"].clone(),
        "ActionSessionOpRequest": spec["components"]["schemas"]["ActionSessionOpRequest"].clone(),
        "ActionSessionOpResponse": spec["components"]["schemas"]["ActionSessionOpResponse"].clone(),
        "ShellAgentProjectSummary": spec["components"]["schemas"]["ShellAgentProjectSummary"].clone(),
        "ShellClientCapabilities": spec["components"]["schemas"]["ShellClientCapabilities"].clone(),
        "ShellClientView": spec["components"]["schemas"]["ShellClientView"].clone(),
        "ShellClientsResponse": spec["components"]["schemas"]["ShellClientsResponse"].clone(),
        "ShellClientProjectsRequest": spec["components"]["schemas"]["ShellClientProjectsRequest"].clone(),
        "ShellClientProjectsResponse": spec["components"]["schemas"]["ShellClientProjectsResponse"].clone(),
        "ShellClientProjectCreateRequest": spec["components"]["schemas"]["ShellClientProjectCreateRequest"].clone(),
        "ShellClientProjectCreateResponse": spec["components"]["schemas"]["ShellClientProjectCreateResponse"].clone(),
        "ShellRunRequest": spec["components"]["schemas"]["ShellRunRequest"].clone(),
        "ShellRunResponse": spec["components"]["schemas"]["ShellRunResponse"].clone()
    });
    apply_project_description_to_schema(
        &mut spec,
        &[
            "ContextBatchRequest",
            "EditRequest",
            "ArtifactRequest",
            "GitRequest",
            "ProjectDoctorRequest",
            "CommandRequestOpRequest",
            "JobOpRequest",
            "CheckRequest",
            "ReportRequest",
        ],
    );
    apply_edit_timeout_guidance(&mut spec);
    apply_job_recovery_guidance(&mut spec);
    apply_context_batch_guidance(&mut spec);
    apply_trusted_command_guidance(&mut spec);
    apply_action_session_openapi(&mut spec);
    apply_shell_client_openapi(&mut spec);
    spec["components"]["schemas"]["ReportRequest"]["properties"]["channel"]["description"] =
        serde_json::json!("Report channel; not the project field.");
    res.render(Json(spec));
}
