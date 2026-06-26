use salvo::prelude::*;
use serde_json::{json, Value};

fn public_url() -> String {
    std::env::var("DROP_PUBLIC_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://localhost:8080".to_string())
}

/// The exact, ordered set of GPT Actions operation ids exposed by
/// `/openapi.json`. Tests assert this set matches the generated schema.
///
/// Order is grouped by recommended GPT call flow:
/// 1. discovery (`listRuntimeTools`, `listProjects`, `getRuntimeStatus`)
/// 2. code tasks (`runCodexTask`, `getRuntimeJobStatus`, `getRuntimeJobLog`)
/// 3. project inspection (`readProjectFile`, `getProjectGitStatus`,
///    `getProjectGitDiff`, `getProjectGitDiffSummary`, `listProjectFiles`,
///    `searchProjectText`)
/// 4. project mutation (`validateProjectPatch`, `applyProjectPatch`,
///    `applyProjectPatchChecked`, `runProjectShellCommand`,
///    `deleteProjectFiles`, `gitRestorePaths`, `discardUntrackedFiles`,
///    `replaceProjectFileText`)
/// 5. job inspection (`listRuntimeJobs`, `getRuntimeJobTail`)
/// 6. advanced/generic entry point (`callRuntimeTool`)
///
/// Phase 3 promotes the core runtime tools to dedicated GPT Actions, and Phase
/// 5 promotes the safer structured text replacement action, so a custom GPT can
/// drive the coding loop without `callRuntimeTool` for common edits. Codex is
/// an optional advanced capability. `callRuntimeTool` remains as an advanced
/// escape hatch; prefer the dedicated typed actions.
#[cfg(test)]
const GPT_ACTION_OPS: &[&str] = &[
    "listRuntimeTools",
    "listProjects",
    "getRuntimeStatus",
    "runCodexTask",
    "getRuntimeJobStatus",
    "getRuntimeJobLog",
    "readProjectFile",
    "getProjectGitStatus",
    "getProjectGitDiff",
    "getProjectGitDiffSummary",
    "listProjectFiles",
    "searchProjectText",
    "validateProjectPatch",
    "applyProjectPatch",
    "applyProjectPatchChecked",
    "runProjectShellCommand",
    "deleteProjectFiles",
    "gitRestorePaths",
    "discardUntrackedFiles",
    "replaceProjectFileText",
    "listRuntimeJobs",
    "getRuntimeJobTail",
    "callRuntimeTool",
];

