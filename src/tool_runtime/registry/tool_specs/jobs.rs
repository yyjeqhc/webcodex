use super::super::input_schemas::{
    job_log_input_schema, job_status_input_schema, list_jobs_input_schema,
    open_session_shell_input_schema, run_job_input_schema, run_process_input_schema,
    run_shell_input_schema, session_shell_exec_input_schema, session_shell_identity_input_schema,
    stop_job_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "run_process",
            "Preferred bounded native-process primitive. Pass executable and args as structured data; argv is executed directly without shell parsing. Windows .cmd/.bat files are rejected because they require shell/script semantics. Use run_shell explicitly for those files, pipelines, redirection, builtins, shell functions, or operator diagnostics.",
            run_process_input_schema(),
        ),
        tool_spec(
            "run_shell",
            "Bounded command escape hatch for validation, builds, tests, or diagnostics only. Do not use as the primary file editing path; prefer cargo_* / validate_patch for common checks and apply_text_edits for source edits.",
            run_shell_input_schema(),
        ),
        tool_spec(
            "open_session_shell",
            "Explicitly open one bounded, long-lived sh/bash process for an active Workflow Session. State is isolated from run_shell/run_job and from every other Session.",
            open_session_shell_input_schema(),
        ),
        tool_spec(
            "session_shell_exec",
            "Execute one framed command in an existing Session persistent shell. Commands are serialized; cwd, variables, exports, functions, and umask remain in that shell process.",
            session_shell_exec_input_schema(),
        ),
        tool_spec(
            "session_shell_status",
            "Read Runner-authoritative state for an explicit Session persistent shell. This never sends input to the process.",
            session_shell_identity_input_schema(),
        ),
        tool_spec(
            "close_session_shell",
            "Idempotently close an explicit Session persistent shell and terminate its complete process group.",
            session_shell_identity_input_schema(),
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
            "job_status",
            "Get bounded lifecycle status for a runtime job. Omits command_preview by default and never returns stdout/stderr bodies.",
            job_status_input_schema(),
        ),
        tool_spec(
            "job_log",
            "Read stdout/stderr for a runtime job. With after_observation_token and wait_secs (1..=60), waits once for progress or a terminal state; never a subscription.",
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
    ]
}
