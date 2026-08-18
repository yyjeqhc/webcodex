use super::super::input_schemas::{
    job_log_input_schema, job_status_input_schema, list_jobs_input_schema,
    observe_jobs_input_schema, open_session_shell_input_schema, run_detached_process_input_schema,
    run_job_input_schema, run_process_input_schema, run_script_input_schema,
    run_shell_input_schema, session_shell_exec_input_schema, session_shell_identity_input_schema,
    stop_job_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "run_process",
            "Run one native executable with literal argv and no shell parsing. Long work continues as the same execution; choose a shell command tool only when shell syntax or a Windows batch file is required.",
            run_process_input_schema(),
        ),
        tool_spec(
            "run_detached_process",
            "Start a detached process as a durable Job with literal argv and explicit detached authority. A bounded replay key blocks duplicate dispatch while the Job is active or retained; expired keys are not retry tokens. Observe or stop with Job tools. No shell, script, SSH-resource, or retry fallback.",
            run_detached_process_input_schema(),
        ),
        tool_spec(
            "run_script",
            "Run bounded sh, bash, or PowerShell content as typed script data from a Runner-owned file. Long work continues as the same execution; the script body never becomes shell command text.",
            run_script_input_schema(),
        ),
        tool_spec(
            "run_shell",
            "Run one bounded shell command as an escape hatch for real shell syntax. Prefer structured validation, process, and edit tools when they fit; use asynchronous execution for longer work.",
            run_shell_input_schema(),
        ),
        tool_spec(
            "open_session_shell",
            "Open one bounded, long-lived sh/bash process for an active Workflow Session. Its cwd, variables, functions, and umask are isolated from one-shot commands and every other Session.",
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
            "Start one asynchronous shell Job and return its stable job_id. Queued execution keeps that identity; observe the existing Job before considering any retry.".to_string(),
            run_job_input_schema(),
        ),
        tool_spec(
            "stop_job",
            "Stop one existing WebCodex Job by job_id. Requires confirm=true and preserves project/session ownership; log bodies are not returned.",
            stop_job_input_schema(),
        ),
        tool_spec(
            "job_status",
            "Read bounded lifecycle state for one existing Job. Never starts or retries work; command preview is opt-in and log bodies are excluded.",
            job_status_input_schema(),
        ),
        tool_spec(
            "job_log",
            "Read bounded stdout/stderr for one existing Job. With an observation token, wait_secs is one bounded wait; never starts or retries work and never subscribes.",
            job_log_input_schema(),
        ),
        tool_spec(
            "observe_jobs",
            "Observe 1 to 8 existing Jobs with bounded tails and isolated item errors. Optionally wait once for any change; never launches, retries, stops, or subscribes.",
            observe_jobs_input_schema(),
        ),
        tool_spec(
            "list_jobs",
            "List bounded lifecycle metadata for existing Jobs across Runner and local executors; stdout/stderr bodies are never included.".to_string(),
            list_jobs_input_schema(),
        ),
    ]
}
