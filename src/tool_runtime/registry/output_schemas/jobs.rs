use serde_json::{json, Value};

use super::common::{array_schema, nullable_schema, schema_type, wrapped_output_schema};

fn process_execution_state_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["not_started", "outcome_unknown", "completed", "timed_out"],
        "description": "Canonical lifecycle state. Only not_started is structurally safe to retry without first inspecting target state."
    })
}

fn structured_execution_lifecycle_constraints(execution_source: &str) -> Value {
    json!([
        {
            "if": {
                "anyOf": [
                    {"required": ["execution_state"]},
                    {"required": ["command_started"]},
                    {"required": ["command_completed"]},
                    {"required": ["command_ok"]},
                    {"required": ["failure_kind"]},
                    {"required": ["tool_failure"]},
                    {
                        "properties": {
                            "execution_source": {"const": execution_source}
                        },
                        "required": ["execution_source"]
                    }
                ]
            },
            "then": {
                "required": [
                    "execution_state",
                    "command_started",
                    "command_completed"
                ]
            }
        },
        {
            "if": {
                "properties": {"execution_state": {"const": "not_started"}},
                "required": ["execution_state"]
            },
            "then": {
                "properties": {
                    "command_started": {"const": false},
                    "command_completed": {"const": false}
                },
                "required": ["command_started", "command_completed"]
            }
        },
        {
            "if": {
                "properties": {"execution_state": {"const": "outcome_unknown"}},
                "required": ["execution_state"]
            },
            "then": {
                "properties": {
                    "command_started": {"const": true},
                    "command_completed": {"const": false}
                },
                "required": ["command_started", "command_completed"]
            }
        },
        {
            "if": {
                "properties": {"execution_state": {"const": "completed"}},
                "required": ["execution_state"]
            },
            "then": {
                "properties": {
                    "command_started": {"const": true},
                    "command_completed": {"const": true}
                },
                "required": ["command_started", "command_completed"]
            }
        },
        {
            "if": {
                "properties": {"execution_state": {"const": "timed_out"}},
                "required": ["execution_state"]
            },
            "then": {
                "properties": {
                    "command_started": {"const": true},
                    "command_completed": {"const": false}
                },
                "required": ["command_started", "command_completed"]
            }
        }
    ])
}

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "run_process" => {
            let mut schema = wrapped_output_schema(vec![
                (
                    "duration_ms",
                    schema_type("integer", "Process duration in milliseconds."),
                ),
                (
                    "exit_code",
                    nullable_schema("integer", "Process exit code, when available."),
                ),
                (
                    "stdout_tail",
                    schema_type("string", "Bounded stdout tail."),
                ),
                (
                    "stderr_tail",
                    schema_type("string", "Bounded stderr tail."),
                ),
                (
                    "stdout_lines",
                    schema_type("integer", "Captured stdout line count."),
                ),
                (
                    "stderr_lines",
                    schema_type("integer", "Captured stderr line count."),
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
                    schema_type(
                        "boolean",
                        "Whether callers must conservatively treat the process as started; true includes outcome_unknown because side effects may have occurred.",
                    ),
                ),
                (
                    "command_completed",
                    schema_type(
                        "boolean",
                        "Whether the process reached a terminal result before tool timeout.",
                    ),
                ),
                (
                    "command_ok",
                    schema_type("boolean", "Whether the process completed with exit code 0."),
                ),
                (
                    "failure_kind",
                    nullable_schema(
                        "string",
                        "Structured failure kind such as command_exit_nonzero, timeout, outcome_unknown, capability_unavailable, unsupported_resource, unsupported_executable_type, spawn_failed, permission_denied, invalid_arguments, agent_offline, session_guard_denied, session_closed, or runtime_error.",
                    ),
                ),
                (
                    "tool_failure",
                    schema_type(
                        "boolean",
                        "True for WebCodex tool/runtime failures; false for child exit status failures.",
                    ),
                ),
                ("purpose", schema_type("string", "Declared execution purpose.")),
                (
                    "process_summary",
                    schema_type(
                        "string",
                        "Bounded human-readable executable/argv summary; never execution input.",
                    ),
                ),
                (
                    "cwd",
                    schema_type("string", "Resolved project-relative cwd."),
                ),
                ("executor", schema_type("string", "Executor type: local or agent.")),
                (
                    "execution_source",
                    schema_type("string", "Always run_process."),
                ),
                (
                    "execution_state",
                    process_execution_state_schema(),
                ),
            ]);
            schema["properties"]["output"]["properties"]["execution_source"]["const"] =
                json!("run_process");
            schema["properties"]["output"]["allOf"] =
                structured_execution_lifecycle_constraints("run_process");
            Some(schema)
        }
        "run_script" => {
            let mut schema = wrapped_output_schema(vec![
                (
                    "duration_ms",
                    schema_type("integer", "Script duration in milliseconds."),
                ),
                (
                    "exit_code",
                    nullable_schema("integer", "Interpreter exit code, when available."),
                ),
                (
                    "stdout_tail",
                    schema_type("string", "Bounded stdout tail."),
                ),
                (
                    "stderr_tail",
                    schema_type("string", "Bounded stderr tail."),
                ),
                (
                    "stdout_lines",
                    schema_type("integer", "Captured stdout line count."),
                ),
                (
                    "stderr_lines",
                    schema_type("integer", "Captured stderr line count."),
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
                    schema_type(
                        "boolean",
                        "Whether callers must conservatively treat the script as started; true includes outcome_unknown because side effects may have occurred.",
                    ),
                ),
                (
                    "command_completed",
                    schema_type(
                        "boolean",
                        "Whether the interpreter reached a known terminal result before tool timeout.",
                    ),
                ),
                (
                    "command_ok",
                    schema_type(
                        "boolean",
                        "Whether the interpreter completed with exit code 0.",
                    ),
                ),
                (
                    "failure_kind",
                    nullable_schema(
                        "string",
                        "Structured failure kind such as command_exit_nonzero, timeout, outcome_unknown, capability_unavailable, unsupported_resource, interpreter_unavailable, script_setup_failed, permission_denied, invalid_arguments, agent_offline, session_guard_denied, session_closed, or runtime_error.",
                    ),
                ),
                (
                    "tool_failure",
                    schema_type(
                        "boolean",
                        "True for WebCodex tool/runtime failures; false for interpreter exit status failures.",
                    ),
                ),
                ("purpose", schema_type("string", "Declared execution purpose.")),
                (
                    "script_summary",
                    schema_type(
                        "string",
                        "Bounded body-free language/byte/argument summary; never execution input.",
                    ),
                ),
                (
                    "language",
                    schema_type("string", "Explicit semantic script language."),
                ),
                (
                    "cwd",
                    schema_type("string", "Resolved project-relative cwd."),
                ),
                ("executor", schema_type("string", "Executor type: local or agent.")),
                (
                    "execution_source",
                    schema_type("string", "Always run_script."),
                ),
                (
                    "execution_state",
                    process_execution_state_schema(),
                ),
            ]);
            schema["properties"]["output"]["properties"]["language"]["enum"] =
                json!(["sh", "bash", "powershell"]);
            schema["properties"]["output"]["properties"]["execution_source"]["const"] =
                json!("run_script");
            schema["properties"]["output"]["allOf"] =
                structured_execution_lifecycle_constraints("run_script");
            Some(schema)
        }
        "run_shell" => Some(wrapped_output_schema(vec![
            (
                "duration_ms",
                schema_type("integer", "Command duration in milliseconds."),
            ),
            (
                "exit_code",
                nullable_schema("integer", "Process exit code, when available."),
            ),
            (
                "stdout_tail",
                schema_type("string", "Bounded stdout tail."),
            ),
            (
                "stderr_tail",
                schema_type("string", "Bounded stderr tail."),
            ),
            (
                "stdout_lines",
                schema_type("integer", "Total captured stdout line count."),
            ),
            (
                "stderr_lines",
                schema_type("integer", "Total captured stderr line count."),
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
                schema_type(
                    "boolean",
                    "Whether callers must conservatively treat the command as started; true includes outcome_unknown because side effects may have occurred.",
                ),
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
                    "Structured failure kind such as command_exit_nonzero, timeout, outcome_unknown, agent_offline, spawn_failed, permission_denied, tool_schema_error, or runtime_error.",
                ),
            ),
            (
                "tool_failure",
                schema_type(
                    "boolean",
                    "True for WebCodex tool/runtime failures; false for command exit status failures.",
                ),
            ),
            ("purpose", schema_type("string", "Declared execution purpose.")),
            (
                "command_summary",
                schema_type("string", "Bounded first-line command summary."),
            ),
            (
                "cwd",
                schema_type(
                    "string",
                    "Resolved project-relative cwd, or the selected SSH resource's remote cwd.",
                ),
            ),
            (
                "shell",
                schema_type(
                    "string",
                    "Actual selected shell, configured executor shell, or remote SSH executor.",
                ),
            ),
            ("executor", schema_type("string", "Executor type: local or agent.")),
            (
                "ssh_resource",
                nullable_schema(
                    "string",
                    "Named Runner-local SSH resource used for this command, when any.",
                ),
            ),
            (
                "execution_state",
                schema_type(
                    "string",
                    "Canonical lifecycle state: not_started, outcome_unknown, completed, or timed_out. Only not_started is structurally safe to retry without first inspecting target state.",
                ),
            ),
        ])),
        "open_session_shell"
        | "session_shell_exec"
        | "session_shell_status"
        | "close_session_shell" => Some(persistent_shell_output_schema()),
        "run_job" => Some(wrapped_output_schema(vec![
            ("job_id", schema_type("string", "Runtime job id.")),
            ("kind", schema_type("string", "Job kind.")),
            ("status", schema_type("string", "Initial job status.")),
            ("project", schema_type("string", "Project id.")),
            ("purpose", schema_type("string", "Declared execution purpose.")),
            (
                "command_summary",
                schema_type("string", "Bounded first-line command summary."),
            ),
            (
                "cwd",
                schema_type(
                    "string",
                    "Resolved project-relative cwd, or the selected SSH resource's remote cwd.",
                ),
            ),
            (
                "shell",
                schema_type(
                    "string",
                    "Selected shell, configured executor shell, or remote SSH executor.",
                ),
            ),
            ("executor", schema_type("string", "Executor type: local or agent.")),
            (
                "ssh_resource",
                nullable_schema(
                    "string",
                    "Named Runner-local SSH resource used for this job, when any.",
                ),
            ),
            (
                "execution_state",
                schema_type("string", "Initial execution state; started after acceptance."),
            ),
            (
                "created_at",
                schema_type("integer", "Job creation timestamp."),
            ),
            (
                "observation_token",
                schema_type("string", "Opaque Job-bound observation token. Return it unchanged as after_observation_token for one bounded wait."),
            ),
            (
                "last_update_seq",
                nullable_schema("integer", "Agent Runner protocol diagnostic sequence; not a bounded-wait token. Omitted for local jobs."),
            ),
        ])),
        "list_jobs" => Some(wrapped_output_schema(vec![
            (
                "jobs",
                array_schema(
                    job_summary_schema(),
                    "Bounded job summaries; never includes stdout or stderr bodies.",
                ),
            ),
            ("count", schema_type("integer", "Returned job summary count.")),
            (
                "truncated",
                schema_type(
                    "boolean",
                    "Whether the collected job summaries exceeded the returned limit.",
                ),
            ),
        ])),
        "stop_job" => Some(wrapped_output_schema(vec![
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
            (
                "session_id",
                nullable_schema("string", "Workflow Session that owns this job, when recorded."),
            ),
            (
                "ssh_resource",
                nullable_schema(
                    "string",
                    "Named Runner-local SSH resource used for this job, when any.",
                ),
            ),
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
                "recovery_state",
                nullable_schema("string", "Bounded recovery state such as recovering or reconciled."),
            ),
            (
                "recovered_after_server_restart",
                schema_type("boolean", "True when this record was rebuilt from a same-runner inventory."),
            ),
            (
                "reconciled_at",
                nullable_schema("integer", "Latest same-instance reconciliation timestamp."),
            ),
            (
                "recovery_reason_code",
                nullable_schema("string", "Structured bounded recovery reason code."),
            ),
            (
                "observation_token",
                schema_type("string", "Opaque Job-bound observation token for the current public snapshot."),
            ),
            (
                "last_update_seq",
                nullable_schema("integer", "Agent Runner protocol diagnostic sequence; not a bounded-wait token. Omitted for local jobs."),
            ),
            (
                "stdout_retained_from_line",
                nullable_schema("integer", "First retained absolute stdout line."),
            ),
            (
                "stderr_retained_from_line",
                nullable_schema("integer", "First retained absolute stderr line."),
            ),
            (
                "stdout_log_truncated",
                schema_type("boolean", "True when the retained stdout begins after discarded bytes."),
            ),
            (
                "stderr_log_truncated",
                schema_type("boolean", "True when the retained stderr begins after discarded bytes."),
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
                schema_type("boolean", "True when command_preview is bounded and secret-like command text is replaced with [redacted]."),
            ),
        ])),
        "job_log" | "job_tail" => Some(wrapped_output_schema(vec![
            ("job_id", schema_type("string", "Runtime job id.")),
            (
                "session_id",
                nullable_schema("string", "Workflow Session that owns this job, when recorded."),
            ),
            (
                "ssh_resource",
                nullable_schema(
                    "string",
                    "Named Runner-local SSH resource used for this job, when any.",
                ),
            ),
            (
                "wait_outcome",
                schema_type(
                    "string",
                    "Bounded wait outcome: immediate (no wait or state already available), updated (non-terminal update after waiting), terminal (job terminal), or timeout (wait elapsed with no observable change; a normal result, not an error).",
                ),
            ),
            (
                "waited_ms",
                schema_type("integer", "Milliseconds actually spent waiting, when a bounded wait was requested."),
            ),
            (
                "changed",
                schema_type("boolean", "Whether the current observation_token differs from the supplied after_observation_token."),
            ),
            (
                "terminal",
                schema_type("boolean", "Whether the job is in a terminal state per the canonical job terminal definition."),
            ),
            (
                "exit_code",
                nullable_schema("integer", "Process exit code, when available."),
            ),
            (
                "stdout_tail",
                schema_type("string", "Bounded stdout tail or cursor segment."),
            ),
            (
                "stderr_tail",
                schema_type("string", "Bounded stderr tail or cursor segment."),
            ),
            (
                "stdout_lines",
                schema_type("integer", "Total observed stdout line count."),
            ),
            (
                "stderr_lines",
                schema_type("integer", "Total observed stderr line count."),
            ),
            (
                "stdout_truncated",
                schema_type("boolean", "Whether stdout_tail omits observed lines."),
            ),
            (
                "stderr_truncated",
                schema_type("boolean", "Whether stderr_tail omits observed lines."),
            ),
            (
                "stdout_retained_from_line",
                nullable_schema("integer", "First retained absolute stdout line."),
            ),
            (
                "stderr_retained_from_line",
                nullable_schema("integer", "First retained absolute stderr line."),
            ),
            (
                "earlier_stdout_unavailable",
                schema_type("boolean", "True when earlier stdout is outside the bounded retained tail."),
            ),
            (
                "earlier_stderr_unavailable",
                schema_type("boolean", "True when earlier stderr is outside the bounded retained tail."),
            ),
            (
                "recovery_state",
                nullable_schema("string", "Bounded recovery state such as recovering or reconciled."),
            ),
            (
                "recovery_reason_code",
                nullable_schema("string", "Structured bounded recovery reason code."),
            ),
            (
                "observation_token",
                schema_type("string", "Opaque Job-bound observation token for the returned status and frozen log snapshot."),
            ),
            (
                "last_update_seq",
                nullable_schema("integer", "Agent Runner protocol diagnostic sequence; not a bounded-wait token. Omitted for local jobs."),
            ),
            (
                "cursor",
                super::common::open_object_schema(
                    "Next 1-based stdout/stderr cursors for bounded continuation.",
                ),
            ),
            (
                "status",
                schema_type("string", "Job status observed with the log."),
            ),
            (
                "executor",
                schema_type("string", "Executor backing the job: local or agent."),
            ),
            (
                "cwd",
                nullable_schema(
                    "string",
                    "Resolved project-relative cwd or remote SSH cwd, when recorded.",
                ),
            ),
            (
                "shell",
                nullable_schema(
                    "string",
                    "Selected shell, configured executor shell, or remote SSH executor.",
                ),
            ),
            (
                "purpose",
                nullable_schema("string", "Declared execution purpose, when recorded."),
            ),
            (
                "command_summary",
                nullable_schema("string", "Bounded first-line command summary, when recorded."),
            ),
            (
                "detected_summary",
                super::common::open_object_schema(
                    "Compact detected operation/build/check/test summary from bounded evidence.",
                ),
            ),
        ])),
        _ => None,
    }
}

