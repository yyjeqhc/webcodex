use serde_json::{json, Value};

use super::types::ToolSpec;
use super::ToolRuntime;

pub(crate) fn object_schema(fields: Vec<(&str, &str, &str, bool)>) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for (name, kind, description, is_required) in fields {
        let schema = if kind == "array" {
            json!({
                "type": "array",
                "items": { "type": "string" },
                "description": description,
            })
        } else {
            json!({
                "type": kind,
                "description": description,
            })
        };
        properties.insert(name.to_string(), schema);
        if is_required {
            required.push(Value::String(name.to_string()));
        }
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

impl ToolRuntime {
    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "list_tools".to_string(),
                description: "List tools exposed by this WebCodex runtime.".to_string(),
                input_schema: object_schema(vec![]),
            },
            ToolSpec {
                name: "list_projects".to_string(),
                description: "List agent-registered runtime projects and their execution mode."
                    .to_string(),
                input_schema: object_schema(vec![]),
            },
            ToolSpec {
                name: "register_project".to_string(),
                description: "Register an existing directory as a WebCodex project on a selected agent. "
                    .to_string()
                    + "Mutation with side effects; constrained by agent policy. The agent validates "
                    + "the path, writes projects_dir/<id>.toml atomically, and refreshes its "
                    + "project list. Requires Bearer auth.",
                input_schema: object_schema(vec![
                    ("client_id", "string", "Registered agent client_id.", true),
                    ("id", "string", "Project id (ASCII letters, digits, '-', '_'; no slash).", true),
                    ("name", "string", "Human-readable project name.", true),
                    ("path", "string", "Absolute directory path on the agent host.", true),
                    ("description", "string", "Optional project description.", false),
                    ("allow_patch", "boolean", "Allow patch operations on this project (default true).", false),
                    ("overwrite", "boolean", "Overwrite an existing project config file (default false).", false),
                ]),
            },
            ToolSpec {
                name: "create_project".to_string(),
                description: "Create a new directory on the selected agent and register it as a WebCodex "
                    .to_string()
                    + "project. Mutation with side effects; constrained by agent policy. Creates "
                    + "directory, optional template, optional git init, writes projects_dir/<id>.toml "
                    + "atomically. Requires Bearer auth.",
                input_schema: object_schema(vec![
                    ("client_id", "string", "Registered agent client_id.", true),
                    ("id", "string", "Project id (ASCII letters, digits, '-', '_'; no slash).", true),
                    ("name", "string", "Human-readable project name.", true),
                    ("path", "string", "Absolute directory path on the agent host.", true),
                    ("description", "string", "Optional project description.", false),
                    ("allow_patch", "boolean", "Allow patch operations on this project (default true).", false),
                    ("template", "string", "Template: 'empty' (default) or 'basic'.", false),
                    ("git_init", "boolean", "Initialize git in the new directory (default false).", false),
                    ("allow_existing_empty", "boolean", "Allow registering an existing empty directory (default false).", false),
                    ("overwrite", "boolean", "Overwrite an existing project config file (default false).", false),
                ]),
            },
            ToolSpec {
                name: "list_agents".to_string(),
                description: "List connected local/remote execution agents.".to_string(),
                input_schema: object_schema(vec![]),
            },
            ToolSpec {
                name: "runtime_status".to_string(),
                description: "Return a structured runtime health/observability summary (service "
                    .to_string()
                    + "metadata, projects config status, agent client summaries, and job counts). "
                    + "Read-only; never exposes tokens, secrets, full env, or stdout/stderr.",
                input_schema: object_schema(vec![]),
            },
            ToolSpec {
                name: "run_shell".to_string(),
                description: "Run a short shell command inside an agent-registered project."
                    .to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Configured project id.", true),
                    ("command", "string", "Shell command to run.", true),
                    (
                        "timeout_secs",
                        "integer",
                        "Command timeout in seconds.",
                        false,
                    ),
                    (
                        "cwd",
                        "string",
                        "Optional project-relative working directory.",
                        false,
                    ),
                ]),
            },
            ToolSpec {
                name: "run_job".to_string(),
                description: "Start an asynchronous shell job inside an agent-registered project."
                    .to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Configured project id.", true),
                    (
                        "command",
                        "string",
                        "Shell command to run asynchronously.",
                        true,
                    ),
                    (
                        "timeout_secs",
                        "integer",
                        "Maximum runtime in seconds.",
                        false,
                    ),
                    (
                        "cwd",
                        "string",
                        "Optional project-relative working directory.",
                        false,
                    ),
                ]),
            },
            ToolSpec {
                name: "run_codex".to_string(),
                description: "Start Codex CLI as an asynchronous project job.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Configured project id.", true),
                    (
                        "prompt",
                        "string",
                        "Instruction prompt passed to Codex CLI.",
                        true,
                    ),
                    (
                        "approval_mode",
                        "string",
                        "Codex approval mode. Empty/none/off/disabled omit --approval-mode.",
                        false,
                    ),
                    (
                        "timeout_secs",
                        "integer",
                        "Maximum runtime in seconds.",
                        false,
                    ),
                    (
                        "cwd",
                        "string",
                        "Optional project-relative working directory.",
                        false,
                    ),
                    (
                        "extra_args",
                        "array",
                        "Optional extra Codex CLI arguments.",
                        false,
                    ),
                ]),
            },
            ToolSpec {
                name: "job_status".to_string(),
                description: "Get status for a runtime job.".to_string(),
                input_schema: object_schema(vec![("job_id", "string", "Job id.", true)]),
            },
            ToolSpec {
                name: "job_log".to_string(),
                description: "Read stdout/stderr for a runtime job.".to_string(),
                input_schema: object_schema(vec![
                    ("job_id", "string", "Job id.", true),
                    (
                        "offset",
                        "integer",
                        "Optional 1-based stdout line cursor.",
                        false,
                    ),
                    (
                        "tail_lines",
                        "integer",
                        "Optional number of trailing stdout lines to return.",
                        false,
                    ),
                ]),
            },
            ToolSpec {
                name: "list_project_files".to_string(),
                description: "List files in an agent-registered project directory (bounded, "
                    .to_string()
                    + "read-only). Returns project-relative paths plus a file/dir kind. Routed "
                    + "to the owning registered agent; the server never reads the agent project "
                    + "path directly.",
                input_schema: object_schema(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    (
                        "path",
                        "string",
                        "Optional project-relative directory to list (default: project root).",
                        false,
                    ),
                    (
                        "limit",
                        "integer",
                        "Maximum number of entries to return.",
                        false,
                    ),
                ]),
            },
            ToolSpec {
                name: "search_project_text".to_string(),
                description: "Search text inside an agent-registered project (bounded matches)."
                    .to_string()
                    + " Each match carries a project-relative path, 1-based line number, and a "
                    + "preview line. Sensitive/build directories (.git, target, node_modules) are "
                    + "excluded by default.",
                input_schema: object_schema(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("pattern", "string", "Text pattern to search for.", true),
                    (
                        "path",
                        "string",
                        "Optional project-relative directory to scope the search (default: project root).",
                        false,
                    ),
                    (
                        "limit",
                        "integer",
                        "Maximum number of matches to return.",
                        false,
                    ),
                ]),
            },
            ToolSpec {
                name: "git_diff_summary".to_string(),
                description: "Read-only git diff summary for a project: `git status --porcelain`, "
                    .to_string()
                    + "`git diff --stat`, and a parsed changed-file list. Does not modify the "
                    + "worktree.",
                input_schema: object_schema(vec![(
                    "project",
                    "string",
                    "Agent-registered project id.",
                    true,
                )]),
            },
            ToolSpec {
                name: "list_jobs".to_string(),
                description: "List bounded runtime job summaries across agent and local executors. "
                    .to_string()
                    + "Never returns stdout/stderr bodies — only metadata (job_id, kind, status, "
                    + "project, timestamps, exit_code).",
                input_schema: object_schema(vec![
                    (
                        "limit",
                        "integer",
                        "Maximum number of job summaries to return.",
                        false,
                    ),
                    (
                        "status",
                        "string",
                        "Optional status filter (e.g. running, completed, failed).",
                        false,
                    ),
                ]),
            },
            ToolSpec {
                name: "job_tail".to_string(),
                description: "Return bounded stdout/stderr tails for a job.".to_string(),
                input_schema: object_schema(vec![
                    ("job_id", "string", "Job id.", true),
                    (
                        "tail_lines",
                        "integer",
                        "Optional number of trailing lines to return per stream.",
                        false,
                    ),
                ]),
            },
            ToolSpec {
                name: "read_file".to_string(),
                description: "Read a UTF-8 file from an agent-registered project.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Configured project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("start_line", "integer", "1-based line offset.", false),
                    ("limit", "integer", "Maximum line count.", false),
                ]),
            },
            ToolSpec {
                name: "git_status".to_string(),
                description: "Run git status --porcelain for a project.".to_string(),
                input_schema: object_schema(vec![(
                    "project",
                    "string",
                    "Configured project id.",
                    true,
                )]),
            },
            ToolSpec {
                name: "git_diff".to_string(),
                description: "Run git diff for a project, optionally scoped to paths.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Configured project id.", true),
                    ("args", "array", "Optional path list.", false),
                ]),
            },
            ToolSpec {
                name: "apply_patch".to_string(),
                description: "Apply a unified diff patch to an agent-registered project."
                    .to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Configured project id.", true),
                    ("patch", "string", "Unified diff patch.", true),
                ]),
            },
            ToolSpec {
                name: "apply_patch_checked".to_string(),
                description: "Validate a patch, apply it only if it can apply, then return the post-apply diff summary.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("patch", "string", "Unified diff patch.", true),
                    ("deny_sensitive_paths", "boolean", "Block sensitive path warnings before applying.", false),
                ]),
            },
            ToolSpec {
                name: "delete_project_files".to_string(),
                description: "Delete selected project-relative files only; safer than arbitrary rm for cleanup.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("paths", "array", "Project-relative file paths to delete.", true),
                ]),
            },
            ToolSpec {
                name: "git_restore_paths".to_string(),
                description: "Restore selected tracked paths with git restore; does not remove untracked files.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("paths", "array", "Project-relative tracked paths to restore.", true),
                ]),
            },
            ToolSpec {
                name: "discard_untracked".to_string(),
                description: "Discard selected untracked files with git clean -f -- <paths>.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("paths", "array", "Project-relative untracked paths to remove.", true),
                ]),
            },
            ToolSpec {
                name: "validate_patch".to_string(),
                description: "Dry-run a unified diff with git apply --check/--stat through the owning agent; never writes files.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("patch", "string", "Unified diff patch to validate.", true),
                    ("deny_sensitive_paths", "boolean", "Block sensitive path warnings.", false),
                ]),
            },
            ToolSpec {
                name: "replace_in_file".to_string(),
                description: "Replace a unique substring in a project file via the owning agent. Safer than run_shell sed/awk for text edits. Rejects sensitive paths; fails without writing when old is missing or ambiguous.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("old", "string", "Non-empty substring to replace.", true),
                    ("new", "string", "Replacement string.", true),
                    (
                        "expected_replacements",
                        "integer",
                        "Expected occurrence count (default 1).",
                        false,
                    ),
                    (
                        "allow_multiple",
                        "boolean",
                        "Allow replacing multiple occurrences (default false).",
                        false,
                    ),
                ]),
            },
            ToolSpec {
                name: "write_project_file".to_string(),
                description: "Write a UTF-8 file in a project via the owning agent. Creates or overwrites; rejects sensitive paths. Provide expected_sha256 for safe overwrites. Server never reads the agent filesystem directly.".to_string(),
                input_schema: object_schema(vec![
                    ("project", "string", "Agent-registered project id.", true),
                    ("path", "string", "Project-relative file path.", true),
                    ("content", "string", "UTF-8 file content (no NUL).", true),
                    (
                        "overwrite",
                        "boolean",
                        "Allow overwriting an existing file (default false).",
                        false,
                    ),
                    (
                        "expected_sha256",
                        "string",
                        "Required sha256 of the current file when overwriting.",
                        false,
                    ),
                    (
                        "expected_content_prefix",
                        "string",
                        "Required prefix of the current file when overwriting.",
                        false,
                    ),
                ]),
            },
        ]
    }

    /// The sorted list of accepted runtime tool names (mirrors `tool_specs`).
    pub fn tool_names(&self) -> Vec<String> {
        self.tool_specs().iter().map(|s| s.name.clone()).collect()
    }

    /// Group every accepted tool name into coarse categories so a custom GPT
    /// can pick the right tool family at a glance. A tool may appear in more
    /// than one category. Returned as a JSON object keyed by category.
    pub fn tool_categories(&self) -> Value {
        let names = self.tool_names();
        let pick = |set: &[&str]| -> Vec<String> {
            set.iter()
                .filter(|n| names.iter().any(|x| x == **n))
                .map(|s| s.to_string())
                .collect()
        };
        json!({
            "inspect": pick(&[
                "list_tools", "list_projects", "list_agents", "runtime_status",
                "read_file", "list_project_files", "search_project_text",
                "git_status", "git_diff", "git_diff_summary"
            ]),
            "projects": pick(&["list_projects", "register_project", "create_project"]),
            "git": pick(&[
                "git_status", "git_diff", "git_diff_summary",
                "git_restore_paths", "discard_untracked"
            ]),
            "patch": pick(&["apply_patch", "apply_patch_checked", "validate_patch"]),
            "edit": pick(&["replace_in_file", "write_project_file"]),
            "shell": pick(&["run_shell", "run_job"]),
            "jobs": pick(&[
                "run_codex", "run_job", "job_status", "job_log",
                "list_jobs", "job_tail"
            ]),
            "runtime": pick(&[
                "list_tools", "list_projects", "list_agents", "runtime_status"
            ]),
            "cleanup": pick(&[
                "delete_project_files", "git_restore_paths", "discard_untracked"
            ]),
        })
    }

    /// Short, GPT-facing flow hints. Each entry is well under the 300-char
    /// ToolSpec/operation description budget.
    pub fn recommended_flows() -> Vec<&'static str> {
        vec![
            "Discovery: call list_projects then runtime_status to see agents and projects.",
            "Inspect: use git_diff_summary then read_file before proposing changes.",
            "Patch: call validate_patch to dry-run, then apply_patch_checked to apply safely.",
            "Cleanup: use delete_project_files / git_restore_paths / discard_untracked instead of ad hoc rm.",
            "Jobs: start run_codex, then poll job_status and read job_log/job_tail.",
        ]
    }
}
