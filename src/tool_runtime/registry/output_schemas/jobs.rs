use serde_json::{json, Value};

use super::common::{
    array_schema, nullable_schema, permission_decision_schema, schema_type, session_hint_schema,
    wrapped_output_schema,
};

fn process_execution_state_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["not_started", "outcome_unknown", "completed", "timed_out", "queued", "running"],
        "description": "Canonical result lifecycle, plus queued/running only when the same execution has been exposed as a durable Job. Only not_started is structurally safe to retry without first inspecting target state."
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
                    {"required": ["promoted_to_job"]},
                    {"required": ["terminal"]},
                    {"required": ["job_id"]},
                    {"required": ["job_status"]},
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
                "anyOf": [
                    {"required": ["promoted_to_job"]},
                    {"required": ["terminal"]},
                    {"required": ["job_id"]},
                    {"required": ["job_status"]},
                    {"required": ["effective_timeout_secs"]},
                    {"required": ["sync_wait_secs"]},
                    {"required": ["async_handoff_available"]}
                ]
            },
            "then": {
                "required": [
                    "promoted_to_job",
                    "terminal",
                    "job_id",
                    "job_status",
                    "effective_timeout_secs",
                    "sync_wait_secs",
                    "async_handoff_available",
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
                    "command_completed": {"const": false},
                    "promoted_to_job": {"const": false},
                    "terminal": {"const": true}
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
                    "command_completed": {"const": false},
                    "terminal": {"const": false}
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
                    "command_completed": {"const": true},
                    "promoted_to_job": {"const": false},
                    "terminal": {"const": true}
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
                    "command_completed": {"const": false},
                    "promoted_to_job": {"const": false},
                    "terminal": {"const": true}
                },
                "required": ["command_started", "command_completed"]
            }
        },
        {
            "if": {
                "properties": {"execution_state": {"const": "queued"}},
                "required": ["execution_state"]
            },
            "then": {
                "properties": {
                    "command_started": {"const": false},
                    "command_completed": {"const": false},
                    "promoted_to_job": {"const": true},
                    "terminal": {"const": false}
                },
                "required": [
                    "command_started",
                    "command_completed",
                    "promoted_to_job",
                    "terminal"
                ]
            }
        },
        {
            "if": {
                "properties": {"execution_state": {"const": "running"}},
                "required": ["execution_state"]
            },
            "then": {
                "properties": {
                    "command_started": {"const": true},
                    "command_completed": {"const": false},
                    "promoted_to_job": {"const": true},
                    "terminal": {"const": false}
                },
                "required": [
                    "command_started",
                    "command_completed",
                    "promoted_to_job",
                    "terminal"
                ]
            }
        },
        {
            "if": {
                "properties": {"promoted_to_job": {"const": true}},
                "required": ["promoted_to_job"]
            },
            "then": {
                "properties": {
                    "job_id": {"type": "string", "minLength": 1},
                    "job_status": {"type": "string", "minLength": 1},
                    "observation_token": {"type": "string", "minLength": 1},
                    "terminal": {"const": false},
                    "command_completed": {"const": false},
                    "async_handoff_available": {"const": true},
                    "execution_state": {
                        "enum": ["queued", "running", "outcome_unknown"]
                    }
                },
                "required": [
                    "job_id",
                    "job_status",
                    "observation_token",
                    "terminal",
                    "command_completed",
                    "async_handoff_available"
                ]
            }
        },
        {
            "if": {
                "properties": {"promoted_to_job": {"const": false}},
                "required": ["promoted_to_job"]
            },
            "then": {
                "properties": {
                    "job_id": {"type": "null"},
                    "job_status": {"type": "null"},
                    "execution_state": {
                        "enum": ["not_started", "outcome_unknown", "completed", "timed_out"]
                    }
                },
                "required": ["job_id", "job_status"]
            }
        }
    ])
}

fn structured_continuation_properties() -> Vec<(&'static str, Value)> {
    vec![
        (
            "promoted_to_job",
            schema_type(
                "boolean",
                "True only when the same original execution continues as a durable Job.",
            ),
        ),
        (
            "terminal",
            schema_type(
                "boolean",
                "Whether a trustworthy terminal projection is available from this tool call.",
            ),
        ),
        (
            "job_id",
            nullable_schema(
                "string",
                "Durable continuation Job id, or null when no Job was exposed.",
            ),
        ),
        (
            "job_status",
            nullable_schema(
                "string",
                "Authoritative durable Job status, or null when no Job was exposed.",
            ),
        ),
        (
            "observation_token",
            nullable_schema(
                "string",
                "Current Job observation token when a continuation was exposed.",
            ),
        ),
        (
            "effective_timeout_secs",
            schema_type(
                "integer",
                "Total execution budget in seconds, beginning with the one original execution.",
            ),
        ),
        (
            "sync_wait_secs",
            schema_type(
                "integer",
                "Internal synchronous grace in seconds; this is not a second timeout budget.",
            ),
        ),
        (
            "async_handoff_available",
            schema_type(
                "boolean",
                "Whether this Runner can continue typed structured execution as the same durable Job.",
            ),
        ),
    ]
}

