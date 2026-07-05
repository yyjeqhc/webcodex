use salvo::prelude::*;
use serde_json::{json, Value};

use crate::tool_runtime::sessions::{
    TOOL_ASSERTION_NAME_FIELD, TOOL_CALL_RECORDING_SESSION_ID_FIELD, TOOL_EXPECTED_FAILURE_FIELD,
    TOOL_EXPECTED_FAILURE_KIND_FIELD, TOOL_EXPECT_FAILURE_KIND_ALIAS_FIELD,
};
use crate::tool_runtime::{
    ALLOW_CROSS_PROJECT_SESSION_FIELD, TOOL_CALL_ARGUMENTS_FIELD, TOOL_CALL_PARAMS_FIELD,
    TOOL_CALL_TOOL_FIELD,
};

const PATCH_FIELD_DESCRIPTION: &str = "raw standard unified diff only. Do not include Codex apply_patch wrapper syntax, shell heredocs, \"*** Begin Patch\", \"*** Update File\", or \"*** End Patch\". The first non-empty line should be \"diff --git ...\", \"--- ...\", or another git-apply-compatible unified diff header.";
const SESSION_ID_FIELD_DESCRIPTION: &str = "Optional explicit wc_sess_* id returned by start_session. When provided, records this dedicated action in the session ledger and wins over any current-session binding.";
const FLATTENED_TOOL_ARG_DESCRIPTION: &str =
    "Flattened tool-specific argument. Used only when `params` and `arguments` are absent.";

fn flattened_tool_arg_schema(schema_type: &str) -> Value {
    json!({
        "type": schema_type,
        "description": FLATTENED_TOOL_ARG_DESCRIPTION
    })
}

fn flattened_string_array_tool_arg_schema() -> Value {
    json!({
        "type": "array",
        "items": {"type": "string"},
        "description": FLATTENED_TOOL_ARG_DESCRIPTION
    })
}

