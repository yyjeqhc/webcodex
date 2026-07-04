use super::super::input_schemas::{
    job_log_input_schema, job_status_input_schema, job_tail_input_schema, list_jobs_input_schema,
    run_codex_input_schema, run_job_input_schema, run_shell_input_schema, stop_job_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "run_shell",
            "Bounded command escape hatch for validation, builds, tests, or diagnostics only. Do not use as the primary file editing path; prefer cargo_* / validate_patch for common checks and structured line edit tools for source edits.",
            run_shell_input_schema(),
        ),
        tool_spec(
            "run_job",
            "Start an asynchronous shell job inside an agent-registered project.".to_string(),
            run_job_input_schema(),
        ),
        tool_spec(
            "stop_job",
            "Stop a bounded runtime job started through WebCodex. Requires confirm=true, obeys project/session ownership, never exposes stdout/stderr, and returns stop_effect/terminal lifecycle fields.",
            stop_job_input_schema(),
        ),
        tool_spec(
            "run_codex",
            "Optional Codex CLI delegation as an async project job. Requires Codex CLI installed and configured on the owning agent. Use only when the user explicitly asks to delegate to Codex; otherwise use WebCodex file/git/shell/line-edit tools directly.",
            run_codex_input_schema(),
        ),
        tool_spec(
            "job_status",
            "Get bounded lifecycle status for a runtime job. Omits command_preview by default and never returns stdout/stderr bodies.",
            job_status_input_schema(),
        ),
        tool_spec(
            "job_log",
            "Read stdout/stderr for a runtime job.",
            job_log_input_schema(),
        ),
        tool_spec(
            "list_jobs",
            "List bounded runtime job summaries across agent and local executors. "
                .to_string()
                + "Never returns stdout/stderr bodies — only metadata (job_id, kind, status, "
                + "project, timestamps, exit_code).",
            list_jobs_input_schema(),
        ),
        tool_spec(
            "job_tail",
            "Return bounded stdout/stderr tails for a job.",
            job_tail_input_schema(),
        ),
    ]
}