fn job_command_execution_state_schema() -> Value {
    json!({
        "anyOf": [
            {
                "type": "string",
                "enum": ["not_started", "outcome_unknown", "timed_out", "completed"]
            },
            {"type": "null"}
        ],
        "description": "Phase-A terminal lifecycle for a typed structured execution Job; null for active or legacy shell Jobs."
    })
}

fn job_structured_execution_metadata_schema() -> Value {
    json!({
        "anyOf": [
            {
                "type": "object",
                "properties": {
                    "execution_source": {
                        "type": "string",
                        "enum": ["run_process", "run_script"]
                    },
                    "language": {
                        "anyOf": [
                            {"type": "string", "enum": ["sh", "bash", "powershell"]},
                            {"type": "null"}
                        ]
                    },
                    "script_bytes": nullable_schema("integer", "Script byte count for run_script."),
                    "arg_count": schema_type("integer", "Typed argument count."),
                    "stdin_present": schema_type("boolean", "Whether independent typed stdin was present.")
                },
                "required": ["execution_source", "arg_count", "stdin_present"],
                "additionalProperties": false
            },
            {"type": "null"}
        ],
        "description": "Safe bounded structured-execution metadata. Raw process argv, script content, script args, and stdin are never present."
    })
}

fn observe_jobs_output_schema() -> Value {
    let job_observation = json!({
        "type": "object",
        "additionalProperties": true,
        "properties": {
            "job_id": schema_type("string", "Runtime Job id."),
            "status": schema_type("string", "Canonical current Job status."),
            "exit_code": nullable_schema("integer", "Process exit code, when terminal and available."),
            "command_execution_state": job_command_execution_state_schema(),
            "structured_execution": job_structured_execution_metadata_schema(),
            "stdout_tail": schema_type("string", "Bounded stdout tail."),
            "stderr_tail": schema_type("string", "Bounded stderr tail."),
            "stdout_lines": schema_type("integer", "Total observed stdout line count."),
            "stderr_lines": schema_type("integer", "Total observed stderr line count."),
            "stdout_truncated": schema_type("boolean", "Whether stdout_tail omits observed output."),
            "stderr_truncated": schema_type("boolean", "Whether stderr_tail omits observed output."),
            "stdout_retained_from_line": nullable_schema("integer", "First retained absolute stdout line, when available."),
            "stderr_retained_from_line": nullable_schema("integer", "First retained absolute stderr line, when available."),
            "earlier_stdout_unavailable": schema_type("boolean", "Whether earlier stdout is outside retained bounded logs."),
            "earlier_stderr_unavailable": schema_type("boolean", "Whether earlier stderr is outside retained bounded logs."),
            "recovery_state": nullable_schema("string", "Canonical bounded recovery state."),
            "recovery_reason_code": nullable_schema("string", "Canonical bounded recovery reason code."),
            "recovery_reason": nullable_schema("string", "Canonical bounded recovery explanation."),
            "observation_token": schema_type("string", "Opaque Job-bound token for this returned snapshot."),
            "last_update_seq": nullable_schema("integer", "Agent protocol diagnostic sequence, when available."),
            "cursor": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "stdout": {"type": "integer", "minimum": 1},
                    "stderr": {"type": "integer", "minimum": 1}
                },
                "required": ["stdout", "stderr"]
            },
            "wait_outcome": schema_type("string", "Canonical per-Job internal observation outcome; outer wake_reason is the batch wait fact."),
            "waited_ms": schema_type("integer", "Canonical per-Job internal wait time. Final batch snapshots are non-waiting."),
            "changed": schema_type("boolean", "Whether this snapshot differs from the item's supplied token."),
            "terminal": schema_type("boolean", "Canonical terminal classification."),
            "executor": {
                "type": "string",
                "enum": ["local", "agent"]
            },
            "session_id": nullable_schema("string", "Owning Workflow Session, when recorded."),
            "ssh_resource": nullable_schema("string", "Named SSH resource, when recorded."),
            "cwd": nullable_schema("string", "Recorded working directory."),
            "shell": nullable_schema("string", "Recorded execution shell or executor."),
            "purpose": nullable_schema("string", "Declared execution purpose."),
            "command_summary": nullable_schema("string", "Bounded safe command summary."),
            "detected_summary": {
                "type": "object",
                "additionalProperties": true
            },
            "validation": {
                "anyOf": [
                    {"type": "object", "additionalProperties": true},
                    {"type": "null"}
                ]
            }
        },
        "required": [
            "job_id", "status", "exit_code", "stdout_tail", "stderr_tail",
            "stdout_lines", "stderr_lines", "stdout_truncated", "stderr_truncated",
            "observation_token", "cursor", "wait_outcome", "waited_ms", "changed",
            "terminal", "executor", "cwd", "shell", "purpose", "command_summary",
            "detected_summary", "validation"
        ]
    });
    let item = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "index": {"type": "integer", "minimum": 0, "maximum": 7},
            "job_id": {"type": "string", "minLength": 1},
            "success": {"type": "boolean"},
            "output": {"anyOf": [job_observation.clone(), {"type": "null"}]},
            "error_kind": {"anyOf": [{"type": "string"}, {"type": "null"}]},
            "error": {"anyOf": [{"type": "string"}, {"type": "null"}]}
        },
        "required": ["index", "job_id", "success", "output", "error_kind", "error"],
        "allOf": [{
            "if": {"properties": {"success": {"const": true}}, "required": ["success"]},
            "then": {
                "properties": {
                    "output": job_observation,
                    "error_kind": {"type": "null"},
                    "error": {"type": "null"}
                }
            },
            "else": {
                "properties": {
                    "output": {"type": "null"},
                    "error_kind": {"type": "string"},
                    "error": {"type": "string"}
                }
            }
        }]
    });
    let batch_output = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "requested_count": {"type": "integer", "minimum": 1, "maximum": 8},
            "returned_count": {"type": "integer", "minimum": 0, "maximum": 8},
            "succeeded_count": {"type": "integer", "minimum": 0, "maximum": 8},
            "failed_count": {"type": "integer", "minimum": 0, "maximum": 8},
            "items": {"type": "array", "maxItems": 8, "items": item},
            "wake_reason": {
                "type": "string",
                "enum": ["immediate", "updated", "terminal", "item_error", "timeout"]
            },
            "waited_ms": {"type": "integer", "minimum": 0},
            "changed_count": {"type": "integer", "minimum": 0, "maximum": 8},
            "terminal_count": {"type": "integer", "minimum": 0, "maximum": 8},
            "output_truncated": {"type": "boolean"},
            "next_index": {"anyOf": [{"type": "integer", "minimum": 0, "maximum": 7}, {"type": "null"}]},
            "session_recorded": schema_type("boolean", "True when this batch call was recorded."),
            "session_id": schema_type("string", "Session id used for outer batch telemetry."),
            "session_event_id": schema_type("string", "Outer batch Session event id."),
            "session_hint": session_hint_schema(),
            "permission": permission_decision_schema()
        },
        "required": [
            "requested_count", "returned_count", "succeeded_count", "failed_count",
            "items", "wake_reason", "waited_ms", "changed_count", "terminal_count",
            "output_truncated", "next_index"
        ]
    });
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "success": {"type": "boolean"},
            "output": {"anyOf": [batch_output.clone(), {"type": "object", "additionalProperties": true}, {"type": "null"}]},
            "error": {"anyOf": [{"type": "string"}, {"type": "null"}]}
        },
        "required": ["success", "output"],
        "allOf": [{
            "if": {"properties": {"success": {"const": true}}, "required": ["success"]},
            "then": {"properties": {"output": batch_output, "error": {"type": "null"}}},
            "else": {"required": ["error"], "properties": {"error": {"type": "string"}}}
        }]
    })
}

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "run_process" => {
            let mut properties = vec![
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
            ];
            properties.extend(structured_continuation_properties());
            let mut schema = wrapped_output_schema(properties);
            schema["properties"]["output"]["properties"]["execution_source"]["const"] =
                json!("run_process");
            schema["properties"]["output"]["allOf"] =
                structured_execution_lifecycle_constraints("run_process");
            Some(schema)
        }
        "run_script" => {
            let mut properties = vec![
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
            ];
            properties.extend(structured_continuation_properties());
            let mut schema = wrapped_output_schema(properties);
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
                "command_execution_state",
                job_command_execution_state_schema(),
            ),
            (
                "structured_execution",
                job_structured_execution_metadata_schema(),
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
        "observe_jobs" => Some(observe_jobs_output_schema()),
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
                "command_execution_state",
                job_command_execution_state_schema(),
            ),
            (
                "structured_execution",
                job_structured_execution_metadata_schema(),
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
            "command_execution_state": job_command_execution_state_schema(),
            "structured_execution": job_structured_execution_metadata_schema(),
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