fn public_url() -> String {
    std::env::var("WEBCODEX_PUBLIC_URL")
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
/// 2. job inspection (`getRuntimeJobStatus`, `getRuntimeJobLog`)
/// 3. project inspection (`readProjectFile`, `getProjectGitStatus`,
///    `getProjectGitDiff`, `getProjectGitDiffSummary`, `listProjectFiles`,
///    `searchProjectText`)
/// 4. project mutation (`validateProjectPatch`, `applyProjectPatch`,
///    `applyProjectPatchChecked`, `runProjectShellCommand`,
///    `deleteProjectFiles`, `gitRestorePaths`, `discardUntrackedFiles`,
///    `startProjectShellJob`)
/// 5. job inspection (`listRuntimeJobs`, `getRuntimeJobTail`)
/// 6. advanced/generic entry point (`callRuntimeTool`)
///
/// The compatibility edit tools `replace_in_file` and `write_project_file`
/// remain runtime tools reachable through `callRuntimeTool`; dedicated GPT
/// Actions should prefer structured line edits, `apply_text_edits`, or
/// `apply_patch_checked`.
#[cfg(test)]
const GPT_ACTION_OPS: &[&str] = &[
    "listRuntimeTools",
    "listProjects",
    "registerProject",
    "createProject",
    "getRuntimeStatus",
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
    "importConversationFilesToProject",
    "startProjectShellJob",
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
    "/api/codex/run",
    "/api/shell/run",
    "/api/shell/job",
    "/api/shell/file",
    "/api/shell/jobs/status",
    "/api/shell/jobs/log",
    "/api/shell/jobs/stop",
    "/api/jobs/stop",
    "/api/shell/jobs/list",
    // Compatibility edit tools remain runtime-only. They are reachable through
    // callRuntimeTool / MCP tools/call, but must not be promoted to dedicated
    // GPT Actions.
    "/api/projects/replace_in_file",
    "/api/projects/write_file",
    "/api/shell/agent/register",
    "/api/shell/agent/poll",
    "/api/shell/agent/result",
    "/api/shell/agent/job_update",
    "/api/audit/sessions",
    "/api/audit/session",
    "/api/audit/stats",
    // Phase 2 multi-user auth: user/token management is REST-only admin/self
    // surface. Token creation is sensitive and must not be GPT-importable, so
    // these paths are deliberately excluded from /openapi.json.
    "/api/users/create",
    "/api/users/list",
    "/api/users/me",
    "/api/tokens/create",
    "/api/tokens/register_hash",
    "/api/tokens/list",
    "/api/tokens/revoke",
    // Phase 3 agent token management: same REST-only admin/self surface, also
    // excluded from GPT Actions. Agent tokens are bound to an owner and an
    // allowed_client_id and are only used by the webcodex-agent transport.
    "/api/agent-tokens/create",
    "/api/agent-tokens/register_hash",
    "/api/agent-tokens/list",
    "/api/agent-tokens/revoke",
    // Pairing/enrollment creates temporary credentials and enrollment tokens.
    // It is REST-only for CLI/admin flows and must not be GPT-importable.
    "/api/pairing/create",
    "/api/pairing/enroll",
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

pub(crate) fn build_openapi_spec() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "WebCodex Runtime API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Self-hosted tool runtime for ChatGPT. Flow: call listProjects (or listRuntimeTools), inspect with readProjectFile/getProjectGitStatus/git diff tools, edit with structured file/patch actions, and validate with cargo/job tools. Codex delegation is hidden from GPT Actions for now; run Codex outside WebCodex if needed. All endpoints require Bearer auth; static bearer/API-key hosts may use a shared key for quick start or wc_pat_* for managed mode. MCP and GPT Actions share the same ToolRuntime."
        },
        "servers": [
            {
                "url": public_url(),
                "description": "WebCodex server"
            }
        ],
        "paths": {
            "/api/tools/list": {
                "post": operation(
                    "listRuntimeTools",
                    "List runtime tools",
                    "Read-only. Full detail returns MCP-compatible tool specs and can be too large for GPT Actions. Prefer callRuntimeTool with tool=tool_manifest for daily discovery; when using listRuntimeTools, pass summary_only=true plus category, features, or limit for bounded discovery.",
                    "ToolsListRequest",
                    "ToolsListResponse"
                )
            },
            "/api/projects/list": {
                "post": operation(
                    "listProjects",
                    "List agent-registered projects",
                    "Read-only. Returns projects registered by connected agents with runtime id (`agent:<client_id>:<project_id>`), path, executor, client_id, patch flag, and smoke-selection capabilities such as git_available and recommended_for_smoke. Call this first to learn the project ids required by other actions.",
                    "EmptyRequest",
                    "ToolResult"
                )
            },
            "/api/projects/register": {
                "post": operation_with_examples(
                    "registerProject",
                    "Register an existing project",
                    "Mutation with side effects. Registers an existing directory as a WebCodex project on the selected agent. Executes on the agent and is constrained by agent policy. Requires Bearer auth.",
                    "RegisterProjectRequest",
                    "ToolResult",
                    json!({
                        "basic": {
                            "summary": "Register an existing directory",
                            "value": {
                                "client_id": "oe",
                                "id": "my-project",
                                "name": "My Project",
                                "path": "/root/git/my-project",
                                "description": "Optional description",
                                "allow_patch": true,
                                "overwrite": false
                            }
                        }
                    })
                )
            },
            "/api/projects/create": {
                "post": operation_with_examples(
                    "createProject",
                    "Create and register a new project",
                    "Mutation with side effects. Creates a new directory on the selected agent and registers it as a WebCodex project. Executes on the agent and is constrained by agent policy. Requires Bearer auth.",
                    "CreateProjectRequest",
                    "ToolResult",
                    json!({
                        "basicTemplate": {
                            "summary": "Create a project with the basic template",
                            "value": {
                                "client_id": "oe",
                                "id": "hello",
                                "name": "Hello",
                                "path": "/root/git/hello",
                                "description": "A new project",
                                "allow_patch": true,
                                "template": "basic",
                                "git_init": true,
                                "allow_existing_empty": false,
                                "overwrite": false
                            }
                        },
                        "emptyTemplate": {
                            "summary": "Create an empty project",
                            "value": {
                                "client_id": "oe",
                                "id": "scratch",
                                "name": "Scratch",
                                "path": "/root/git/scratch"
                            }
                        }
                    })
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
            "/api/jobs/status": {
                "post": operation_with_examples(
                    "getRuntimeJobStatus",
                    "Get job status",
                    "Read-only. Returns status, timing, and exit metadata for a runtime job. Use this to poll the job_id returned by run_job until status is completed, failed, stopped, or lost.",
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
                    "Read-only. Returns bounded stdout/stderr text for a runtime job. Use the job_id returned by run_job. Output is always bounded; use tail_lines to limit the trailing stdout window and offset (next_stdout_line) for pagination.",
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
                    "Read-only bounded stdout/stderr tails for a runtime job. Defaults to a bounded tail so the caller never reads full logs by default. Use the job_id returned by run_job.",
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
                    "Read-only. Reads a UTF-8 file from an agent-registered project. Paths are resolved by the owning agent within that project. Output is bounded; use start_line and limit for pagination. Set with_line_numbers=true to include 1-based numbered_text and lines for edit tools.",
                    "ReadProjectFileRequest",
                    "ToolResult",
                    json!({
                        "readme": {
                            "summary": "Read a project README",
                            "value": {
                                "project": "webcodex",
                                "path": "README.md"
                            }
                        },
                        "paginated": {
                            "summary": "Read a slice of a source file",
                            "value": {
                                "project": "webcodex",
                                "path": "src/main.rs",
                                "start_line": 1,
                                "limit": 100,
                                "with_line_numbers": true
                            }
                        }
                    })
                )
            },
            "/api/projects/git_status": {
                "post": operation_with_examples(
                    "getProjectGitStatus",
                    "Get project git status",
                    "Runs `git status --porcelain` in an agent-registered project and returns stdout, stderr, and exit_code. Safe read-only project inspection; use before proposing changes or invoking mutation tools.",
                    "ProjectIdRequest",
                    "ToolResult",
                    json!({
                        "byProject": {
                            "summary": "Check git status of a project",
                            "value": {
                                "project": "webcodex"
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
                                "project": "webcodex"
                            }
                        },
                        "withStat": {
                            "summary": "Diffstat of a project",
                            "value": {
                                "project": "webcodex",
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
                                "project": "webcodex"
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
                                "project": "webcodex"
                            }
                        },
                        "subdir": {
                            "summary": "List a subdirectory",
                            "value": {
                                "project": "webcodex",
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
                    "Read-only bounded text search inside an agent-registered project. Each match carries a project-relative path, 1-based line number, and a preview line. Optional context_before/context_after add bounded 1-based context lines. Sensitive/build dirs (.git, target, node_modules) are excluded.",
                    "SearchProjectTextRequest",
                    "ToolResult",
                    json!({
                        "byPattern": {
                            "summary": "Search for a pattern",
                            "value": {
                                "project": "webcodex",
                                "pattern": "fn main",
                                "limit": 20,
                                "context_before": 2,
                                "context_after": 4
                            }
                        }
                    })
                )
            },
            "/api/projects/apply_patch": {
                "post": operation_with_examples(
                    "applyProjectPatch",
                    "Apply a patch to a project",
                    "Applies a unified diff patch to an agent-registered project through the owning agent. Mutation with side effects; requires Bearer auth and the agent shell capability. Use after inspecting files and validating the patch; for targeted edits prefer structured line edit tools via callRuntimeTool.",
                    "ApplyPatchRequest",
                    "ToolResult",
                    json!({
                        "example": {
                            "summary": "Apply a small unified diff",
                            "value": {
                                "project": "webcodex",
                                "patch": "--- a/README.md\n+++ b/README.md\n@@ -1 +1,2 @@\n# WebCodex\n+edited\n"
                            }
                        }
                    })
                )
            },
            "/api/projects/run_shell": {
                "post": operation_with_examples(
                    "runProjectShellCommand",
                    "Run a shell command in a project",
                    "Runs a shell command in an agent-registered project and returns stdout, stderr, exit_code plus command_started/command_ok/failure_kind/tool_failure. Executable with side effects; requires Bearer auth and agent shell capability.",
                    "RunShellRequest",
                    "ToolResult",
                    json!({
                        "tests": {
                            "summary": "Run the test suite",
                            "value": {
                                "project": "webcodex",
                                "command": "cargo test"
                            }
                        },
                        "withCwd": {
                            "summary": "Run a command in a subdirectory",
                            "value": {
                                "project": "webcodex",
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
                                "project": "webcodex",
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
                                "project": "webcodex",
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
                                "project": "webcodex",
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
                                "project": "webcodex",
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
                                "project": "webcodex",
                                "paths": ["tmp_probe.txt"]
                            }
                        }
                    })
                )
            },
            "/api/artifacts/import": {
                "post": operation_with_examples(
                    "importConversationFilesToProject",
                    "Import ChatGPT conversation files to a project",
                    "Mutation with side effects. Downloads GPT Actions openaiFileIdRefs immediately and saves bounded binary files into an agent-registered project. Populate openaiFileIdRefs from current conversation files generated by image generation, user upload, or Code Interpreter; never call with an empty array.",
                    "ImportConversationFilesRequest",
                    "ImportConversationFilesResponse",
                    json!({
                        "generatedImage": {
                            "summary": "Save a generated image into docs/assets",
                            "value": {
                                "project": "agent:oe:webcodex",
                                "output_dir": "docs/assets",
                                "overwrite": false,
                                "openaiFileIdRefs": [{
                                    "name": "generated.png",
                                    "id": "file_abc123",
                                    "mime_type": "image/png",
                                    "download_link": "https://files.oaiusercontent.com/example"
                                }]
                            }
                        }
                    })
                )
            },
            "/api/projects/run_job": {
                "post": operation_with_examples(
                    "startProjectShellJob",
                    "Start an async project shell job",
                    "Starts an async background shell job in an agent-registered project and returns a job_id. Execution with side effects; requires Bearer auth and the agent async shell job capability. Poll with getRuntimeJobStatus; read output with getRuntimeJobTail or getRuntimeJobLog.",
                    "StartProjectShellJobRequest",
                    "ToolResult",
                    json!({
                        "testCommand": {
                            "summary": "Run a lightweight test command asynchronously",
                            "value": {
                                "project": "webcodex",
                                "command": "cargo test --no-run"
                            }
                        },
                        "withTimeout": {
                            "summary": "Run a check command with a timeout",
                            "value": {
                                "project": "webcodex",
                                "command": "cargo clippy",
                                "timeout_secs": 300,
                                "cwd": "src"
                            }
                        }
                    })
                )
            },
            "/api/tools/call": {
                "post": operation_with_examples(
                    "callRuntimeTool",
                    "Call runtime tool (advanced)",
                    "Advanced generic escape hatch for runtime tools. Prefer dedicated actions. Use listRuntimeTools for names. GPT Actions should use flattened top-level fields; params/arguments remain for non-Action clients. Use recording_session_id for ledger recording; keep explicit ids for continuity.",
                    "ToolCallRequest",
                    "ToolResult",
                    json!({
                        "trackedSession": {
                            "summary": "Start a session with flattened GPT Action fields",
                            "value": {
                                "tool": "start_session",
                                "project": "webcodex",
                                "title": "implement show_changes follow-up",
                                "mode": "read_only"
                            }
                        },
                        "recordedGitStatus": {
                            "summary": "Record this wrapper call while passing flattened tool args",
                            "value": {
                                "tool": "git_status",
                                "project": "webcodex",
                                TOOL_CALL_RECORDING_SESSION_ID_FIELD: "wc_sess_example"
                            }
                        },
                        "sessionSummary": {
                            "summary": "Read a session summary with top-level business session_id",
                            "value": {
                                "tool": "session_summary",
                                "session_id": "wc_sess_example",
                                "limit": 20
                            }
                        },
                        "postSessionMessage": {
                            "summary": "Post session-local guidance while recording the wrapper call separately",
                            "value": {
                                "tool": "post_session_message",
                                "session_id": "wc_sess_business",
                                TOOL_CALL_RECORDING_SESSION_ID_FIELD: "wc_sess_recorder",
                                "kind": "guidance",
                                "message": "Keep new capabilities behind callRuntimeTool; do not add dedicated OpenAPI operations.",
                                "tags": ["openapi", "constraint"],
                                "priority": "normal"
                            }
                        },
                        "showChanges": {
                            "summary": "Summarize current worktree changes with optional session activity",
                            "value": {
                                "tool": "show_changes",
                                "project": "webcodex",
                                "session_id": "wc_sess_example",
                                "include_diff": false,
                                "session_event_limit": 30
                            }
                        },
                        "bindCurrentSession": {
                            "summary": "Bind an existing session as current in memory for a project",
                            "value": {
                                "tool": "bind_current_session",
                                "project": "webcodex",
                                "session_id": "wc_sess_example"
                            }
                        },
                        "readFile": {
                            "summary": "Call read_file via flattened GPT Action fields",
                            "value": {
                                "tool": "read_file",
                                "project": "webcodex",
                                "path": "README.md",
                                "with_line_numbers": true
                            }
                        },
                        "checkpointRestore": {
                            "summary": "Restore a checkpoint via flattened GPT Action fields",
                            "value": {
                                "tool": "workspace_checkpoint_restore",
                                "project": "webcodex",
                                "checkpoint_id": "wc_ckpt_abc",
                                "confirm": true,
                                TOOL_CALL_RECORDING_SESSION_ID_FIELD: "wc_sess_record"
                            }
                        },
                        "applyTextEdits": {
                            "summary": "Atomic multi-block edit via flattened GPT Action fields",
                            "value": {
                                "tool": "apply_text_edits",
                                "project": "webcodex",
                                "path": "src/lib.rs",
                                "dry_run": true,
                                "edits": [
                                    {"kind": "replace_exact", "old_text": "alpha", "new_text": "beta"}
                                ],
                                "expected_file_sha256": "sha256-of-original-file"
                            }
                        },
                        "argumentsAlias": {
                            "summary": "MCP-style arguments alias (params wins when both present)",
                            "value": {
                                "tool": "git_diff_summary",
                                "arguments": {
                                    "project": "webcodex"
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
                    "description": "Bearer token. Static bearer/API-key hosts may send a shared key for quick start or wc_pat_* for managed mode; WEBCODEX_TOKEN is the server bootstrap/admin credential."
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
        "x-openai-isConsequential": is_consequential_operation(operation_id),
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

fn is_consequential_operation(operation_id: &str) -> bool {
    match operation_id {
        "listRuntimeTools"
        | "listProjects"
        | "listAgents"
        | "getRuntimeStatus"
        | "readProjectFile"
        | "listProjectFiles"
        | "searchProjectText"
        | "getProjectGitStatus"
        | "getProjectGitDiff"
        | "getProjectGitDiffSummary"
        | "getProjectGitDiffHunks"
        | "getRuntimeJobStatus"
        | "getRuntimeJobLog"
        | "getRuntimeJobTail"
        | "listRuntimeJobs"
        | "validateProjectPatch"
        | "registerProject"
        | "createProject" => false,

        "applyProjectPatch"
        | "applyProjectPatchChecked"
        | "importConversationFilesToProject"
        | "runProjectShellCommand"
        | "startProjectShellJob"
        | "stopRuntimeJob"
        | "deleteProjectFiles"
        | "gitRestorePaths"
        | "discardUntracked"
        | "discardUntrackedFiles"
        | "callRuntimeTool" => true,

        other => panic!("missing consequential classification for operationId {other}"),
    }
}

fn schemas() -> Value {
    let mut schemas = json!({
        "EmptyRequest": {
            "type": "object",
            "additionalProperties": false,
            "properties": {},
            "description": "Empty request body. Send {} for actions that take no arguments."
        },
        "ToolsListRequest": {
            "type": "object",
            "additionalProperties": false,
            "description": "Optional bounded runtime tool discovery request. Omit fields for the legacy full detail list; GPT Actions should prefer summary_only=true with category, features, or limit.",
            "properties": {
                "category": {
                    "type": "string",
                    "description": "Optional tool_manifest category filter such as artifact, edit, session, git, validation, job, project, or runtime."
                },
                "features": {
                    "type": "string",
                    "description": "Optional loose feature filter such as artifact, artifact_upload, upload, read, edit, session, git, or validation."
                },
                "summary_only": {
                    "type": "boolean",
                    "description": "When true, return compact summaries without full input/output schemas."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 100,
                    "description": "Maximum returned tools for focused discovery. Runtime caps this at 100."
                }
            }
        },
        "OpenAiFileIdRef": {
            "type": "object",
            "additionalProperties": false,
            "required": ["download_link"],
            "description": "GPT Actions file reference. Field name openaiFileIdRefs must be used by the Action request so ChatGPT can pass conversation files.",
            "properties": {
                "name": {"type": "string"},
                "id": {"type": "string"},
                "mime_type": {"type": "string"},
                "download_link": {"type": "string", "description": "Temporary download URL; WebCodex downloads it immediately."}
            }
        },
        "ImportConversationFilesRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["openaiFileIdRefs", "project"],
            "description": "Import up to 10 GPT Actions conversation files into a project. Supports image/png, image/jpeg, image/webp, application/pdf, application/zip, text/plain, text/csv, application/json, and restricted application/octet-stream.",
            "properties": {
                "openaiFileIdRefs": {"type": "array", "maxItems": 10, "items": {"$ref": "#/components/schemas/OpenAiFileIdRef"}},
                "project": {"type": "string", "description": "Agent-registered runtime project id from listProjects."},
                "output_dir": {"type": "string", "description": "Optional project-relative output directory, for example docs/assets or artifacts/imports."},
                "targets": {"type": "array", "items": {"type": "string"}, "description": "Optional per-file output filenames."},
                "overwrite": {"type": "boolean", "description": "Allow overwriting existing files. Defaults to false."}
            }
        },
        "ImportConversationFilesResponse": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "success": {"type": "boolean"},
                "output": {
                    "type": "object",
                    "additionalProperties": true,
                    "properties": {
                        "count": {"type": "integer"},
                        "imported": {"type": "array", "items": {"type": "object", "additionalProperties": true}}
                    }
                },
                "error": {"type": "string", "nullable": true}
            }
        },
        "ToolCallRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": [TOOL_CALL_TOOL_FIELD],
            "description": "Generic runtime tool call. `tool` is the runtime tool name. GPT Actions should pass tool-specific arguments as flattened top-level fields because some Action runtimes reject free-form params/arguments objects. `params` and `arguments` remain accepted for non-Action clients, with `params` taking precedence. Top-level `session_id` is ordinary tool business input; use `recording_session_id` to record this wrapper call in the session ledger and enforce that recorder session's guards. When no explicit tool session_id is provided, project tools may use the caller/transport/project current session established by bind_current_session. That current-session binding is process-local in-memory control metadata, not the durable session ledger, and may be lost on restart. For reliable long-running or cross-client workflows, keep and pass explicit session_id or recording_session_id values. For daily discovery prefer tool_manifest; it exposes accepted_flattened_args for GPT Action top-level calls. Use list_tools with summary_only/category/features/limit only for focused discovery.",
            "properties": {
                ALLOW_CROSS_PROJECT_SESSION_FIELD: {
                    "type": "boolean",
                    "description": "Advanced/debug escape hatch for callRuntimeTool. When true, allow recording a project tool call into a session whose associated project differs from the request project; session_project_mismatch warning metadata is still returned. Used only when `params` and `arguments` are absent, or inside params/arguments for non-Action clients."
                },
                "session_id": {
                    "type": "string",
                    "description": "Flattened tool-specific argument. For session_summary and message-board tools this is the required business session id to read or update in the session ledger; for project tools it is the explicit tool session that wins over current-session binding. Use recording_session_id to record the wrapper call itself."
                },
                "kind": {
                    "type": "string",
                    "description": "Flattened tool-specific argument. For message-board tools, one of note, proposal, question, answer, decision, risk, progress, guidance, todo. For workspace_checkpoint_create, one of snapshot, baseline, before_refactor, after_refactor, last_known_good, rollback_candidate. Used only when `params` and `arguments` are absent."
                },
                "labels": {
                    "type": "array",
                    "items": {"type": "string", "maxLength": 64, "pattern": "^[A-Za-z0-9._-]+$"},
                    "maxItems": 20,
                    "description": "Flattened workspace_checkpoint_create labels. Used only when `params` and `arguments` are absent."
                },
                "validation": {
                    "type": "object",
                    "additionalProperties": false,
                    "description": "Flattened workspace_checkpoint_create validation metadata. The runtime records this metadata only and does not run commands.",
                    "properties": {
                        "status": {
                            "type": "string",
                            "enum": ["unknown", "not_run", "passed", "failed"]
                        },
                        "commands": {
                            "type": "array",
                            "items": {"type": "string", "maxLength": 200},
                            "maxItems": 20
                        },
                        "summary": {
                            "anyOf": [
                                {"type": "string"},
                                {"type": "null"}
                            ],
                            "maxLength": 500
                        }
                    }
                },
                "note": {
                    "type": "string",
                    "description": "Flattened workspace_checkpoint_create optional note (not used by restore). Used only when `params` and `arguments` are absent."
                },
                "include_untracked": {
                    "type": "boolean",
                    "description": "Flattened workspace_checkpoint_create flag to capture small non-secret UTF-8 untracked files (default false). Used only when `params` and `arguments` are absent."
                },
                "checkpoint_id": {
                    "type": "string",
                    "description": "Flattened workspace_checkpoint_show/restore/delete wc_ckpt_* id. Used only when `params` and `arguments` are absent."
                },
                "confirm": {
                    "type": "boolean",
                    "description": "Flattened confirmation flag for workspace_checkpoint_restore/delete and stop_job; must be true to proceed. Used only when `params` and `arguments` are absent."
                },
                "include_command_preview": {
                    "type": "boolean",
                    "description": "Flattened job_status debug flag. Defaults to false; when true, job_status includes bounded command_preview metadata. stdout/stderr bodies are never included. Used only when `params` and `arguments` are absent."
                },
                "include_diff_stat": {
                    "type": "boolean",
                    "description": "Flattened workspace_checkpoint_show flag to include tracked/staged diff stat strings (default false). Used only when `params` and `arguments` are absent."
                },
                "edits": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 20,
                    "description": "Flattened apply_text_edits ordered batch of 1..20 atomic edits. Used only when `params` and `arguments` are absent.",
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["kind"],
                        "properties": {
                            "kind": {
                                "type": "string",
                                "enum": ["replace_exact", "insert_after", "insert_before", "delete_exact"],
                                "description": "Atomic edit kind."
                            },
                            "old_text": {
                                "type": "string",
                                "description": "Exact text to replace or delete, required by replace_exact/delete_exact."
                            },
                            "new_text": {
                                "type": "string",
                                "description": "Replacement or inserted text, required by replace_exact/insert_before/insert_after."
                            },
                            "anchor_text": {
                                "type": "string",
                                "description": "Unique anchor text required by insert_before/insert_after."
                            }
                        }
                    }
                },
                "dry_run": {
                    "type": "boolean",
                    "description": "Flattened apply_text_edits / validate_patch flag to compute the plan without writing. Used only when `params` and `arguments` are absent."
                },
                "expected_file_sha256": {
                    "type": "string",
                    "description": "Flattened apply_text_edits optional sha256 guard for the whole original file. Used only when `params` and `arguments` are absent."
                },
                "message": {
                    "type": "string",
                    "description": "Flattened post_session_message body. Used only when `params` and `arguments` are absent."
                },
                "tags": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Flattened post_session_message tags. Used only when `params` and `arguments` are absent."
                },
                "reply_to": {
                    "type": "string",
                    "description": "Flattened post_session_message reply target wc_msg_* id. Used only when `params` and `arguments` are absent."
                },
                "priority": {
                    "type": "string",
                    "description": "Flattened post_session_message priority: low, normal, or high. Used only when `params` and `arguments` are absent."
                },
                "status": {
                    "type": "string",
                    "description": "Flattened list_session_messages status filter: open or resolved. Used only when `params` and `arguments` are absent."
                },
                "message_id": {
                    "type": "string",
                    "description": "Flattened resolve_session_message wc_msg_* id. Used only when `params` and `arguments` are absent."
                },
                "resolution": {
                    "type": "string",
                    "description": "Flattened resolve_session_message resolution note. Used only when `params` and `arguments` are absent."
                },
                "project": flattened_tool_arg_schema("string"),
                "title": flattened_tool_arg_schema("string"),
                "mode": flattened_tool_arg_schema("string"),
                "deny_write_tools": flattened_tool_arg_schema("boolean"),
                "deny_shell_tools": flattened_tool_arg_schema("boolean"),
                "include_runtime_status": {
                    "type": "boolean",
                    "description": "Flattened start_coding_task flag. Defaults to true. Used only when `params` and `arguments` are absent."
                },
                "include_git": {
                    "type": "boolean",
                    "description": "Flattened start_coding_task flag. Defaults to true. Used only when `params` and `arguments` are absent."
                },
                "include_recent_commits": {
                    "type": "boolean",
                    "description": "Flattened start_coding_task flag. Defaults to true. Used only when `params` and `arguments` are absent."
                },
                "include_rules": {
                    "type": "boolean",
                    "description": "Flattened start_coding_task flag. Defaults to true. Used only when `params` and `arguments` are absent."
                },
                "include_tool_manifest": {
                    "type": "boolean",
                    "description": "Flattened start_coding_task flag. Defaults to true and returns compact tool_manifest output without full schemas. Used only when `params` and `arguments` are absent."
                },
                "tool_manifest_categories": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Flattened start_coding_task compact manifest category filter. When include_tool_manifest=true, prefer a bounded set such as workflow, session, git, edit, artifact, and cleanup instead of all tools. Used only when `params` and `arguments` are absent."
                },
                "tool_manifest_limit": {
                    "type": "integer",
                    "description": "Flattened start_coding_task compact manifest entry limit; clamped by the runtime to 1..100. Used only when `params` and `arguments` are absent."
                },
                "bind_current": {
                    "type": "boolean",
                    "description": "Flattened start_coding_task flag. Defaults to false. Binding is process-local in-memory control metadata. Used only when `params` and `arguments` are absent."
                },
                "path": {
                    "type": "string",
                    "description": "Flattened tool-specific argument. For artifact_upload_chunk/finish/abort this is required and must exactly match the path used by artifact_upload_begin to bind upload_id to the target path. Used only when `params` and `arguments` are absent."
                },
                "command": flattened_tool_arg_schema("string"),
                "cwd": flattened_tool_arg_schema("string"),
                "timeout_secs": flattened_tool_arg_schema("integer"),
                "args": flattened_string_array_tool_arg_schema(),
                "paths": flattened_string_array_tool_arg_schema(),
                "pattern": flattened_tool_arg_schema("string"),
                "limit": flattened_tool_arg_schema("integer"),
                "skip": {
                    "type": "integer",
                    "description": "Flattened git_log commit offset. Used only when `params` and `arguments` are absent."
                },
                "category": {
                    "type": "string",
                    "description": "Flattened list_tools/tool_manifest category filter. Used only when `params` and `arguments` are absent."
                },
                "include_recommended_flows": {
                    "type": "boolean",
                    "description": "Flattened tool_manifest flag. Defaults to true and controls recommended_flows in compact discovery output. Used only when `params` and `arguments` are absent."
                },
                "include_risk_summary": {
                    "type": "boolean",
                    "description": "Flattened tool_manifest flag. Defaults to true and controls risk_summary in compact discovery output. Used only when `params` and `arguments` are absent."
                },
                "include_diff": flattened_tool_arg_schema("boolean"),
                "include_hygiene": {
                    "type": "boolean",
                    "description": "Flattened finish_coding_task flag. Defaults to true. Used only when `params` and `arguments` are absent."
                },
                "max_findings": {
                    "type": "integer",
                    "description": "Flattened workspace_hygiene_check maximum findings to return; clamped by the runtime to 1..200. Used only when `params` and `arguments` are absent."
                },
                "include_tracked": {
                    "type": "boolean",
                    "description": "Flattened workspace_hygiene_check flag. When true, also report tracked suspicious path names by path/name only; file contents are never read. Used only when `params` and `arguments` are absent."
                },
                "include_handoff": {
                    "type": "boolean",
                    "description": "Flattened finish_coding_task flag. Defaults to true. Used only when `params` and `arguments` are absent."
                },
                "include_validation_summary": {
                    "type": "boolean",
                    "description": "Flattened finish_coding_task flag. Defaults to true; minimal diagnostics may be derived from safe bounded validation metadata, but raw stdout/stderr is never exposed. Used only when `params` and `arguments` are absent."
                },
                "include_validation": {
                    "type": "boolean",
                    "description": "Flattened session_handoff_summary flag. Defaults to true; validation is ledger-derived and parser.available is true only when safe bounded metadata is present. Used only when `params` and `arguments` are absent."
                },
                "include_workspace": {
                    "type": "boolean",
                    "description": "Flattened session_handoff_summary flag. Include a bounded workspace/git status summary when project is provided. Used only when params and arguments are absent."
                },
                "include_checkpoints": {
                    "type": "boolean",
                    "description": "Flattened session_handoff_summary flag. Include bounded checkpoint candidates when project is provided. Used only when params and arguments are absent."
                },
                "max_hunks": flattened_tool_arg_schema("integer"),
                "max_hunk_lines": flattened_tool_arg_schema("integer"),
                "session_event_limit": flattened_tool_arg_schema("integer"),
                "cached": flattened_tool_arg_schema("boolean"),
                "check": flattened_tool_arg_schema("boolean"),
                "all_targets": flattened_tool_arg_schema("boolean"),
                "all_features": flattened_tool_arg_schema("boolean"),
                "no_default_features": flattened_tool_arg_schema("boolean"),
                "features": {
                    "type": "string",
                    "description": "Flattened list_tools feature filter, or cargo feature selection for cargo tools. Used only when `params` and `arguments` are absent."
                },
                "summary_only": {
                    "type": "boolean",
                    "description": "Flattened list_tools/session_handoff_summary/finish_coding_task flag. For list_tools, returns compact tool summaries without full schemas. For handoff/finish, returns compact verdict fields and omits recent_events, long ledger details, command text, stdout/stderr, tails, and excerpts. Used only when `params` and `arguments` are absent."
                },
                "package": flattened_tool_arg_schema("string"),
                "filter": flattened_tool_arg_schema("string"),
                "no_run": flattened_tool_arg_schema("boolean"),
                "start_line": flattened_tool_arg_schema("integer"),
                "context_before": flattened_tool_arg_schema("integer"),
                "context_after": flattened_tool_arg_schema("integer"),
                "with_line_numbers": flattened_tool_arg_schema("boolean"),
                "end_line": flattened_tool_arg_schema("integer"),
                "line": flattened_tool_arg_schema("integer"),
                "text": flattened_tool_arg_schema("string"),
                "old_text": flattened_tool_arg_schema("string"),
                "new_text": flattened_tool_arg_schema("string"),
                "expected_old_sha256": flattened_tool_arg_schema("string"),
                "expected_old_prefix": flattened_tool_arg_schema("string"),
                "expected_anchor_sha256": flattened_tool_arg_schema("string"),
                "expected_anchor_prefix": flattened_tool_arg_schema("string"),
                "content": flattened_tool_arg_schema("string"),
                "content_base64": flattened_tool_arg_schema("string"),
                "mime_type": flattened_tool_arg_schema("string"),
                "upload_id": {
                    "type": "string",
                    "description": "Flattened artifact_upload_chunk/finish/abort wc_upload_* id. The same path from artifact_upload_begin is also required so the runtime can bind upload_id to the requested target path. Used only when `params` and `arguments` are absent."
                },
                "expected_bytes": {
                    "type": "integer",
                    "description": "Flattened artifact_upload_begin final byte count guard. Used only when `params` and `arguments` are absent."
                },
                "encoding": flattened_tool_arg_schema("string"),
                "max_bytes": flattened_tool_arg_schema("integer"),
                "overwrite": flattened_tool_arg_schema("boolean"),
                "expected_sha256": flattened_tool_arg_schema("string"),
                "expected_content_prefix": flattened_tool_arg_schema("string"),
                "old": flattened_tool_arg_schema("string"),
                "new": flattened_tool_arg_schema("string"),
                "expected_replacements": flattened_tool_arg_schema("integer"),
                "allow_multiple": flattened_tool_arg_schema("boolean"),
                "allow_missing": {
                    "type": "boolean",
                    "description": "Flattened read_project_artifact_metadata flag. When true, a missing artifact returns exists=false instead of a failed tool call. Used only when `params` and `arguments` are absent."
                },
                "paths": flattened_string_array_tool_arg_schema(),
                "patch": flattened_tool_arg_schema("string"),
                "deny_sensitive_paths": flattened_tool_arg_schema("boolean"),
                "job_id": flattened_tool_arg_schema("string"),
                "tail_lines": flattened_tool_arg_schema("integer"),
                "offset": flattened_tool_arg_schema("integer"),
                "length": flattened_tool_arg_schema("integer"),
                "client_id": flattened_tool_arg_schema("string"),
                "id": flattened_tool_arg_schema("string"),
                "name": flattened_tool_arg_schema("string"),
                "description": flattened_tool_arg_schema("string"),
                "allow_patch": flattened_tool_arg_schema("boolean"),
                "template": flattened_tool_arg_schema("string"),
                "git_init": flattened_tool_arg_schema("boolean"),
                "allow_existing_empty": flattened_tool_arg_schema("boolean")
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
                    "description": "Runtime job id returned by run_job."
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
                    "description": "Runtime job id returned by run_job."
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
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
                },
                "start_line": {
                    "type": "integer",
                    "description": "Optional 1-based line offset for pagination."
                },
                "limit": {
                    "type": "integer",
                    "description": "Optional maximum line count (bounded server-side)."
                },
                "with_line_numbers": {
                    "type": "boolean",
                    "description": "Optional. When true, output includes numbered_text and lines with 1-based line numbers."
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
                },
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
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
                },
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
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
                    "description": PATCH_FIELD_DESCRIPTION
                },
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
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
                    "description": PATCH_FIELD_DESCRIPTION
                },
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
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
                    "description": PATCH_FIELD_DESCRIPTION
                },
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
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
                },
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
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
                },
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
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
                },
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
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
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
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
        "WriteProjectFileRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project", "path", "content"],
            "description": "Write a UTF-8 project file via the owning agent. Mutation with side effects; creates new files and overwrites existing ones when a guard matches.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "path": {
                    "type": "string",
                    "description": "Project-relative file path. Absolute paths and traversal (..) are rejected. Sensitive paths are rejected."
                },
                "content": {
                    "type": "string",
                    "description": "Full UTF-8 file content to write."
                },
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
                },
                "overwrite": {
                    "type": "boolean",
                    "description": "Optional. When true, allows overwriting an existing file (guarded by expected_sha256 / expected_content_prefix when set)."
                },
                "expected_sha256": {
                    "type": "string",
                    "description": "Optional sha256 of the existing file content. Overwrite only proceeds when it matches; prevents accidental overwrites."
                },
                "expected_content_prefix": {
                    "type": "string",
                    "description": "Optional prefix the existing file content must start with before overwriting."
                }
            }
        },
        "StartProjectShellJobRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["project", "command"],
            "description": "Start an async background shell job in an agent-registered project. Execution with side effects; returns a job_id to poll with getRuntimeJobStatus.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "command": {
                    "type": "string",
                    "description": "Shell command to run asynchronously in the project directory."
                },
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
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
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
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
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
                },
                "path": {
                    "type": "string",
                    "description": "Optional project-relative directory to scope the search (default: project root)."
                },
                "limit": {
                    "type": "integer",
                    "description": "Optional maximum number of matches to return."
                },
                "context_before": {
                    "type": "integer",
                    "description": "Optional context lines before each match; clamped server-side to 20."
                },
                "context_after": {
                    "type": "integer",
                    "description": "Optional context lines after each match; clamped server-side to 20."
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
                    "description": "Runtime job id returned by run_job."
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
            "description": "Run a shell command in an agent-registered project. Executable with side effects; result output includes command_started, command_ok, failure_kind, and tool_failure semantics.",
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Agent-registered runtime project id from listProjects, such as `agent:<client_id>:<project_id>`."
                },
                "command": {
                    "type": "string",
                    "description": "Shell command to run in the project directory."
                },
                "session_id": {
                    "type": "string",
                    "description": SESSION_ID_FIELD_DESCRIPTION
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
            "required": ["name", "description", "inputSchema", "outputSchema", "annotations"],
            "properties": {
                "name": { "type": "string" },
                "description": { "type": "string" },
                "inputSchema": { "type": "object", "additionalProperties": true },
                "outputSchema": { "type": "object", "additionalProperties": true },
                "annotations": {
                    "type": "object",
                    "description": "Tool annotations / client hints.",
                    "additionalProperties": true
                }
            }
        },
        "ToolSummary": {
            "type": "object",
            "required": ["name", "category", "risk", "read_only", "requires_project"],
            "description": "Compact tool summary returned by listRuntimeTools when summary_only=true.",
            "properties": {
                "name": { "type": "string" },
                "description": { "type": "string" },
                "category": { "type": "string" },
                "risk": { "type": "string" },
                "read_only": { "type": "boolean" },
                "requires_project": { "type": "boolean" },
                "annotations": {
                    "type": "object",
                    "description": "Tool annotations / client hints.",
                    "additionalProperties": true
                }
            }
        },
        "ToolsListResponse": {
            "type": "object",
            "required": ["success", "tools", "names", "count"],
            "description": "Runtime tool list. No-arg calls return the full MCP-compatible ToolSpec list for schema debugging. Bounded calls can return compact ToolSummary entries without schemas. GPT Actions should prefer tool_manifest for daily discovery.",
            "properties": {
                "success": { "type": "boolean" },
                "tools": {
                    "type": "array",
                    "items": {
                        "oneOf": [
                            { "$ref": "#/components/schemas/ToolSpec" },
                            { "$ref": "#/components/schemas/ToolSummary" }
                        ]
                    }
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
                "total_count": {
                    "type": "integer",
                    "description": "Total number of model-visible runtime tools before filters."
                },
                "filtered_count": {
                    "type": "integer",
                    "description": "Number of tools matching category/features before limit."
                },
                "truncated": {
                    "type": "boolean",
                    "description": "Whether the response was truncated by limit."
                },
                "category": {
                    "type": ["string", "null"],
                    "description": "Requested category filter, when provided."
                },
                "features": {
                    "type": ["string", "null"],
                    "description": "Requested feature filter, when provided."
                },
                "limit": {
                    "type": ["integer", "null"],
                    "description": "Effective bounded discovery limit, when a bounded request was used."
                },
                "categories": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "description": "Optional grouping by family: inspect, git, review, validation, patch, edit, shell, jobs, runtime, cleanup. A tool may appear in more than one category."
                },
                "recommended_flows": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional short GPT flow hints for common tool sequences."
                },
                "hint": {
                    "type": "string",
                    "description": "Short guidance for using bounded discovery."
                },
                "recommended_next": {
                    "type": "string",
                    "description": "Recommended next discovery action."
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
        },
        "RegisterProjectRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["client_id", "id", "name", "path"],
            "description": "Register an existing directory as a WebCodex project on the selected agent. Mutation with side effects; executes on the agent and is constrained by agent policy.",
            "properties": {
                "client_id": {"type": "string", "description": "Registered agent client_id from listAgents."},
                "id": {"type": "string", "description": "Project id (ASCII letters, digits, '-', '_'; no slash)."},
                "name": {"type": "string", "description": "Human-readable project name."},
                "path": {"type": "string", "description": "Absolute directory path on the agent host."},
                "description": {"type": "string", "description": "Optional project description."},
                "allow_patch": {"type": "boolean", "description": "Allow patch operations on this project (default true)."},
                "overwrite": {"type": "boolean", "description": "Overwrite an existing project config file (default false)."}
            }
        },
        "CreateProjectRequest": {
            "type": "object",
            "additionalProperties": false,
            "required": ["client_id", "id", "name", "path"],
            "description": "Create a new directory on the selected agent and register it as a WebCodex project. Mutation with side effects; executes on the agent and is constrained by agent policy.",
            "properties": {
                "client_id": {"type": "string", "description": "Registered agent client_id from listAgents."},
                "id": {"type": "string", "description": "Project id (ASCII letters, digits, '-', '_'; no slash)."},
                "name": {"type": "string", "description": "Human-readable project name."},
                "path": {"type": "string", "description": "Absolute directory path on the agent host."},
                "description": {"type": "string", "description": "Optional project description."},
                "allow_patch": {"type": "boolean", "description": "Allow patch operations on this project (default true)."},
                "template": {"type": "string", "description": "Template: 'empty' (default) or 'basic'."},
                "git_init": {"type": "boolean", "description": "Initialize git in the new directory (default false)."},
                "allow_existing_empty": {"type": "boolean", "description": "Allow registering an existing empty directory (default false)."},
                "overwrite": {"type": "boolean", "description": "Overwrite an existing project config file (default false)."}
            }
        }
    });
    insert_tool_call_request_reserved_properties(&mut schemas);
    schemas
}

