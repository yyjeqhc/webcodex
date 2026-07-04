use serde_json::Value;

use super::common::{nullable_schema, schema_type, wrapped_output_schema};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "run_shell" => Some(wrapped_output_schema(vec![
            (
                "duration_ms",
                schema_type("integer", "Command duration in milliseconds."),
            ),
            (
                "exit_code",
                nullable_schema("integer", "Process exit code, when available."),
            ),
            ("stdout", schema_type("string", "Captured stdout.")),
            ("stderr", schema_type("string", "Captured stderr.")),
            (
                "stdout_tail",
                schema_type("string", "Bounded stdout tail on failure."),
            ),
            (
                "stderr_tail",
                schema_type("string", "Bounded stderr tail on failure."),
            ),
            (
                "stdout_truncated",
                schema_type("boolean", "Whether stdout_tail was truncated."),
            ),
            (
                "stderr_truncated",
                schema_type("boolean", "Whether stderr_tail was truncated."),
            ),
            (
                "command_started",
                schema_type("boolean", "Whether the command process was started."),
            ),
            (
                "command_completed",
                schema_type(
                    "boolean",
                    "Whether the command reached a terminal result before tool timeout.",
                ),
            ),
            (
                "command_ok",
                schema_type("boolean", "Whether the command completed with exit code 0."),
            ),
            (
                "failure_kind",
                nullable_schema(
                    "string",
                    "Structured failure kind such as command_exit_nonzero, timeout, agent_offline, spawn_failed, permission_denied, tool_schema_error, or runtime_error.",
                ),
            ),
            (
                "tool_failure",
                schema_type(
                    "boolean",
                    "True for WebCodex tool/runtime failures; false for command exit status failures.",
                ),
            ),
        ])),
        "run_job" | "run_codex" => Some(wrapped_output_schema(vec![
            ("job_id", schema_type("string", "Runtime job id.")),
            ("kind", schema_type("string", "Job kind.")),
            ("status", schema_type("string", "Initial job status.")),
            ("project", schema_type("string", "Project id.")),
        ])),
        "stop_job" => Some(wrapped_output_schema(vec![
            (
                "stopped",
                schema_type("boolean", "Compatibility field; true when a stop was requested, already pending, or applied. Prefer stop_effect, terminal, and terminal_pending."),
            ),
            (
                "already_finished",
                schema_type("boolean", "True when the job was already terminal."),
            ),
            (
                "already_stop_requested",
                schema_type("boolean", "True when the job was already stop_requested before this call."),
            ),
            (
                "stop_request_accepted",
                schema_type("boolean", "True when this call requested or applied a stop."),
            ),
            (
                "target_was_active_at_request",
                schema_type("boolean", "True when status_before was running-like or stop_requested."),
            ),
            (
                "terminal",
                schema_type("boolean", "True when status_after is terminal."),
            ),
            (
                "terminal_pending",
                schema_type("boolean", "True when status_after is stop_requested and waiting for terminal status."),
            ),
            (
                "final_status",
                nullable_schema("string", "Terminal final status when terminal=true; null otherwise."),
            ),
            (
                "stop_effect",
                schema_type("string", "Precise stop outcome: requested, stopped, already_finished, already_stop_requested, not_found, forbidden, or confirmation_required."),
            ),
            ("job_id", schema_type("string", "Runtime job id.")),
            ("project", schema_type("string", "Project id.")),
            (
                "status_before",
                schema_type("string", "Job status observed before stop."),
            ),
            (
                "status_after",
                schema_type("string", "Job status after stop/no-op."),
            ),
            (
                "command_started",
                schema_type("boolean", "Always false; stop_job does not start a shell command."),
            ),
            (
                "ownership_basis",
                schema_type("string", "Ownership basis: project_and_session or unknown_session_project_only."),
            ),
        ])),
        "job_status" => Some(wrapped_output_schema(vec![
            ("job_id", schema_type("string", "Runtime job id.")),
            ("project", nullable_schema("string", "Project id, when known.")),
            ("status", schema_type("string", "Current job status.")),
            (
                "exit_code",
                nullable_schema("integer", "Process exit code, when available."),
            ),
            (
                "started_at",
                nullable_schema("integer", "Job start timestamp."),
            ),
            ("ended_at", nullable_schema("integer", "Job end timestamp.")),
            (
                "error",
                nullable_schema("string", "Job error message, when available."),
            ),
            (
                "command_preview_included",
                schema_type("boolean", "True only when include_command_preview=true was requested."),
            ),
            (
                "active",
                schema_type("boolean", "True for blocking active or terminal-pending jobs."),
            ),
            (
                "blocking_active",
                schema_type("boolean", "True for queued, running, started, or agent_queued jobs."),
            ),
            (
                "terminal",
                schema_type("boolean", "True when the job status is terminal."),
            ),
            (
                "terminal_pending",
                schema_type("boolean", "True when status is stop_requested."),
            ),
            (
                "command_preview",
                schema_type(
                    "string",
                    "Bounded command preview only when include_command_preview=true.",
                ),
            ),
            (
                "command_preview_truncated",
                schema_type("boolean", "True when command_preview was truncated to command_preview_max_chars."),
            ),
            (
                "command_preview_max_chars",
                schema_type("integer", "Maximum command preview character count before truncation."),
            ),
            (
                "command_preview_bounded",
                schema_type("boolean", "True when command_preview is bounded; this does not claim secret redaction."),
            ),
        ])),
        "job_log" => Some(wrapped_output_schema(vec![
            ("job_id", schema_type("string", "Runtime job id.")),
            (
                "stdout",
                schema_type("string", "Captured stdout or selected stdout tail."),
            ),
            (
                "stderr",
                schema_type("string", "Captured stderr or selected stderr tail."),
            ),
            (
                "next_stdout_line",
                schema_type("integer", "Next stdout line offset."),
            ),
            (
                "next_stderr_line",
                schema_type("integer", "Next stderr line offset."),
            ),
            (
                "status",
                schema_type("string", "Job status observed with the log."),
            ),
        ])),
        _ => None,
    }
}