/// Legacy and non-GPT-Actions paths that must never appear in
/// `/openapi.json`. The GPT Actions surface is intentionally small and
/// POST-only; raw shell, file transfer, desktop, and the old codex
/// command/context endpoints belong to other internal routers, not to
/// the GPT-importable schema.
#[cfg(test)]
const LEGACY_FORBIDDEN_PATHS: &[&str] = &[
    "/api/messages",
    "/api/files",
    "/api/desktop/task_op",
    "/api/desktop/task",
    "/api/codex/command_request_op",
    "/api/codex/command_request",
    "/api/codex/context",
    "/api/codex/context_batch",
    "/api/codex/apply_patch",
    "/api/codex/edit",
    "/api/codex/artifact",
    "/api/codex/git",
    "/api/codex/job",
    "/api/codex/report",
    "/api/codex/projects",
    "/api/shell/run",
    "/api/shell/job",
    "/api/shell/file",
    "/api/shell/jobs/status",
    "/api/shell/jobs/log",
    "/api/shell/jobs/stop",
    "/api/jobs/stop",
    "/api/shell/jobs/list",
    // Phase 5: `replace_in_file` was promoted to a dedicated GPT Action
    // (`replaceProjectFileText`) so it is no longer forbidden here. `write_file`
    // remains a runtime-only thin REST wrapper over ToolRuntime (also reachable
    // via callRuntimeTool / MCP tools/call); it is intentionally NOT promoted
    // to a dedicated GPT Action because whole-file overwrite is riskier.
    "/api/projects/write_file",
    "/api/shell/agent/register",
    "/api/shell/agent/poll",
    "/api/shell/agent/result",
    "/api/shell/agent/job_update",
    "/api/audit/sessions",
    "/api/audit/session",
    "/api/audit/stats",
    "/mcp",
    "/openapi.json",
    // The MCP App console is a public static HTML/JS/CSS surface served via
    // GET; it is intentionally NOT a GPT Action and must never appear in the
    // POST-only /openapi.json schema.
    "/console",
    "/console/app.js",
    "/console/styles.css",
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
            "description": "Self-hosted tool runtime for ChatGPT. Recommended flow: call listProjects (or listRuntimeTools) to discover available projects, then runCodexTask to start a Codex CLI task, then getRuntimeJobStatus / getRuntimeJobLog to poll the returned job_id. Use readProjectFile and getProjectGitStatus for safe project inspection. callRuntimeTool is an advanced generic entry point for any runtime tool; prefer the dedicated actions when available. All endpoints require Bearer auth (DROP_TOKEN). MCP and GPT Actions share the same ToolRuntime."
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
                    "Returns the MCP-compatible tool list plus `names`, `count`, `categories`, and `recommended_flows`. Useful for discovering every tool name accepted by callRuntimeTool and their groupings. GPT Actions normally do not need this if the dedicated actions cover the task.",
                    "EmptyRequest",
                    "ToolsListResponse"
                )
            },
            "/api/projects/list": {
                "post": operation(
                    "listProjects",
                    "List agent-registered projects",
                    "Returns the list of projects registered by connected agents with their runtime id (`agent:<client_id>:<project_id>`), path, executor, client_id, and whether patching is allowed. Call this first to learn the project ids required by runCodexTask, readProjectFile, and getProjectGitStatus.",
                    "EmptyRequest",
                    "ToolResult"
                )
            },
            "/api/runtime/status": {
                "post": operation(
                    "getRuntimeStatus",
                    "Get runtime status",
                    "Read-only runtime health/observability summary with service metadata, registered agents, project counts, and job counts. Never exposes tokens, secrets, full env, or stdout/stderr. Call first when troubleshooting.",
                    "EmptyRequest",
                    "ToolResult"
                )
            },
            "/api/codex/run": {
                "post": operation_with_examples(
                    "runCodexTask",
                    "Run Codex CLI task",
                    "Recommended primary code action. Starts Codex CLI asynchronously in an agent-registered project and returns a job_id. Do not assemble raw shell to run Codex; poll with getRuntimeJobStatus and read output with getRuntimeJobLog.",
                    "CodexRunRequest",
                    "ToolResult",
                    json!({
                        "projectAndPrompt": {
                            "summary": "Start a Codex task in a project",
                            "value": {
                                "project": "private-drop",
                                "prompt": "Inspect the codebase and summarize the runtime architecture."
                            }
                        },
                        "withTimeout": {
                            "summary": "Start a Codex task with an explicit timeout",
                            "value": {
                                "project": "private-drop",
                                "prompt": "Run the test suite and report failures.",
                                "timeout_secs": 600
                            }
                        }
                    })
                )
            },
            "/api/jobs/status": {
                "post": operation_with_examples(
                    "getRuntimeJobStatus",
                    "Get job status",
                    "Returns status, timing, and exit metadata for a runtime job. Use this to poll the job_id returned by runCodexTask until status is completed, failed, stopped, or lost.",
                    "JobStatusRequest",
                    "ToolResult",
                    json!({
                        "byJobId": {
                            "summary": "Poll a job by id",
                            "value": {
                                "job_id": "11111111-2222-3333-4444-555555555555"
                            }
                        }
                    })
                )
            },
            "/api/jobs/log": {
                "post": operation_with_examples(
                    "getRuntimeJobLog",
                    "Get job log",
                    "Returns bounded stdout/stderr text for a runtime job. Use the job_id returned by runCodexTask. Output is always bounded; use tail_lines to limit the trailing stdout window and offset (next_stdout_line) for pagination.",
                    "JobLogRequest",
                    "ToolResult",
                    json!({
                        "byJobId": {
                            "summary": "Read the tail of a job log",
                            "value": {
                                "job_id": "11111111-2222-3333-4444-555555555555"
                            }
                        },
                        "withTailLines": {
                            "summary": "Read the last N stdout lines",
                            "value": {
                                "job_id": "11111111-2222-3333-4444-555555555555",
                                "tail_lines": 200
                            }
                        }
                    })
                )
            },
            "/api/jobs/list": {
                "post": operation_with_examples(
                    "listRuntimeJobs",
                    "List runtime jobs",
                    "Read-only bounded runtime job summaries across agent and local executors. Never returns stdout/stderr bodies — only metadata (job_id, kind, status, project, timestamps, exit_code). Optional `status` filter and `limit`.",
                    "ListJobsRequest",
                    "ToolResult",
                    json!({
                        "all": {
                            "summary": "List recent jobs",
                            "value": {}
                        },
                        "running": {
                            "summary": "List running jobs",
                            "value": {
                                "status": "running",
                                "limit": 20
                            }
                        }
                    })
                )
            },
            "/api/jobs/tail": {
                "post": operation_with_examples(
                    "getRuntimeJobTail",
                    "Get job tail",
                    "Read-only bounded stdout/stderr tails for a runtime job. Defaults to a bounded tail so the caller never reads full logs by default. Use the job_id returned by runCodexTask.",
                    "JobTailRequest",
                    "ToolResult",
                    json!({
                        "byJobId": {
                            "summary": "Read a bounded tail",
                            "value": {
                                "job_id": "11111111-2222-3333-4444-555555555555",
                                "tail_lines": 50
                            }
                        }
                    })
                )
            },
            "/api/projects/read_file": {
                "post": operation_with_examples(
                    "readProjectFile",
                    "Read a project file",
                    "Reads a UTF-8 file from an agent-registered project. Paths are resolved by the owning agent within that project. Output is bounded; use start_line and limit for pagination. This is the safe, dedicated alternative to callRuntimeTool for file inspection.",
                    "ReadProjectFileRequest",
                    "ToolResult",
                    json!({
                        "readme": {
                            "summary": "Read a project README",
                            "value": {
                                "project": "private-drop",
                                "path": "README.md"
                            }
                        },
                        "paginated": {
                            "summary": "Read a slice of a source file",
                            "value": {
                                "project": "private-drop",
                                "path": "src/main.rs",
                                "start_line": 1,
                                "limit": 100
                            }
                        }
                    })
                )
            },
            "/api/projects/git_status": {
                "post": operation_with_examples(
                    "getProjectGitStatus",
                    "Get project git status",
                    "Runs `git status --porcelain` in an agent-registered project and returns stdout, stderr, and exit_code. Safe read-only project inspection. Use this before proposing changes via runCodexTask.",
                    "ProjectIdRequest",
                    "ToolResult",
                    json!({
                        "byProject": {
                            "summary": "Check git status of a project",
                            "value": {
                                "project": "private-drop"
                            }
                        }
                    })
                )
            },
            "/api/projects/git_diff": {
                "post": operation_with_examples(
                    "getProjectGitDiff",
                    "Get project git diff",
                    "Runs `git diff` in an agent-registered project and returns stdout, stderr, and exit_code. Optional `args` scopes paths or adds flags (e.g. [\"--stat\"]). Read-only inspection; routes to the owning agent.",
                    "ProjectGitDiffRequest",
                    "ToolResult",
                    json!({
                        "byProject": {
                            "summary": "Full diff of a project",
                            "value": {
                                "project": "private-drop"
                            }
                        },
                        "withStat": {
                            "summary": "Diffstat of a project",
                            "value": {
                                "project": "private-drop",
                                "args": ["--stat"]
                            }
                        }
                    })
                )
            },
            "/api/projects/git_diff_summary": {
                "post": operation_with_examples(
                    "getProjectGitDiffSummary",
                    "Get project git diff summary",
                    "Read-only git diff summary for an agent-registered project: `git status --porcelain`, `git diff --stat`, and a parsed changed-file list. Does not modify the worktree. Routes to the owning agent.",
                    "ProjectIdRequest",
                    "ToolResult",
                    json!({
                        "byProject": {
                            "summary": "Diff summary of a project",
                            "value": {
                                "project": "private-drop"
                            }
                        }
                    })
                )
            },
            "/api/projects/list_files": {
                "post": operation_with_examples(
                    "listProjectFiles",
                    "List project files",
                    "Read-only bounded file listing of an agent-registered project directory. Returns project-relative paths plus a file/dir kind. Optional `path` scopes a subdirectory; `limit` bounds the entry count. Routes to the owning agent.",
                    "ListProjectFilesRequest",
                    "ToolResult",
                    json!({
                        "root": {
                            "summary": "List project root",
                            "value": {
                                "project": "private-drop"
                            }
                        },
                        "subdir": {
                            "summary": "List a subdirectory",
                            "value": {
                                "project": "private-drop",
                                "path": "src",
                                "limit": 100
                            }
                        }
                    })
                )
            },
            "/api/projects/search_text": {
                "post": operation_with_examples(
                    "searchProjectText",
                    "Search project text",
                    "Read-only bounded text search inside an agent-registered project. Each match carries a project-relative path, 1-based line number, and a preview line. Sensitive/build dirs (.git, target, node_modules) are excluded. Routes to the owning agent.",
                    "SearchProjectTextRequest",
                    "ToolResult",
                    json!({
                        "byPattern": {
                            "summary": "Search for a pattern",
                            "value": {
                                "project": "private-drop",
                                "pattern": "fn main",
                                "limit": 20
                            }
                        }
                    })
                )
            },
            "/api/projects/apply_patch": {
                "post": operation_with_examples(
                    "applyProjectPatch",
                    "Apply a patch to a project",
                    "Applies a unified diff patch to an agent-registered project through the owning agent. Mutation with side effects; requires Bearer auth and the agent shell capability. Prefer runCodexTask for exploratory edits.",
                    "ApplyPatchRequest",
                    "ToolResult",
                    json!({
                        "example": {
                            "summary": "Apply a small unified diff",
                            "value": {
                                "project": "private-drop",
                                "patch": "--- a/README.md\n+++ b/README.md\n@@ -1 +1,2 @@\n# Private Drop\n+edited\n"
                            }
                        }
                    })
                )
            },
            "/api/projects/run_shell": {
                "post": operation_with_examples(
                    "runProjectShellCommand",
                    "Run a shell command in a project",
                    "Runs a shell command in an agent-registered project through the owning agent and returns stdout, stderr, and exit_code. Executable with side effects; requires Bearer auth and the agent shell capability. Use for build/test/diagnostic commands.",
                    "RunShellRequest",
                    "ToolResult",
                    json!({
                        "tests": {
                            "summary": "Run the test suite",
                            "value": {
                                "project": "private-drop",
                                "command": "cargo test"
                            }
                        },
                        "withCwd": {
                            "summary": "Run a command in a subdirectory",
                            "value": {
                                "project": "private-drop",
                                "command": "ls",
                                "cwd": "src"
                            }
                        }
                    })
                )
            },
            "/api/projects/validate_patch": {
                "post": operation_with_examples(
                    "validateProjectPatch",
                    "Validate a project patch (dry-run)",
                    "Read-only dry-run patch preflight. Runs `git apply --check` and `git apply --stat` through the owning agent without modifying the worktree. Returns can_apply, affected_files, stat, and warnings. Never writes files.",
                    "ValidatePatchRequest",
                    "ToolResult",
                    json!({
                        "byProject": {
                            "summary": "Dry-run a small patch",
                            "value": {
                                "project": "private-drop",
                                "patch": "--- a/f.txt\n+++ b/f.txt\n@@ -1 +1,2 @@\nx\n+y\n"
                            }
                        }
                    })
                )
            },
            "/api/projects/apply_patch_checked": {
                "post": operation_with_examples(
                    "applyProjectPatchChecked",
                    "Apply a checked patch to a project",
                    "Mutation with side effects. Runs the validate_patch preflight first and, only when can_apply=true, applies the patch and returns the post-apply diff summary. Requires Bearer auth and the agent shell capability.",
                    "ApplyPatchCheckedRequest",
                    "ToolResult",
                    json!({
                        "byProject": {
                            "summary": "Validate then apply a small patch",
                            "value": {
                                "project": "private-drop",
                                "patch": "--- a/f.txt\n+++ b/f.txt\n@@ -1 +1,2 @@\nx\n+y\n"
                            }
                        }
                    })
                )
            },
            "/api/projects/delete_files": {
                "post": operation_with_examples(
                    "deleteProjectFiles",
                    "Delete project files",
                    "Mutation with side effects. Deletes selected project-relative files only (not directories). Safer than ad hoc rm. Requires Bearer auth and the agent shell capability.",
                    "DeleteProjectFilesRequest",
                    "ToolResult",
                    json!({
                        "byProject": {
                            "summary": "Delete selected files",
                            "value": {
                                "project": "private-drop",
                                "paths": ["tmp_probe.txt"]
                            }
                        }
                    })
                )
            },
            "/api/projects/git_restore_paths": {
                "post": operation_with_examples(
                    "gitRestorePaths",
                    "Restore tracked project paths",
                    "Mutation with side effects. Runs `git restore -- <paths>` on selected tracked project-relative paths. Does not remove untracked files. Requires Bearer auth and the agent shell capability.",
                    "GitRestorePathsRequest",
                    "ToolResult",
                    json!({
                        "byProject": {
                            "summary": "Restore selected tracked paths",
                            "value": {
                                "project": "private-drop",
                                "paths": ["tmp_probe.txt"]
                            }
                        }
                    })
                )
            },
            "/api/projects/discard_untracked": {
                "post": operation_with_examples(
                    "discardUntrackedFiles",
                    "Discard untracked project files",
                    "Mutation with side effects. Runs `git clean -f -- <paths>` only for selected project-relative untracked paths. Requires Bearer auth and the agent shell capability.",
                    "DiscardUntrackedRequest",
                    "ToolResult",
                    json!({
                        "byProject": {
                            "summary": "Discard selected untracked files",
                            "value": {
                                "project": "private-drop",
                                "paths": ["tmp_probe.txt"]
                            }
                        }
                    })
                )
            },
            "/api/projects/replace_in_file": {
                "post": operation_with_examples(
                    "replaceProjectFileText",
                    "Replace text in a project file",
                    "Mutation with side effects: modifies a project file by replacing a unique substring via the owning agent shell capability. Requires Bearer auth. Fails without writing when old is missing or ambiguous. Rejects sensitive paths.",
                    "ReplaceInFileRequest",
                    "ToolResult",
                    json!({
                        "byProject": {
                            "summary": "Replace a unique substring in a project file",
                            "value": {
                                "project": "private-drop",
                                "path": "src/main.rs",
                                "old": "fn main()",
                                "new": "fn main() -> Result<(), Box<dyn std::error::Error>>"
                            }
                        }
                    })
                )
            },
            "/api/tools/call": {
                "post": operation_with_examples(
                    "callRuntimeTool",
                    "Call runtime tool (advanced)",
                    "Advanced generic entry point for calling any runtime tool by name with params. Prefer dedicated actions when available. Use listRuntimeTools to discover accepted tool names.",
                    "ToolCallRequest",
                    "ToolResult",
                    json!({
                        "gitStatus": {
                            "summary": "Call git_status via the generic entry point",
                            "value": {
                                "tool": "git_status",
                                "params": {
                                    "project": "private-drop"
                                }
                            }
                        },
                        "readFile": {
                            "summary": "Call read_file via the generic entry point",
                            "value": {
                                "tool": "read_file",
                                "params": {
                                    "project": "private-drop",
                                    "path": "README.md"
                                }
                            }
                        },
                        "argumentsAlias": {
                            "summary": "MCP-style arguments alias (params wins when both present)",
                            "value": {
                                "tool": "git_diff_summary",
                                "arguments": {
                                    "project": "private-drop"
                                }
                            }
                        },
                        "noParams": {
                            "summary": "Argument-less tool; omit params",
                            "value": {
                                "tool": "list_tools"
                            }
                        }
                    })
                )
            }
        },
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "Bearer token. Set DROP_TOKEN on the server and send Authorization: Bearer <DROP_TOKEN>."
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
    operation_with_examples(
        operation_id,
        summary,
        description,
        request_schema,
        response_schema,
        Value::Null,
    )
}