fn persistent_shell_output_schema() -> Value {
    wrapped_output_schema(vec![
        (
            "shell_id",
            schema_type("string", "Opaque persistent shell id."),
        ),
        (
            "project",
            schema_type("string", "Exact runtime project id."),
        ),
        (
            "session_id",
            schema_type("string", "Owning Workflow Session id."),
        ),
        (
            "executor",
            schema_type("string", "Process host class: local or agent."),
        ),
        (
            "shell",
            nullable_schema("string", "Long-lived shell dialect: sh or bash."),
        ),
        (
            "profile",
            nullable_schema("string", "Runner shell profile applied once at open."),
        ),
        (
            "initial_cwd",
            nullable_schema("string", "Project-relative cwd selected at open."),
        ),
        (
            "cwd",
            nullable_schema("string", "Safely observed current project-relative cwd."),
        ),
        (
            "created_at",
            nullable_schema("integer", "Shell creation timestamp."),
        ),
        (
            "last_activity_at",
            nullable_schema("integer", "Last shell activity timestamp."),
        ),
        (
            "shell_state",
            schema_type(
                "string",
                "opening, running, exited, closed, poisoned, lost, or unknown.",
            ),
        ),
        (
            "execution_state",
            schema_type("string", "Lifecycle or command execution state."),
        ),
        (
            "command_started",
            schema_type(
                "boolean",
                "Whether this exec command was written to the shell.",
            ),
        ),
        (
            "command_completed",
            schema_type(
                "boolean",
                "Whether command completion was established by a verified frame or authoritative shell exit.",
            ),
        ),
        (
            "command_ok",
            schema_type("boolean", "Whether a completed command exited with code 0."),
        ),
        (
            "exit_code",
            nullable_schema("integer", "Command or shell exit code, when known."),
        ),
        ("stdout", schema_type("string", "Bounded command stdout.")),
        ("stderr", schema_type("string", "Bounded command stderr.")),
        (
            "stdout_truncated",
            schema_type("boolean", "Whether command stdout exceeded the bound."),
        ),
        (
            "stderr_truncated",
            schema_type("boolean", "Whether command stderr exceeded the bound."),
        ),
        (
            "duration_ms",
            schema_type("integer", "Command duration in milliseconds."),
        ),
        (
            "busy",
            schema_type("boolean", "Whether a command is in flight."),
        ),
        (
            "already_closed",
            schema_type(
                "boolean",
                "Whether close observed an already-terminal shell.",
            ),
        ),
        (
            "close_reason",
            nullable_schema("string", "Bounded terminal reason."),
        ),
        (
            "purpose",
            nullable_schema("string", "Declared execution intent for this command."),
        ),
        (
            "error_code",
            nullable_schema("string", "Structured persistent-shell error code."),
        ),
        (
            "tool_failure",
            schema_type(
                "boolean",
                "Whether WebCodex rejected or lost the operation.",
            ),
        ),
    ])
}