fn insert_tool_call_request_reserved_properties(schemas: &mut Value) {
    let Some(properties) = schemas
        .pointer_mut("/ToolCallRequest/properties")
        .and_then(Value::as_object_mut)
    else {
        return;
    };

    properties.insert(
        TOOL_CALL_TOOL_FIELD.to_string(),
        json!({
            "type": "string",
            "description": format!(
                "Runtime tool name. Accepted values: {}. Prefer tool_manifest for daily discovery; use listRuntimeTools for schema debugging.",
                crate::tool_runtime::tool_definition::model_visible_tool_names_csv()
            )
        }),
    );
    properties.insert(
        TOOL_CALL_PARAMS_FIELD.to_string(),
        json!({
            "type": "object",
            "description": "Tool-specific arguments object for non-Action clients. Takes precedence over `arguments` when both are present. GPT Actions should prefer flattened top-level fields.",
            "nullable": true,
            "additionalProperties": true
        }),
    );
    properties.insert(
        TOOL_CALL_ARGUMENTS_FIELD.to_string(),
        json!({
            "type": "object",
            "description": "Compatibility alias for `params`. Used only when `params` is absent; ignored otherwise.",
            "nullable": true,
            "additionalProperties": true
        }),
    );
    properties.insert(
        TOOL_CALL_RECORDING_SESSION_ID_FIELD.to_string(),
        json!({
            "type": "string",
            "description": "Optional recorder metadata for the generic wrapper call. Pass an explicit wc_sess_* id from start_session to record this call in the session ledger and enforce that recorder session's guards. This field is stripped before concrete tool dispatch. Use top-level session_id for ordinary tool input such as session_summary.session_id or post_session_message.session_id."
        }),
    );
    properties.insert(
        TOOL_EXPECTED_FAILURE_FIELD.to_string(),
        json!({
            "type": "boolean",
            "description": "Flattened testing/smoke metadata only. When true, a failed runtime tool call is classified as an expected failure in session_handoff_summary/finish_coding_task. Does not change authorization, permission decisions, execution, hard guards, command_started, or the immediate success/error result."
        }),
    );
    properties.insert(
        TOOL_EXPECTED_FAILURE_KIND_FIELD.to_string(),
        json!({
            "type": "string",
            "description": "Flattened testing/smoke metadata only. Expected structured failure_kind or error_kind when expected_failure=true. Mismatches are surfaced in handoff/finish summaries and do not change tool behavior."
        }),
    );
    properties.insert(
        TOOL_EXPECT_FAILURE_KIND_ALIAS_FIELD.to_string(),
        json!({
            "type": "string",
            "description": "Flattened testing/smoke alias for expected_failure_kind. Matches structured failure_kind or error_kind and does not change authorization, permissions, execution, or immediate output."
        }),
    );
    properties.insert(
        TOOL_ASSERTION_NAME_FIELD.to_string(),
        json!({
            "type": "string",
            "description": "Flattened testing/smoke assertion label recorded in the session ledger. Does not change tool behavior or safety decisions."
        }),
    );
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
    fn openapi_operations_have_consequential_flags() {
        let spec = build_openapi_spec();
        let paths = spec["paths"].as_object().unwrap();
        let mut count = 0;
        for methods in paths.values() {
            for op in methods.as_object().unwrap().values() {
                count += 1;
                let operation_id = op["operationId"].as_str().unwrap();
                assert!(
                    op.get("x-openai-isConsequential")
                        .and_then(|v| v.as_bool())
                        .is_some(),
                    "operation {} must have x-openai-isConsequential",
                    operation_id
                );
            }
        }
        assert_eq!(count, 25);
    }

    #[test]
    fn openapi_consequential_flags_match_operation_risk() {
        let spec = build_openapi_spec();
        let mut flags = std::collections::BTreeMap::new();
        for methods in spec["paths"].as_object().unwrap().values() {
            for op in methods.as_object().unwrap().values() {
                let operation_id = op["operationId"].as_str().unwrap().to_string();
                let consequential = op["x-openai-isConsequential"].as_bool().unwrap();
                flags.insert(operation_id, consequential);
            }
        }
        let readonly = [
            "listRuntimeTools",
            "listProjects",
            "getRuntimeStatus",
            "readProjectFile",
            "listProjectFiles",
            "searchProjectText",
            "getProjectGitStatus",
            "getProjectGitDiff",
            "getProjectGitDiffSummary",
            "getRuntimeJobStatus",
            "getRuntimeJobLog",
            "getRuntimeJobTail",
            "listRuntimeJobs",
            "validateProjectPatch",
            "registerProject",
            "createProject",
        ];
        let consequential = [
            "applyProjectPatch",
            "applyProjectPatchChecked",
            "runProjectShellCommand",
            "startProjectShellJob",
            "deleteProjectFiles",
            "gitRestorePaths",
            "discardUntrackedFiles",
            "callRuntimeTool",
        ];
        for id in readonly {
            assert_eq!(
                flags.get(id),
                Some(&false),
                "{} should be non-consequential",
                id
            );
        }
        for id in consequential {
            assert_eq!(flags.get(id), Some(&true), "{} should be consequential", id);
        }
        assert_eq!(flags.len(), 25);
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
            "/api/projects/register",
            "/api/projects/create",
            "/api/runtime/status",
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
            "/api/projects/run_job",
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
        let description = spec["components"]["securitySchemes"]["bearerAuth"]["description"]
            .as_str()
            .expect("bearerAuth description");
        assert!(
            description.contains("shared key"),
            "bearerAuth description must mention shared-key quick start: {description}"
        );
        assert!(
            description.contains("quick start"),
            "bearerAuth description must mention quick start: {description}"
        );
        assert!(
            description.contains("wc_pat"),
            "bearerAuth description must mention wc_pat managed tokens: {description}"
        );
        assert!(
            description.contains("managed mode"),
            "bearerAuth description must mention managed mode: {description}"
        );
        assert!(
            !description.contains("personal API token only")
                && !description.contains("wc_pat only"),
            "bearerAuth description must not regress to PAT-only guidance: {description}"
        );
    }

    #[test]
    fn openapi_info_description_mentions_bearer_host_auth_modes() {
        let spec = build_openapi_spec();
        let description = spec["info"]["description"]
            .as_str()
            .expect("info description");
        for expected in [
            "static bearer/API-key hosts",
            "shared key",
            "quick start",
            "wc_pat",
            "managed mode",
        ] {
            assert!(
                description.contains(expected),
                "OpenAPI info.description must mention {expected:?}: {description}"
            );
        }
        assert!(
            !description.contains("personal API token only")
                && !description.contains("wc_pat only"),
            "OpenAPI info.description must not regress to PAT-only guidance: {description}"
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
    fn openapi_hides_codex_delegation_from_model_facing_spec() {
        let spec = build_openapi_spec();
        assert!(
            spec["paths"].get("/api/codex/run").is_none(),
            "Codex delegation must not be exposed as a GPT Action path"
        );
        let serialized = serde_json::to_string(&spec).unwrap();
        assert!(
            !serialized.contains("runCodexTask"),
            "Codex delegation operation id must stay hidden from OpenAPI"
        );
        assert!(
            !serialized.contains("CodexRunRequest"),
            "Codex delegation request schema must stay hidden from OpenAPI"
        );
        let tool_desc = spec["components"]["schemas"]["ToolCallRequest"]["properties"]
            [TOOL_CALL_TOOL_FIELD]["description"]
            .as_str()
            .unwrap();
        assert!(
            !tool_desc.contains("run_codex"),
            "callRuntimeTool allowed-name description must not advertise run_codex"
        );
        // callRuntimeTool should be marked advanced/generic.
        let call_tool = &spec["paths"]["/api/tools/call"]["post"]["description"]
            .as_str()
            .unwrap();
        assert!(
            call_tool.contains("Advanced"),
            "callRuntimeTool description should mark it as advanced"
        );
        assert!(
            call_tool.contains("generic escape hatch")
                && call_tool.contains("Prefer dedicated actions"),
            "callRuntimeTool description should prefer dedicated tools: {call_tool}"
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
        use crate::tool_runtime::tool_definition::model_visible_tool_definitions;

        let spec = build_openapi_spec();
        let tool_desc = &spec["components"]["schemas"]["ToolCallRequest"]["properties"]
            [TOOL_CALL_TOOL_FIELD]["description"]
            .as_str()
            .unwrap();
        for definition in model_visible_tool_definitions() {
            let name = definition.name;
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
        // getRuntimeJobStatus, getRuntimeJobLog, and callRuntimeTool must ship
        // with at least one request example so GPT has a concrete template to
        // follow. Phase 3 dedicated actions are also required to carry examples.
        for (path, label) in [
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
            ("/api/projects/run_job", "startProjectShellJob"),
            ("/api/projects/register", "registerProject"),
            ("/api/projects/create", "createProject"),
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
            spec["paths"]["/api/projects/run_job"]["post"]["operationId"],
            "startProjectShellJob"
        );
        assert_eq!(
            spec["paths"]["/api/projects/register"]["post"]["operationId"],
            "registerProject"
        );
        assert_eq!(
            spec["paths"]["/api/projects/create"]["post"]["operationId"],
            "createProject"
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
            "/api/projects/run_job",
            "/api/projects/register",
            "/api/projects/create",
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
        }
        // The patch/shell/cleanup/write mutations must also mention the agent
        // shell capability. startProjectShellJob requires the async shell job
        // capability (checked separately below), not the plain shell capability.
        for path in [
            "/api/projects/apply_patch",
            "/api/projects/apply_patch_checked",
            "/api/projects/run_shell",
            "/api/projects/delete_files",
            "/api/projects/git_restore_paths",
            "/api/projects/discard_untracked",
        ] {
            let desc = spec["paths"][path]["post"]["description"]
                .as_str()
                .unwrap_or("");
            assert!(
                desc.to_lowercase().contains("agent shell capability"),
                "{} description should mention the agent shell capability, got: {}",
                path,
                desc
            );
        }
        // startProjectShellJob requires the async shell job capability, not
        // the plain shell capability. Pin its capability wording so GPT callers
        // understand the different requirement.
        {
            let desc = spec["paths"]["/api/projects/run_job"]["post"]["description"]
                .as_str()
                .unwrap_or("");
            assert!(
                desc.to_lowercase().contains("async shell job"),
                "startProjectShellJob description should mention the async shell job capability, got: {}",
                desc
            );
        }
    }

    #[test]
    fn openapi_readonly_actions_describe_readonly() {
        // Every read-only dedicated action must mark itself read-only (or
        // "never writes") in its description so GPT callers can tell them
        // apart from mutations. This covers all 14 read-only operations;
        // callRuntimeTool is excluded because it is a generic escape hatch
        // that can dispatch either read-only or mutating tools.
        let spec = build_openapi_spec();
        for path in [
            "/api/tools/list",
            "/api/projects/list",
            "/api/runtime/status",
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
        ] {
            let desc = spec["paths"][path]["post"]["description"]
                .as_str()
                .unwrap_or("");
            let lower = desc.to_lowercase();
            assert!(
                lower.contains("read-only") || lower.contains("never writes"),
                "{} description should be marked read-only or never writes, got: {}",
                path,
                desc
            );
        }
    }

    #[test]
    fn openapi_request_body_schemas_have_additional_properties_false() {
        // Every requestBody schema referenced by an operation must declare
        // `additionalProperties: false` at the top level so GPT Actions
        // rejects unknown fields rather than silently dropping them. Inner
        // properties (e.g. ToolCallRequest.params) may still allow arbitrary
        // keys; this guard only pins the top-level request object.
        let spec = build_openapi_spec();
        let schemas = spec["components"]["schemas"]
            .as_object()
            .expect("schemas object");
        for (path, methods) in spec["paths"].as_object().unwrap() {
            for (method, op) in methods.as_object().unwrap() {
                let request_schema_ref =
                    op["requestBody"]["content"]["application/json"]["schema"]["$ref"].as_str();
                let schema_name = match request_schema_ref {
                    Some(r) => r.strip_prefix("#/components/schemas/").unwrap_or(r),
                    None => continue,
                };
                let schema = schemas.get(schema_name).unwrap_or_else(|| {
                    panic!(
                        "{} {} references unknown schema '{}'",
                        method, path, schema_name
                    )
                });
                assert_eq!(
                    schema["additionalProperties"],
                    Value::Bool(false),
                    "{} {} requestBody schema '{}' must have additionalProperties=false",
                    method,
                    path,
                    schema_name
                );
            }
        }
    }

    #[test]
    fn openapi_file_search_shell_schemas_include_ergonomics_fields() {
        let spec = build_openapi_spec();
        let schemas = &spec["components"]["schemas"];
        let read_props = schemas["ReadProjectFileRequest"]["properties"]
            .as_object()
            .unwrap();
        assert!(read_props.contains_key("with_line_numbers"));

        let search_props = schemas["SearchProjectTextRequest"]["properties"]
            .as_object()
            .unwrap();
        assert!(search_props.contains_key("context_before"));
        assert!(search_props.contains_key("context_after"));

        let run_shell_description = schemas["RunShellRequest"]["description"]
            .as_str()
            .unwrap_or("");
        assert!(run_shell_description.contains("shell command"));
        let op_description = spec["paths"]["/api/projects/run_shell"]["post"]["description"]
            .as_str()
            .unwrap_or("");
        assert!(op_description.contains("failure_kind"));
        assert!(op_description.contains("tool_failure"));
    }

    #[test]
    fn openapi_dedicated_project_action_schemas_include_optional_session_id() {
        let spec = build_openapi_spec();
        let schemas = &spec["components"]["schemas"];
        for name in [
            "ReadProjectFileRequest",
            "RunShellRequest",
            "WriteProjectFileRequest",
            "ProjectIdRequest",
            "ProjectGitDiffRequest",
            "SearchProjectTextRequest",
            "ApplyPatchRequest",
            "ApplyPatchCheckedRequest",
            "ValidatePatchRequest",
            "DeleteProjectFilesRequest",
            "GitRestorePathsRequest",
            "DiscardUntrackedRequest",
            "ReplaceInFileRequest",
            "StartProjectShellJobRequest",
            "ListProjectFilesRequest",
        ] {
            let schema = &schemas[name];
            assert!(
                schema["properties"].get("session_id").is_some(),
                "{name} missing optional session_id property"
            );
            assert_eq!(
                schema["properties"]["session_id"]["description"], SESSION_ID_FIELD_DESCRIPTION,
                "{name} session_id description should match dedicated action guidance"
            );
            let required = schema["required"].as_array().unwrap();
            assert!(
                !required.iter().any(|field| field == "session_id"),
                "{name} must not require session_id"
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
            properties.contains_key(TOOL_CALL_PARAMS_FIELD),
            "ToolCallRequest must declare a `params` property"
        );
        let params = &properties[TOOL_CALL_PARAMS_FIELD];
        assert_eq!(params["type"], "object", "params must be type object");
        assert_eq!(params["nullable"], true, "params must allow null");
        assert_eq!(
            params["additionalProperties"], true,
            "params must allow arbitrary object properties"
        );
        let description = tool_call["description"].as_str().unwrap_or("");
        assert!(
            description.contains(TOOL_CALL_RECORDING_SESSION_ID_FIELD)
                && description.contains("flattened top-level fields"),
            "ToolCallRequest should document GPT Action flattened fields and recorder metadata: {description}"
        );
        for phrase in [
            "record this wrapper call in the session ledger",
            "current-session binding is process-local in-memory",
            "not the durable session ledger",
            "explicit session_id or recording_session_id",
        ] {
            assert!(
                description.contains(phrase),
                "ToolCallRequest should document {phrase}: {description}"
            );
        }
        let properties = tool_call["properties"].as_object().unwrap();
        let recording_desc = properties[TOOL_CALL_RECORDING_SESSION_ID_FIELD]["description"]
            .as_str()
            .unwrap_or("");
        assert!(
            recording_desc.contains("session ledger"),
            "recording_session_id should mention session ledger: {recording_desc}"
        );
        let session_desc = properties["session_id"]["description"]
            .as_str()
            .unwrap_or("");
        assert!(
            session_desc.contains("explicit tool session")
                && session_desc.contains("wins over current-session binding"),
            "session_id should describe explicit session priority: {session_desc}"
        );
        let start_example = &spec["paths"]["/api/tools/call"]["post"]["requestBody"]["content"]
            ["application/json"]["examples"]["trackedSession"]["value"];
        assert_eq!(start_example["mode"], "read_only");
        assert_eq!(start_example["title"], "implement show_changes follow-up");
        // `tool` remains required; `params` is optional (advanced callers may
        // omit it for argument-less tools).
        let required = tool_call["required"].as_array().unwrap();
        assert!(required
            .iter()
            .any(|v| v.as_str() == Some(TOOL_CALL_TOOL_FIELD)));
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
            properties.contains_key(TOOL_CALL_ARGUMENTS_FIELD),
            "ToolCallRequest must declare an `arguments` alias property"
        );
        let arguments = &properties[TOOL_CALL_ARGUMENTS_FIELD];
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
    fn openapi_call_runtime_tool_declares_flattened_action_fields() {
        let spec = build_openapi_spec();
        let tool_call = &spec["components"]["schemas"]["ToolCallRequest"];
        let properties = tool_call["properties"].as_object().unwrap();

        for field in [
            "project",
            "title",
            "session_id",
            TOOL_CALL_RECORDING_SESSION_ID_FIELD,
            ALLOW_CROSS_PROJECT_SESSION_FIELD,
            TOOL_EXPECTED_FAILURE_FIELD,
            TOOL_EXPECTED_FAILURE_KIND_FIELD,
            TOOL_EXPECT_FAILURE_KIND_ALIAS_FIELD,
            TOOL_ASSERTION_NAME_FIELD,
            "mode",
            "deny_write_tools",
            "deny_shell_tools",
            "include_runtime_status",
            "include_git",
            "include_recent_commits",
            "include_rules",
            "include_tool_manifest",
            "tool_manifest_categories",
            "tool_manifest_limit",
            "bind_current",
            "path",
            "start_line",
            "end_line",
            "line",
            "text",
            "old_text",
            "new_text",
            "include_diff",
            "include_hygiene",
            "max_findings",
            "include_tracked",
            "include_handoff",
            "include_validation_summary",
            "include_validation",
            "include_workspace",
            "include_checkpoints",
            "summary_only",
            "include_recommended_flows",
            "include_risk_summary",
            "skip",
            "context_before",
            "context_after",
            "with_line_numbers",
            "content_base64",
            "mime_type",
            "upload_id",
            "expected_bytes",
            "encoding",
            "offset",
            "length",
            "max_bytes",
            "expected_old_prefix",
            "expected_anchor_prefix",
            "note",
            "include_untracked",
            "checkpoint_id",
            "confirm",
            "include_command_preview",
            "include_diff_stat",
            "edits",
            "dry_run",
            "expected_file_sha256",
        ] {
            assert!(
                properties.contains_key(field),
                "ToolCallRequest.properties.{} must exist for flattened GPT Action calls",
                field
            );
        }

        assert!(properties.contains_key(TOOL_CALL_PARAMS_FIELD));
        assert!(properties.contains_key(TOOL_CALL_ARGUMENTS_FIELD));
        assert_eq!(
            properties[ALLOW_CROSS_PROJECT_SESSION_FIELD]["type"],
            "boolean"
        );
        assert_eq!(properties[TOOL_EXPECTED_FAILURE_FIELD]["type"], "boolean");
        assert_eq!(
            properties[TOOL_EXPECTED_FAILURE_KIND_FIELD]["type"],
            "string"
        );
        assert_eq!(properties[TOOL_ASSERTION_NAME_FIELD]["type"], "string");
        let required = tool_call["required"].as_array().unwrap();
        assert_eq!(required, &vec![json!(TOOL_CALL_TOOL_FIELD)]);
        assert_eq!(tool_call["additionalProperties"], false);

        let desc_blob = serde_json::to_string(tool_call).unwrap();
        assert!(
            desc_blob.contains("top-level fields") && desc_blob.contains("params/arguments"),
            "ToolCallRequest must document flattened GPT Action compatibility"
        );
    }

    #[test]
    fn openapi_tool_call_request_exposes_handoff_flattened_flags() {
        let spec = build_openapi_spec();
        let tool_call = &spec["components"]["schemas"]["ToolCallRequest"];
        let properties = tool_call["properties"].as_object().unwrap();

        for field in [
            "include_validation",
            "include_workspace",
            "include_checkpoints",
            "include_tool_manifest",
            "tool_manifest_categories",
            "tool_manifest_limit",
            "include_recommended_flows",
            "include_risk_summary",
        ] {
            assert!(
                properties.contains_key(field),
                "ToolCallRequest.properties.{field} must exist for flattened GPT Action calls"
            );
        }

        assert_eq!(properties["include_validation"]["type"], "boolean");
        assert_eq!(properties["include_workspace"]["type"], "boolean");
        assert_eq!(properties["include_checkpoints"]["type"], "boolean");
        assert_eq!(properties["include_tool_manifest"]["type"], "boolean");
        assert_eq!(properties["tool_manifest_categories"]["type"], "array");
        assert_eq!(properties["tool_manifest_limit"]["type"], "integer");
        assert_eq!(properties["include_recommended_flows"]["type"], "boolean");
        assert_eq!(properties["include_risk_summary"]["type"], "boolean");
        assert_eq!(
            tool_call["additionalProperties"], false,
            "ToolCallRequest must keep explicit flattened fields with additionalProperties=false"
        );

        let count = operation_ids(&spec).len();
        assert_eq!(count, 25, "GPT Actions operation count must stay 25");
    }

    #[test]
    fn openapi_call_runtime_tool_declares_checkpoint_flattened_fields() {
        // Regression: GPT Action wrapper rejected checkpoint note,
        // include_untracked, checkpoint_id, confirm, and include_diff_stat
        // because ToolCallRequest.properties did not declare them while
        // additionalProperties stayed false. Each flattened field must be
        // an explicit top-level property so GPT Actions accept it.
        let spec = build_openapi_spec();
        let properties = spec["components"]["schemas"]["ToolCallRequest"]["properties"]
            .as_object()
            .unwrap();
        for field in [
            "note",
            "include_untracked",
            "checkpoint_id",
            "confirm",
            "include_diff_stat",
        ] {
            assert!(
                properties.contains_key(field),
                "ToolCallRequest.properties.{field} must exist for flattened checkpoint GPT Action calls"
            );
        }
        assert_eq!(properties["note"]["type"], "string");
        assert_eq!(properties["include_untracked"]["type"], "boolean");
        assert_eq!(properties["checkpoint_id"]["type"], "string");
        assert_eq!(properties["confirm"]["type"], "boolean");
        assert_eq!(properties["include_diff_stat"]["type"], "boolean");
        assert_eq!(
            spec["components"]["schemas"]["ToolCallRequest"]["additionalProperties"],
            false
        );
        let count: usize = spec["paths"]
            .as_object()
            .unwrap()
            .values()
            .map(|m| m.as_object().unwrap().len())
            .sum();
        assert_eq!(count, 25, "operation count must stay 25");
    }

    #[test]
    fn openapi_call_runtime_tool_declares_apply_text_edits_flattened_fields() {
        // Regression: GPT Action wrapper rejected apply_text_edits edits,
        // dry_run, and expected_file_sha256 because ToolCallRequest.properties
        // did not declare them. `edits` must mirror the runtime input schema
        // (bounded array of typed items), not a bare free-form object.
        let spec = build_openapi_spec();
        let tool_call = &spec["components"]["schemas"]["ToolCallRequest"];
        let properties = tool_call["properties"].as_object().unwrap();
        for field in ["edits", "dry_run", "expected_file_sha256"] {
            assert!(
                properties.contains_key(field),
                "ToolCallRequest.properties.{field} must exist for flattened apply_text_edits GPT Action calls"
            );
        }
        assert_eq!(properties["dry_run"]["type"], "boolean");
        assert_eq!(properties["expected_file_sha256"]["type"], "string");
        let edits = &properties["edits"];
        assert_eq!(
            edits["type"], "array",
            "edits must be an array, not a bare object"
        );
        assert_eq!(edits["minItems"], 1);
        assert_eq!(edits["maxItems"], 20);
        let items = &edits["items"];
        assert_eq!(items["type"], "object");
        assert_eq!(items["additionalProperties"], false);
        let kind_enum = &items["properties"]["kind"]["enum"]
            .as_array()
            .expect("edits.items.kind must be an enum");
        for variant in [
            "replace_exact",
            "insert_after",
            "insert_before",
            "delete_exact",
        ] {
            assert!(
                kind_enum.iter().any(|v| v == variant),
                "edits.items.kind enum must include {variant}"
            );
        }
        assert_eq!(
            tool_call["additionalProperties"], false,
            "additionalProperties must stay false"
        );
        let count: usize = spec["paths"]
            .as_object()
            .unwrap()
            .values()
            .map(|m| m.as_object().unwrap().len())
            .sum();
        assert_eq!(count, 25, "operation count must stay 25");
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
        assert!(props.contains_key("total_count"));
        assert!(props.contains_key("filtered_count"));
        assert!(props.contains_key("truncated"));
        assert!(props.contains_key("hint"));
        assert!(props.contains_key("recommended_next"));
        assert_eq!(
            spec["paths"]["/api/tools/list"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/ToolsListRequest"
        );
        let req_props = spec["components"]["schemas"]["ToolsListRequest"]["properties"]
            .as_object()
            .unwrap();
        for field in ["category", "features", "summary_only", "limit"] {
            assert!(
                req_props.contains_key(field),
                "ToolsListRequest must expose bounded field {field}"
            );
        }
        assert_eq!(
            spec["components"]["schemas"]["ToolsListRequest"]["properties"]["limit"]["maximum"],
            100
        );
    }

    #[test]
    fn openapi_tool_spec_includes_output_schema() {
        let spec = build_openapi_spec();
        let tool_spec = &spec["components"]["schemas"]["ToolSpec"];
        let required = tool_spec["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "inputSchema"));
        assert!(required.iter().any(|v| v == "outputSchema"));
        assert!(required.iter().any(|v| v == "annotations"));
        let props = tool_spec["properties"].as_object().unwrap();
        assert!(props["inputSchema"].is_object());
        assert!(props["outputSchema"].is_object());
        assert!(props["annotations"].is_object());
        assert_eq!(props["annotations"]["additionalProperties"], true);
    }

    #[test]
    fn openapi_runtime_only_tools_do_not_get_dedicated_paths() {
        let spec = build_openapi_spec();
        let paths = spec["paths"].as_object().unwrap();
        for forbidden in [
            "/api/projects/cargo_fmt",
            "/api/projects/cargo_check",
            "/api/projects/cargo_test",
            "/api/projects/git_diff_hunks",
            "/api/projects/show_changes",
            "/api/projects/workspace_checkpoint_create",
            "/api/projects/workspace_checkpoint_list",
            "/api/projects/workspace_checkpoint_show",
            "/api/projects/workspace_checkpoint_restore",
            "/api/projects/workspace_checkpoint_delete",
            "/api/projects/replace_in_file",
            "/api/projects/write_file",
        ] {
            assert!(
                !paths.contains_key(forbidden),
                "{} must remain runtime-only via callRuntimeTool",
                forbidden
            );
        }
    }

    #[test]
    fn openapi_operation_count_is_twenty_five_after_demoting_compatibility_edits() {
        // Phase 3 promoted 10 core runtime tools to dedicated GPT Actions,
        // then later phases promoted run_job plus project onboarding actions.
        // Compatibility edit tools are now runtime-only again, so the current
        // GPT Action count is 25 while Codex delegation remains hidden.
        // The surface must stay <= 30.
        let spec = build_openapi_spec();
        let count: usize = spec["paths"]
            .as_object()
            .unwrap()
            .values()
            .map(|m| m.as_object().unwrap().len())
            .sum();
        assert_eq!(
            count, 25,
            "GPT Actions schema must be 25 operations after demoting compatibility edits"
        );
        assert!(count <= 30, "GPT Actions schema must stay <= 30 operations");
    }

    #[test]
    fn openapi_artifact_upload_tools_remain_generic_and_under_action_limit() {
        let spec = build_openapi_spec();
        let paths = spec["paths"].as_object().unwrap();
        for path in [
            "/api/projects/artifact_upload_begin",
            "/api/projects/artifact_upload_chunk",
            "/api/projects/artifact_upload_finish",
            "/api/projects/artifact_upload_abort",
        ] {
            assert!(
                !paths.contains_key(path),
                "{path} must remain runtime-only via callRuntimeTool"
            );
        }

        let ids = operation_ids(&spec);
        for id in &ids {
            assert!(
                !id.contains("ArtifactUpload") && !id.contains("artifactUpload"),
                "artifact upload must not be promoted to a dedicated GPT Action: {id}"
            );
        }
        let count = ids.len();
        assert_eq!(count, 25, "GPT Actions operation count must stay 25");
        assert!(count <= 30, "GPT Actions operation count must stay <= 30");

        let tool_call = &spec["components"]["schemas"]["ToolCallRequest"];
        let tool_desc = tool_call["properties"][TOOL_CALL_TOOL_FIELD]["description"]
            .as_str()
            .unwrap();
        for tool in [
            "artifact_upload_begin",
            "artifact_upload_chunk",
            "artifact_upload_finish",
            "artifact_upload_abort",
        ] {
            assert!(
                tool_desc.contains(tool),
                "callRuntimeTool must document runtime tool {tool}"
            );
        }
        let properties = tool_call["properties"].as_object().unwrap();
        for field in [
            "project",
            "path",
            "content_base64",
            "upload_id",
            "offset",
            "expected_bytes",
            "expected_sha256",
            "mime_type",
            "overwrite",
            "allow_missing",
        ] {
            assert!(
                properties.contains_key(field),
                "ToolCallRequest.properties.{field} must exist for flattened artifact upload calls"
            );
        }
        let path_desc = properties["path"]["description"].as_str().unwrap();
        assert!(
            path_desc.contains("must exactly match the path used by artifact_upload_begin")
                && path_desc.contains("bind upload_id"),
            "path flattened description must explain upload path binding: {path_desc}"
        );
        let upload_id_desc = properties["upload_id"]["description"].as_str().unwrap();
        assert!(
            upload_id_desc.contains("same path from artifact_upload_begin is also required"),
            "upload_id flattened description must mention repeated path: {upload_id_desc}"
        );
        for field in [
            "category",
            "features",
            "summary_only",
            "limit",
            "include_recommended_flows",
            "include_risk_summary",
        ] {
            assert!(
                properties.contains_key(field),
                "ToolCallRequest.properties.{field} must exist for flattened list_tools/tool_manifest calls"
            );
        }
    }

    #[test]
    fn openapi_compatibility_edit_tools_remain_runtime_only() {
        // Compatibility edit tools remain reachable via callRuntimeTool / MCP
        // tools/call, but should not be promoted to dedicated GPT Actions.
        let spec = build_openapi_spec();
        let paths = spec["paths"].as_object().unwrap();
        assert!(
            !paths.contains_key("/api/projects/write_file"),
            "write_file must remain runtime-only through callRuntimeTool"
        );
        assert!(
            !paths.contains_key("/api/projects/replace_in_file"),
            "replace_in_file must remain runtime-only through callRuntimeTool"
        );
        assert!(
            paths.contains_key("/api/projects/run_job"),
            "run_job remains a dedicated execution action"
        );
        assert_eq!(
            spec["paths"]["/api/projects/run_job"]["post"]["operationId"],
            "startProjectShellJob"
        );
        assert!(
            LEGACY_FORBIDDEN_PATHS.contains(&"/api/projects/write_file"),
            "write_file must stay in the forbidden guard"
        );
        assert!(
            LEGACY_FORBIDDEN_PATHS.contains(&"/api/projects/replace_in_file"),
            "replace_in_file must stay in the forbidden guard"
        );
        assert!(
            !LEGACY_FORBIDDEN_PATHS.contains(&"/api/projects/run_job"),
            "run_job must not be in the forbidden guard now that it is a dedicated action"
        );
        let tool_desc = spec["components"]["schemas"]["ToolCallRequest"]["properties"]
            [TOOL_CALL_TOOL_FIELD]["description"]
            .as_str()
            .unwrap();
        for tool in ["write_project_file", "replace_in_file"] {
            assert!(
                tool_desc.contains(tool),
                "callRuntimeTool must document runtime tool {tool}"
            );
        }
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

    #[test]
    fn openapi_does_not_expose_agent_token_management_endpoints() {
        // Phase 3: agent token management endpoints (create/list/revoke) are
        // REST-only admin/self surfaces and must NOT appear in /openapi.json
        // (GPT Actions). They are listed in LEGACY_FORBIDDEN_PATHS too; this
        // test pins the specific endpoints for clarity.
        let spec = build_openapi_spec();
        let paths = spec["paths"].as_object().unwrap();
        for path in [
            "/api/agent-tokens/create",
            "/api/agent-tokens/register_hash",
            "/api/agent-tokens/list",
            "/api/agent-tokens/revoke",
        ] {
            assert!(
                !paths.contains_key(path),
                "agent token management endpoint '{}' must not appear in openapi.json",
                path
            );
        }
    }

    #[test]
    fn openapi_does_not_expose_pairing_endpoints() {
        let spec = build_openapi_spec();
        let paths = spec["paths"].as_object().unwrap();
        for path in ["/api/pairing/create", "/api/pairing/enroll"] {
            assert!(
                !paths.contains_key(path),
                "pairing endpoint '{}' must not appear in openapi.json",
                path
            );
        }
    }

    #[test]
    fn openapi_operation_count_stays_twenty_five_after_demoting_compatibility_edits() {
        // Phase 3 adds agent token management endpoints to the REST surface
        // but does NOT add them to /openapi.json. The GPT Actions operation
        // count must remain 25 while compatibility edit tools are runtime-only.
        let spec = build_openapi_spec();
        let count: usize = spec["paths"]
            .as_object()
            .unwrap()
            .values()
            .map(|m| m.as_object().unwrap().len())
            .sum();
        assert_eq!(
            count, 25,
            "GPT Actions schema must remain 25 operations after demoting compatibility edits"
        );
    }
}

#[cfg(test)]
mod patch_description_tests {
    use super::*;

    #[test]
    fn openapi_patch_request_descriptions_reject_codex_wrapper() {
        let spec = build_openapi_spec();
        let schemas = &spec["components"]["schemas"];
        let apply_desc = schemas["ApplyPatchRequest"]["properties"]["patch"]["description"]
            .as_str()
            .expect("ApplyPatchRequest patch description");
        let validate_desc = schemas["ValidatePatchRequest"]["properties"]["patch"]["description"]
            .as_str()
            .expect("ValidatePatchRequest patch description");
        let checked_desc = schemas["ApplyPatchCheckedRequest"]["properties"]["patch"]
            ["description"]
            .as_str()
            .expect("ApplyPatchCheckedRequest patch description");

        assert!(
            apply_desc.contains("raw standard unified diff"),
            "{apply_desc}"
        );
        assert!(
            validate_desc.contains("Codex apply_patch wrapper"),
            "{validate_desc}"
        );
        assert!(checked_desc.contains("*** Begin Patch"), "{checked_desc}");
    }
}