fn operation_with_examples(
    operation_id: &str,
    summary: &str,
    description: &str,
    request_schema: &str,
    response_schema: &str,
    examples: Value,
) -> Value {
    let mut media_type = json!({
        "schema": {
            "$ref": format!("#/components/schemas/{}", request_schema)
        }
    });
    if let Value::Object(examples_obj) = examples {
        if !examples_obj.is_empty() {
            media_type["examples"] = Value::Object(examples_obj);
        }
    }
    json!({
        "operationId": operation_id,
        "summary": summary,
        "description": description,
        "requestBody": {
            "required": true,
            "content": {
                "application/json": media_type
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
            "properties": {},
            "description": "Empty request body. Send {} for actions that take no arguments (listRuntimeTools, listProjects)."
        },
        "ToolCallRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["tool"],
            "description": "Generic runtime tool call. `tool` is the runtime tool name. `params` carries the tool-specific arguments object. `arguments` is accepted as a compatibility alias; when both `params` and `arguments` are present, `params` wins. Omit both (or send null) for argument-less tools like list_tools.",
            "properties": {
                "tool": {
                    "type": "string",
                    "description": "Runtime tool name. Common values: list_tools, list_projects, runtime_status, read_file, git_status, git_diff, git_diff_summary, validate_patch, apply_patch_checked, apply_patch, run_shell, run_job, run_codex, job_status, job_log, list_jobs, job_tail. Use listRuntimeTools for all names."
                },
                "params": {
                    "type": "object",
                    "description": "Tool-specific arguments object. Takes precedence over `arguments` when both are present. Omit or send {} for argument-less tools (list_tools, list_projects, list_agents).",
                    "nullable": true,
                    "additionalProperties": true
                },
                "arguments": {
                    "type": "object",
                    "description": "Compatibility alias for `params`. Used only when `params` is absent; ignored otherwise. Useful for MCP-style callers that send `arguments`.",
                    "nullable": true,
                    "additionalProperties": true
                }
            }
        },
        "CodexRunRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project", "prompt"],
            "description": "Start a Codex CLI task. `project` must be an agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`. `prompt` is the instruction passed to Codex CLI.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "prompt": {
                    "type": "string",
                    "description": "Instruction prompt passed to Codex CLI. Must be non-empty and within CODEX_MAX_PROMPT_BYTES."
                },
                "approval_mode": {
                    "type": "string",
                    "description": "Optional Codex approval mode. Empty/none/off/disabled omit --approval-mode (use this if the Codex CLI does not support the flag). Other values (e.g. full-auto, suggest) are passed via --approval-mode."
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Maximum runtime in seconds. Defaults to CODEX_DEFAULT_TIMEOUT_SECS (3600)."
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional project-relative working directory. The owning agent enforces its cwd policy."
                },
                "extra_args": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Optional additional Codex CLI arguments. Each entry must be present in CODEX_ALLOWED_EXTRA_ARGS (empty by default)."
                }
            }
        },
        "JobStatusRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["job_id"],
            "description": "Poll a runtime job by id.",
            "properties": {
                "job_id": {
                    "type": "string",
                    "description": "Runtime job id returned by runCodexTask or run_job."
                }
            }
        },
        "JobLogRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["job_id"],
            "description": "Read bounded stdout/stderr for a runtime job.",
            "properties": {
                "job_id": {
                    "type": "string",
                    "description": "Runtime job id returned by runCodexTask or run_job."
                },
                "offset": {
                    "type": "integer",
                    "description": "Optional 1-based stdout line cursor. Use the next_stdout_line value from a previous response for pagination."
                },
                "tail_lines": {
                    "type": "integer",
                    "description": "Optional number of trailing stdout lines to return. Logs are always bounded; large values are capped server-side."
                }
            }
        },
        "ReadProjectFileRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project", "path"],
            "description": "Read a UTF-8 file from an agent-registered project.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "path": {
                    "type": "string",
                    "description": "Project-relative file path. Absolute paths and traversal (..) are rejected."
                },
                "start_line": {
                    "type": "integer",
                    "description": "Optional 1-based line offset for pagination."
                },
                "limit": {
                    "type": "integer",
                    "description": "Optional maximum line count (bounded server-side)."
                }
            }
        },
        "ProjectIdRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project"],
            "description": "Identify a project by id.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                }
            }
        },
        "ProjectGitDiffRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project"],
            "description": "Run `git diff` in an agent-registered project. Optional `args` scopes paths or adds git diff flags.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional git diff arguments / path specs (e.g. [\"--stat\"] or [\"src/main.rs\"])."
                }
            }
        },
        "ApplyPatchRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project", "patch"],
            "description": "Apply a unified diff patch to an agent-registered project. Executable mutation; the owning agent must allow patching.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "patch": {
                    "type": "string",
                    "description": "Unified diff patch content. Applied by the owning agent."
                }
            }
        },
        "ValidatePatchRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project", "patch"],
            "description": "Dry-run a unified diff patch against an agent-registered project without applying it. Read-only preflight.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "patch": {
                    "type": "string",
                    "description": "Unified diff patch content to validate."
                },
                "deny_sensitive_paths": {
                    "type": "boolean",
                    "description": "Optional. When true, sensitive-path warnings become a hard policy block (can_apply=false)."
                }
            }
        },
        "ApplyPatchCheckedRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project", "patch"],
            "description": "Validate then apply a unified diff patch. Mutation with side effects; applies only when the preflight passes.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "patch": {
                    "type": "string",
                    "description": "Unified diff patch content. Applied by the owning agent when the preflight passes."
                },
                "deny_sensitive_paths": {
                    "type": "boolean",
                    "description": "Optional. When true, sensitive-path warnings block the apply."
                }
            }
        },
        "DeleteProjectFilesRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project", "paths"],
            "description": "Delete selected project-relative files only (not directories). Mutation with side effects.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Project-relative file paths to delete."
                }
            }
        },
        "GitRestorePathsRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project", "paths"],
            "description": "Restore selected tracked project-relative paths with git restore. Mutation with side effects.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Project-relative tracked paths to restore."
                }
            }
        },
        "DiscardUntrackedRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project", "paths"],
            "description": "Discard selected untracked project-relative files with git clean -f. Mutation with side effects.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Project-relative untracked paths to remove."
                }
            }
        },
        "ReplaceInFileRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project", "path", "old", "new"],
            "description": "Replace a unique substring in a project file. Mutation with side effects; routes to the owning agent. Fails without writing when `old` is missing or ambiguous.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "path": {
                    "type": "string",
                    "description": "Project-relative file path. Absolute paths and traversal (..) are rejected."
                },
                "old": {
                    "type": "string",
                    "description": "Non-empty substring to replace. The call fails without writing when it is missing or ambiguous (unless allow_multiple/expected_replacements permit more)."
                },
                "new": {
                    "type": "string",
                    "description": "Replacement string. May be empty to delete the match."
                },
                "expected_replacements": {
                    "type": "integer",
                    "description": "Optional expected number of replacements. Defaults to 1. The call fails if the actual count differs."
                },
                "allow_multiple": {
                    "type": "boolean",
                    "description": "Optional. When true, allows more than one replacement. Defaults to false."
                }
            }
        },
        "ListProjectFilesRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project"],
            "description": "List files in an agent-registered project directory. Read-only bounded listing.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "path": {
                    "type": "string",
                    "description": "Optional project-relative directory to list (default: project root)."
                },
                "limit": {
                    "type": "integer",
                    "description": "Optional maximum number of entries to return."
                }
            }
        },
        "SearchProjectTextRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project", "pattern"],
            "description": "Search text inside an agent-registered project. Read-only bounded matches.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "pattern": {
                    "type": "string",
                    "description": "Text pattern to search for."
                },
                "path": {
                    "type": "string",
                    "description": "Optional project-relative directory to scope the search (default: project root)."
                },
                "limit": {
                    "type": "integer",
                    "description": "Optional maximum number of matches to return."
                }
            }
        },
        "ListJobsRequest": {
            "type": "object",
            "additionalProperties": false,
            "description": "List bounded runtime job summaries. Read-only; never returns stdout/stderr bodies.",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Optional maximum number of job summaries to return."
                },
                "status": {
                    "type": "string",
                    "description": "Optional status filter (e.g. running, completed, failed)."
                }
            }
        },
        "JobTailRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["job_id"],
            "description": "Read bounded stdout/stderr tails for a runtime job. Read-only.",
            "properties": {
                "job_id": {
                    "type": "string",
                    "description": "Runtime job id returned by runCodexTask or run_job."
                },
                "tail_lines": {
                    "type": "integer",
                    "description": "Optional number of trailing lines to return per stream."
                }
            }
        },
        "RunShellRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project", "command"],
            "description": "Run a shell command in an agent-registered project. Executable with side effects; requires the agent shell capability.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "command": {
                    "type": "string",
                    "description": "Shell command to run in the project directory."
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional maximum runtime in seconds."
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional project-relative working directory. The owning agent enforces its cwd policy."
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
            "required": ["success", "tools", "names", "count"],
            "description": "Runtime tool list. `tools` is the full MCP-compatible ToolSpec list (back-compat). `names` is just the tool name strings, `count` is the tool count, `categories` groups tools by family, and `recommended_flows` lists short GPT flow hints.",
            "properties": {
                "success": { "type": "boolean" },
                "tools": {
                    "type": "array",
                    "items": { "$ref": "#/components/schemas/ToolSpec" }
                },
                "names": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Accepted runtime tool names, in spec order."
                },
                "count": {
                    "type": "integer",
                    "description": "Number of tools in `tools`/`names`."
                },
                "categories": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "description": "Optional grouping by family: inspect, git, patch, shell, jobs, runtime, cleanup. A tool may appear in more than one category."
                },
                "recommended_flows": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional short GPT flow hints for common tool sequences."
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

    /// Collect all operation ids in the spec (sorted, deduplicated).
    fn operation_ids(spec: &Value) -> Vec<String> {
        let mut ids = Vec::new();
        for methods in spec["paths"].as_object().unwrap().values() {
            for op in methods.as_object().unwrap().values() {
                ids.push(op["operationId"].as_str().unwrap().to_string());
            }
        }
        ids.sort();
        ids
    }

    #[test]
    fn openapi_operation_ids_are_minimal() {
        let spec = build_openapi_spec();
        let ids = operation_ids(&spec);
        let mut expected = GPT_ACTION_OPS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        expected.sort();
        assert_eq!(ids, expected);
    }

    #[test]
    fn openapi_operation_ids_match_expected_set_exactly() {
        let spec = build_openapi_spec();
        let ids = operation_ids(&spec);
        let expected: Vec<String> = GPT_ACTION_OPS.iter().map(|s| s.to_string()).collect();
        assert_eq!(ids.len(), expected.len());
        for id in &expected {
            assert!(ids.contains(id), "missing operation id: {}", id);
        }
    }

    #[test]
    fn openapi_does_not_expose_any_legacy_or_non_gpt_action_paths() {
        let spec = build_openapi_spec();
        let paths = spec["paths"].as_object().unwrap();
        for legacy in LEGACY_FORBIDDEN_PATHS {
            assert!(
                !paths.contains_key(*legacy),
                "legacy/non-GPT-Actions path '{}' must not appear in openapi.json",
                legacy
            );
        }
    }

    #[test]
    fn openapi_phase3_exposes_validate_patch_as_dedicated_action() {
        // Phase 3: validate_patch is now promoted to a dedicated GPT Action
        // (validateProjectPatch) so a custom GPT can dry-run patches without
        // callRuntimeTool. It is a read-only preflight that never modifies the
        // worktree.
        let spec = build_openapi_spec();
        let paths = spec["paths"].as_object().unwrap();
        assert!(
            paths.contains_key("/api/projects/validate_patch"),
            "validate_patch must now appear in /openapi.json as a dedicated read-only action"
        );
        assert_eq!(
            spec["paths"]["/api/projects/validate_patch"]["post"]["operationId"],
            "validateProjectPatch"
        );
        let desc = spec["paths"]["/api/projects/validate_patch"]["post"]["description"]
            .as_str()
            .unwrap();
        assert!(
            desc.to_lowercase().contains("read-only"),
            "validateProjectPatch description must be marked read-only: {}",
            desc
        );
    }

    #[test]
    fn openapi_exposes_expected_gpt_action_paths() {
        let spec = build_openapi_spec();
        let paths = spec["paths"].as_object().unwrap();
        for expected in [
            "/api/tools/list",
            "/api/projects/list",
            "/api/runtime/status",
            "/api/codex/run",
            "/api/jobs/status",
            "/api/jobs/log",
            "/api/jobs/list",
            "/api/jobs/tail",
            "/api/projects/read_file",
            "/api/projects/git_status",
            "/api/projects/git_diff",
            "/api/projects/git_diff_summary",
            "/api/projects/list_files",
            "/api/projects/search_text",
            "/api/projects/validate_patch",
            "/api/projects/apply_patch",
            "/api/projects/apply_patch_checked",
            "/api/projects/run_shell",
            "/api/projects/delete_files",
            "/api/projects/git_restore_paths",
            "/api/projects/discard_untracked",
            "/api/projects/replace_in_file",
            "/api/tools/call",
        ] {
            assert!(
                paths.contains_key(expected),
                "expected GPT Actions path '{}' missing from openapi.json",
                expected
            );
        }
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
    fn openapi_top_level_security_uses_bearer() {
        let spec = build_openapi_spec();
        let security = spec["security"].as_array().expect("security array");
        assert!(!security.is_empty());
        assert!(security[0]["bearerAuth"].is_array());
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

    #[test]
    fn openapi_paths_only_use_post_method() {
        // GPT Actions surface is POST-only. /openapi.json itself is served by
        // a separate GET route and must NOT appear inside the schema paths.
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
    fn openapi_has_no_duplicate_operation_ids() {
        let spec = build_openapi_spec();
        let mut ids = Vec::new();
        for methods in spec["paths"].as_object().unwrap().values() {
            for op in methods.as_object().unwrap().values() {
                ids.push(op["operationId"].as_str().unwrap().to_string());
            }
        }
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            ids.len(),
            sorted.len(),
            "duplicate operation ids detected: {:?}",
            ids
        );
    }

    #[test]
    fn openapi_operation_descriptions_fit_chatgpt_limit() {
        let spec = build_openapi_spec();
        for (path, methods) in spec["paths"].as_object().unwrap() {
            for (method, op) in methods.as_object().unwrap() {
                let operation_id = op["operationId"].as_str().unwrap_or("<missing>");
                let desc = op["description"].as_str().unwrap_or("");
                assert!(
                    desc.chars().count() <= 300,
                    "{} {} operationId {} description has length {}",
                    method,
                    path,
                    operation_id,
                    desc.chars().count()
                );
            }
        }
    }

    #[test]
    fn openapi_recommended_actions_have_run_codex_first_guidance() {
        let spec = build_openapi_spec();
        // runCodexTask description should recommend it as the primary action.
        let run_codex = &spec["paths"]["/api/codex/run"]["post"]["description"]
            .as_str()
            .unwrap();
        assert!(
            run_codex.contains("Recommended"),
            "runCodexTask description should mark it as recommended"
        );
        // callRuntimeTool should be marked advanced/generic.
        let call_tool = &spec["paths"]["/api/tools/call"]["post"]["description"]
            .as_str()
            .unwrap();
        assert!(
            call_tool.contains("Advanced"),
            "callRuntimeTool description should mark it as advanced"
        );
        // getRuntimeJobStatus / getRuntimeJobLog should mention job_id polling.
        let status_desc = &spec["paths"]["/api/jobs/status"]["post"]["description"]
            .as_str()
            .unwrap();
        assert!(status_desc.contains("job_id"));
        let log_desc = &spec["paths"]["/api/jobs/log"]["post"]["description"]
            .as_str()
            .unwrap();
        assert!(log_desc.contains("job_id"));
    }

    #[test]
    fn openapi_call_runtime_tool_lists_accepted_tool_names() {
        let spec = build_openapi_spec();
        let tool_desc = &spec["components"]["schemas"]["ToolCallRequest"]["properties"]["tool"]
            ["description"]
            .as_str()
            .unwrap();
        for name in [
            "git_status",
            "read_file",
            "run_codex",
            "job_status",
            "job_log",
        ] {
            assert!(
                tool_desc.contains(name),
                "ToolCallRequest.tool description should list accepted tool name '{}'",
                name
            );
        }
    }

    #[test]
    fn openapi_key_actions_have_examples() {
        let spec = build_openapi_spec();
        // runCodexTask, getRuntimeJobStatus, getRuntimeJobLog, and
        // callRuntimeTool must ship with at least one request example so GPT
        // has a concrete template to follow. Phase 3 dedicated actions are
        // also required to carry examples.
        for (path, label) in [
            ("/api/codex/run", "runCodexTask"),
            ("/api/jobs/status", "getRuntimeJobStatus"),
            ("/api/jobs/log", "getRuntimeJobLog"),
            ("/api/projects/read_file", "readProjectFile"),
            ("/api/projects/git_status", "getProjectGitStatus"),
            ("/api/projects/git_diff", "getProjectGitDiff"),
            ("/api/projects/git_diff_summary", "getProjectGitDiffSummary"),
            ("/api/projects/list_files", "listProjectFiles"),
            ("/api/projects/search_text", "searchProjectText"),
            ("/api/projects/validate_patch", "validateProjectPatch"),
            ("/api/projects/apply_patch", "applyProjectPatch"),
            (
                "/api/projects/apply_patch_checked",
                "applyProjectPatchChecked",
            ),
            ("/api/projects/run_shell", "runProjectShellCommand"),
            ("/api/projects/delete_files", "deleteProjectFiles"),
            ("/api/projects/git_restore_paths", "gitRestorePaths"),
            ("/api/projects/discard_untracked", "discardUntrackedFiles"),
            ("/api/projects/replace_in_file", "replaceProjectFileText"),
            ("/api/jobs/list", "listRuntimeJobs"),
            ("/api/jobs/tail", "getRuntimeJobTail"),
            ("/api/tools/call", "callRuntimeTool"),
        ] {
            let examples = &spec["paths"][path]["post"]["requestBody"]["content"]
                ["application/json"]["examples"];
            assert!(
                examples.is_object(),
                "{} request should declare examples",
                label
            );
            assert!(
                !examples.as_object().unwrap().is_empty(),
                "{} request should declare at least one example",
                label
            );
        }
    }

    #[test]
    fn openapi_dedicated_actions_have_expected_routes_and_operation_ids() {
        let spec = build_openapi_spec();
        assert_eq!(
            spec["paths"]["/api/projects/list"]["post"]["operationId"],
            "listProjects"
        );
        assert_eq!(
            spec["paths"]["/api/projects/read_file"]["post"]["operationId"],
            "readProjectFile"
        );
        assert_eq!(
            spec["paths"]["/api/projects/git_status"]["post"]["operationId"],
            "getProjectGitStatus"
        );
        assert_eq!(
            spec["paths"]["/api/projects/git_diff"]["post"]["operationId"],
            "getProjectGitDiff"
        );
        assert_eq!(
            spec["paths"]["/api/projects/apply_patch"]["post"]["operationId"],
            "applyProjectPatch"
        );
        assert_eq!(
            spec["paths"]["/api/projects/run_shell"]["post"]["operationId"],
            "runProjectShellCommand"
        );
        // Phase 3 dedicated actions.
        assert_eq!(
            spec["paths"]["/api/projects/git_diff_summary"]["post"]["operationId"],
            "getProjectGitDiffSummary"
        );
        assert_eq!(
            spec["paths"]["/api/projects/list_files"]["post"]["operationId"],
            "listProjectFiles"
        );
        assert_eq!(
            spec["paths"]["/api/projects/search_text"]["post"]["operationId"],
            "searchProjectText"
        );
        assert_eq!(
            spec["paths"]["/api/projects/validate_patch"]["post"]["operationId"],
            "validateProjectPatch"
        );
        assert_eq!(
            spec["paths"]["/api/projects/apply_patch_checked"]["post"]["operationId"],
            "applyProjectPatchChecked"
        );
        assert_eq!(
            spec["paths"]["/api/projects/delete_files"]["post"]["operationId"],
            "deleteProjectFiles"
        );
        assert_eq!(
            spec["paths"]["/api/projects/git_restore_paths"]["post"]["operationId"],
            "gitRestorePaths"
        );
        assert_eq!(
            spec["paths"]["/api/projects/discard_untracked"]["post"]["operationId"],
            "discardUntrackedFiles"
        );
        assert_eq!(
            spec["paths"]["/api/projects/replace_in_file"]["post"]["operationId"],
            "replaceProjectFileText"
        );
        assert_eq!(
            spec["paths"]["/api/jobs/list"]["post"]["operationId"],
            "listRuntimeJobs"
        );
        assert_eq!(
            spec["paths"]["/api/jobs/tail"]["post"]["operationId"],
            "getRuntimeJobTail"
        );
    }

    #[test]
    fn openapi_mutation_actions_describe_execution_risk_and_auth() {
        // Phase 3 mutation actions (applyProjectPatch, applyProjectPatchChecked,
        // runProjectShellCommand, deleteProjectFiles, gitRestorePaths,
        // discardUntrackedFiles) are executable actions with side effects; their
        // descriptions must call out the execution risk/side effects and the
        // Bearer-auth requirement so GPT callers understand they are not
        // read-only inspection.
        let spec = build_openapi_spec();
        for path in [
            "/api/projects/apply_patch",
            "/api/projects/apply_patch_checked",
            "/api/projects/run_shell",
            "/api/projects/delete_files",
            "/api/projects/git_restore_paths",
            "/api/projects/discard_untracked",
            "/api/projects/replace_in_file",
        ] {
            let desc = spec["paths"][path]["post"]["description"]
                .as_str()
                .unwrap_or("");
            assert!(
                desc.to_lowercase().contains("side effect"),
                "{} description should mention side effects, got: {}",
                path,
                desc
            );
            assert!(
                desc.to_lowercase().contains("bearer auth"),
                "{} description should mention Bearer auth, got: {}",
                path,
                desc
            );
            assert!(
                desc.to_lowercase().contains("agent shell capability"),
                "{} description should mention the agent shell capability, got: {}",
                path,
                desc
            );
        }
    }

    #[test]
    fn openapi_readonly_actions_describe_readonly() {
        // Phase 3 read-only dedicated actions must mark themselves read-only in
        // their description so GPT callers can tell them apart from mutations.
        let spec = build_openapi_spec();
        for path in [
            "/api/projects/git_diff_summary",
            "/api/projects/list_files",
            "/api/projects/search_text",
            "/api/projects/validate_patch",
            "/api/jobs/list",
            "/api/jobs/tail",
        ] {
            let desc = spec["paths"][path]["post"]["description"]
                .as_str()
                .unwrap_or("");
            assert!(
                desc.to_lowercase().contains("read-only"),
                "{} description should be marked read-only, got: {}",
                path,
                desc
            );
        }
    }

    #[test]
    fn openapi_call_runtime_tool_params_is_explicit_object() {
        // callRuntimeTool's ToolCallRequest must declare `params` as a property
        // that is an OpenAPI 3.1 object accepting arbitrary tool arguments.
        // GPT Actions sometimes mishandles free-form object params, which is
        // why dedicated typed actions are preferred; this test pins the schema
        // so `params` stays present and object-typed for advanced callers.
        let spec = build_openapi_spec();
        let tool_call = &spec["components"]["schemas"]["ToolCallRequest"];
        let properties = tool_call["properties"].as_object().unwrap();
        assert!(
            properties.contains_key("params"),
            "ToolCallRequest must declare a `params` property"
        );
        let params = &properties["params"];
        assert_eq!(params["type"], "object", "params must be type object");
        assert_eq!(params["nullable"], true, "params must allow null");
        assert_eq!(
            params["additionalProperties"], true,
            "params must allow arbitrary object properties"
        );
        // `tool` remains required; `params` is optional (advanced callers may
        // omit it for argument-less tools).
        let required = tool_call["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "tool"));
    }

    #[test]
    fn openapi_call_runtime_tool_documents_arguments_alias() {
        // Phase 2: ToolCallRequest must document the `arguments` compatibility
        // alias and state that `params` wins. Both must be object-typed.
        let spec = build_openapi_spec();
        let properties = spec["components"]["schemas"]["ToolCallRequest"]["properties"]
            .as_object()
            .unwrap();
        assert!(
            properties.contains_key("arguments"),
            "ToolCallRequest must declare an `arguments` alias property"
        );
        let arguments = &properties["arguments"];
        assert_eq!(arguments["type"], "object", "arguments must be type object");
        assert_eq!(arguments["nullable"], true, "arguments must allow null");
        assert_eq!(
            arguments["additionalProperties"], true,
            "arguments must allow arbitrary object properties"
        );
        let desc_blob =
            serde_json::to_string(&spec["components"]["schemas"]["ToolCallRequest"]).unwrap();
        assert!(
            desc_blob.contains("params") && desc_blob.contains("precedence"),
            "ToolCallRequest description must document params precedence over arguments"
        );
    }

    #[test]
    fn openapi_tools_list_response_includes_names_count_categories_flows() {
        // Phase 2: ToolsListResponse must declare names/count (required) and
        // categories/recommended_flows (optional), while keeping `tools` for
        // backward compatibility.
        let spec = build_openapi_spec();
        let resp = &spec["components"]["schemas"]["ToolsListResponse"];
        let required = resp["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "tools"));
        assert!(required.iter().any(|v| v == "names"));
        assert!(required.iter().any(|v| v == "count"));
        let props = resp["properties"].as_object().unwrap();
        assert!(props.contains_key("tools"));
        assert!(props.contains_key("names"));
        assert!(props.contains_key("count"));
        assert!(props.contains_key("categories"));
        assert!(props.contains_key("recommended_flows"));
    }

    #[test]
    fn openapi_operation_count_is_twenty_three() {
        // Phase 3 promoted 10 core runtime tools to dedicated GPT Actions,
        // bringing the schema from 12 to 22 ops. Phase 5 promotes
        // replace_in_file to a dedicated GPT Action (replaceProjectFileText),
        // bringing the count to 23. The surface must stay <= 30. write_file
        // stays runtime-only (NOT a dedicated GPT Action).
        let spec = build_openapi_spec();
        let count: usize = spec["paths"]
            .as_object()
            .unwrap()
            .values()
            .map(|m| m.as_object().unwrap().len())
            .sum();
        assert_eq!(count, 23, "GPT Actions schema must be 23 operations");
        assert!(count <= 30, "GPT Actions schema must stay <= 30 operations");
    }

    #[test]
    fn openapi_replace_in_file_promoted_but_write_file_stays_runtime_only() {
        // Phase 5 promotes replace_in_file to a dedicated GPT Action
        // (replaceProjectFileText); write_file stays runtime-only (reachable
        // via callRuntimeTool / MCP) because whole-file overwrite is riskier.
        let spec = build_openapi_spec();
        let paths = spec["paths"].as_object().unwrap();
        assert!(
            paths.contains_key("/api/projects/replace_in_file"),
            "replace_in_file must now appear in /openapi.json as a dedicated mutation action"
        );
        assert_eq!(
            spec["paths"]["/api/projects/replace_in_file"]["post"]["operationId"],
            "replaceProjectFileText"
        );
        assert!(
            !paths.contains_key("/api/projects/write_file"),
            "write_file must stay runtime-only (not a dedicated GPT Action)"
        );
        // replace_in_file is no longer forbidden; write_file still is, so
        // future edits catch accidental promotion.
        assert!(
            !LEGACY_FORBIDDEN_PATHS.contains(&"/api/projects/replace_in_file"),
            "replace_in_file must be removed from the forbidden guard now that it is a dedicated action"
        );
        assert!(LEGACY_FORBIDDEN_PATHS.contains(&"/api/projects/write_file"));
    }

    #[test]
    fn openapi_call_runtime_tool_examples_cover_alias_and_no_params() {
        // Phase 2: callRuntimeTool examples should demonstrate the arguments
        // alias and the argument-less (params omitted) shapes so a custom GPT
        // has concrete templates for both.
        let spec = build_openapi_spec();
        let examples = &spec["paths"]["/api/tools/call"]["post"]["requestBody"]["content"]
            ["application/json"]["examples"];
        let keys: Vec<&str> = examples
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        assert!(
            keys.iter()
                .any(|k| examples[*k]["value"]["arguments"].is_object()),
            "callRuntimeTool examples should include an arguments-alias variant"
        );
        assert!(
            keys.iter().any(|k| {
                let v = &examples[*k]["value"];
                v["tool"].as_str() == Some("list_tools") && v.get("params").is_none()
            }),
            "callRuntimeTool examples should include an argument-less variant"
        );
    }

    #[test]
    fn openapi_spec_serializes_as_valid_json() {
        // Building the spec must not panic and must produce a JSON object with
        // the top-level OpenAPI 3.1 keys ChatGPT expects.
        let spec = build_openapi_spec();
        assert_eq!(spec["openapi"], "3.1.0");
        assert!(spec["info"]["title"].is_string());
        assert!(spec["info"]["version"].is_string());
        assert!(spec["servers"].is_array());
        assert!(spec["paths"].is_object());
        assert!(spec["components"]["schemas"].is_object());
        assert!(spec["security"].is_array());
    }

    #[test]
    fn openapi_exposes_get_runtime_status_action() {
        let spec = build_openapi_spec();
        assert_eq!(
            spec["paths"]["/api/runtime/status"]["post"]["operationId"],
            "getRuntimeStatus"
        );
        assert!(spec["paths"]["/api/runtime/status"]["post"]["description"]
            .as_str()
            .unwrap()
            .contains("observability"));
    }
}