fn job_summary_schema() -> Value {
    json!({
        "type": "object",
        "description": "Bounded job metadata summary. Does not include stdout, stderr, command text, or log bodies.",
        "properties": {
            "job_id": schema_type("string", "Runtime job id."),
            "kind": schema_type("string", "Job kind."),
            "status": schema_type("string", "Current job status."),
            "project": nullable_schema("string", "Project id, when known."),
            "session_id": nullable_schema("string", "Workflow Session that owns this job, when recorded."),
            "ssh_resource": nullable_schema("string", "Named Runner-local SSH resource used for this job, when any."),
            "executor": schema_type("string", "Executor backing this job, such as agent or local."),
            "client_id": nullable_schema("string", "Agent client id for agent-backed jobs, when available."),
            "created_at": schema_type("integer", "Job creation timestamp."),
            "started_at": nullable_schema("integer", "Job start timestamp, when available."),
            "ended_at": nullable_schema("integer", "Job end timestamp, when available."),
            "duration_ms": nullable_schema("integer", "Job duration in milliseconds, when available."),
            "elapsed_secs": nullable_schema("integer", "Elapsed job runtime in seconds, when available."),
            "exit_code": nullable_schema("integer", "Process exit code, when available."),
            "recovery_state": nullable_schema("string", "Bounded recovery state, when applicable."),
            "recovered_after_server_restart": schema_type("boolean", "True when rebuilt from a same-runner inventory."),
            "reconciled_at": nullable_schema("integer", "Latest reconciliation timestamp."),
            "recovery_reason_code": nullable_schema("string", "Structured bounded recovery reason code."),
            "last_update_seq": nullable_schema("integer", "Latest accepted runner update sequence.")
        },
        "required": [
            "job_id",
            "kind",
            "status",
            "project",
            "executor",
            "created_at",
            "started_at",
            "ended_at",
            "exit_code"
        ],
        "additionalProperties": true
    })
}
