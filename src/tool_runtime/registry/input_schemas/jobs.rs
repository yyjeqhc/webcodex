use serde_json::{json, Value};

use super::common::{object_schema, with_optional_session_id};
use crate::shell_protocol::{
    PROCESS_ARG_MAX_BYTES, PROCESS_ARG_MAX_COUNT, PROCESS_CWD_MAX_BYTES,
    PROCESS_EXECUTABLE_MAX_BYTES, PROCESS_STDIN_MAX_BYTES, PROCESS_TIMEOUT_MAX_SECS,
};

pub(crate) fn run_process_input_schema() -> Value {
    let mut schema = object_schema(with_optional_session_id(vec![
        ("project", "string", "Configured project id.", true),
        (
            "executable",
            "string",
            "Native executable name or path. It is executed directly and is never parsed as shell text. On Windows, .cmd/.bat files are rejected because they require shell/script semantics; use run_shell explicitly when those semantics are intended.",
            true,
        ),
        (
            "args",
            "array",
            "Ordered argv values passed literally to the child process. Defaults to an empty array. Runtime validation allows at most 16,000 UTF-8 bytes across executable and argv boundaries.",
            false,
        ),
        (
            "cwd",
            "string",
            "Project-relative working directory. Omit, empty string, or '.' for the project root. Named Session SSH resources are unsupported for run_process.",
            false,
        ),
        (
            "stdin",
            "string",
            "Optional bounded UTF-8 stdin payload passed through a pipe.",
            false,
        ),
        (
            "timeout_secs",
            "integer",
            "Synchronous process timeout in seconds (minimum 1, maximum 120, default 60).",
            false,
        ),
        (
            "purpose",
            "string",
            "Declared execution intent: validation, test, build, format, release, diagnostic, operation, or other. This records evidence and never changes authorization.",
            false,
        ),
    ]));
    schema["properties"]["executable"]["minLength"] = json!(1);
    schema["properties"]["executable"]["maxLength"] = json!(PROCESS_EXECUTABLE_MAX_BYTES);
    schema["properties"]["args"]["maxItems"] = json!(PROCESS_ARG_MAX_COUNT);
    schema["properties"]["args"]["items"]["maxLength"] = json!(PROCESS_ARG_MAX_BYTES);
    schema["properties"]["args"]["default"] = json!([]);
    schema["properties"]["stdin"] = json!({
        "anyOf": [
            {"type": "string", "maxLength": PROCESS_STDIN_MAX_BYTES},
            {"type": "null"}
        ],
        "description": "Optional bounded UTF-8 stdin payload passed through a pipe; null and omission both mean no stdin payload."
    });
    schema["properties"]["cwd"]["maxLength"] = json!(PROCESS_CWD_MAX_BYTES);
    schema["properties"]["timeout_secs"]["minimum"] = json!(1);
    schema["properties"]["timeout_secs"]["maximum"] = json!(PROCESS_TIMEOUT_MAX_SECS);
    schema["properties"]["timeout_secs"]["default"] = json!(60);
    schema["properties"]["purpose"]["enum"] = json!([
        "validation",
        "test",
        "build",
        "format",
        "release",
        "diagnostic",
        "operation",
        "other"
    ]);
    schema
}

pub(crate) fn run_shell_input_schema() -> Value {
    let mut schema = object_schema(with_optional_session_id(vec![
        ("project", "string", "Configured project id.", true),
        ("command", "string", "Shell command to run.", true),
        (
            "timeout_secs",
            "integer",
            "Synchronous command timeout in seconds (minimum 1, maximum 120, default 60). Out-of-range values are rejected before the command starts; use run_job for longer work.",
            false,
        ),
        (
            "cwd",
            "string",
            "Working directory contract: without a Session SSH resource, omit, empty string, or '.' selects the project root and any other value is project-relative. With a named Session SSH resource, cwd is a remote path checked by the remote shell instead of the Runner project-root policy.",
            false,
        ),
        (
            "purpose",
            "string",
            "Declared execution intent: validation, test, build, format, release, diagnostic, operation, or other. This records evidence and never changes authorization.",
            false,
        ),
        (
            "shell",
            "string",
            "Optional explicit command language: sh or bash. When omitted, local run_shell uses sh, an agent-backed run_shell uses that Agent's configured shell, and a named Session SSH resource uses the remote login shell. The response always records the actual selection.",
            false,
        ),
    ]));
    schema["properties"]["purpose"]["enum"] = json!([
        "validation",
        "test",
        "build",
        "format",
        "release",
        "diagnostic",
        "operation",
        "other"
    ]);
    schema["properties"]["shell"]["enum"] = json!(["sh", "bash"]);
    schema["properties"]["timeout_secs"]["minimum"] = json!(1);
    schema["properties"]["timeout_secs"]["maximum"] = json!(120);
    schema["properties"]["timeout_secs"]["default"] = json!(60);
    schema
}

pub(crate) fn run_job_input_schema() -> Value {
    let mut schema = object_schema(with_optional_session_id(vec![
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
            "Working directory contract: without a Session SSH resource, omit, empty string, or '.' selects the project root and any other value is project-relative. With a named Session SSH resource, cwd is a remote path checked by the remote shell instead of the Runner project-root policy.",
            false,
        ),
        (
            "purpose",
            "string",
            "Declared execution intent: validation, test, build, format, release, diagnostic, operation, or other. This records evidence and never changes authorization.",
            false,
        ),
        (
            "shell",
            "string",
            "Optional explicit command language: sh or bash. When omitted, local run_job preserves its existing bash contract, an agent-backed run_job uses that Agent's configured shell, and a named Session SSH resource uses the remote login shell. The response always records the actual selection.",
            false,
        ),
    ]));
    schema["properties"]["purpose"]["enum"] = json!([
        "validation",
        "test",
        "build",
        "format",
        "release",
        "diagnostic",
        "operation",
        "other"
    ]);
    schema["properties"]["shell"]["enum"] = json!(["sh", "bash"]);
    schema
}

pub(crate) fn open_session_shell_input_schema() -> Value {
    let mut schema = object_schema(vec![
        ("project", "string", "Exact Workflow Session project id.", true),
        (
            "session_id",
            "string",
            "Explicit active Workflow Session id. Current-session fallback is not used.",
            true,
        ),
        (
            "cwd",
            "string",
            "Optional project-relative initial cwd. Omission uses the Session execution context and then the project default.",
            false,
        ),
        (
            "shell",
            "string",
            "Optional long-lived shell dialect: sh or bash.",
            false,
        ),
    ]);
    schema["properties"]["shell"]["enum"] = json!(["sh", "bash"]);
    schema
}

pub(crate) fn session_shell_exec_input_schema() -> Value {
    let mut schema = object_schema(vec![
        ("project", "string", "Exact Workflow Session project id.", true),
        ("session_id", "string", "Explicit active Workflow Session id.", true),
        (
            "shell_id",
            "string",
            "Opaque id returned by open_session_shell.",
            true,
        ),
        (
            "command",
            "string",
            "One command evaluated by the existing long-lived shell.",
            true,
        ),
        (
            "timeout_secs",
            "integer",
            "Command timeout in seconds (1..=3600, default 60). A timeout interrupts the process group and requires verified resynchronization.",
            false,
        ),
        (
            "purpose",
            "string",
            "Declared execution intent recorded as evidence.",
            false,
        ),
    ]);
    schema["properties"]["timeout_secs"]["minimum"] = json!(1);
    schema["properties"]["timeout_secs"]["maximum"] = json!(3600);
    schema["properties"]["timeout_secs"]["default"] = json!(60);
    schema["properties"]["command"]["maxLength"] = json!(8000);
    schema["properties"]["purpose"]["enum"] = json!([
        "validation",
        "test",
        "build",
        "format",
        "release",
        "diagnostic",
        "operation",
        "other"
    ]);
    schema
}

pub(crate) fn session_shell_identity_input_schema() -> Value {
    object_schema(vec![
        (
            "project",
            "string",
            "Exact Workflow Session project id.",
            true,
        ),
        (
            "session_id",
            "string",
            "Explicit active Workflow Session id.",
            true,
        ),
        (
            "shell_id",
            "string",
            "Opaque id returned by open_session_shell.",
            true,
        ),
    ])
}

pub(crate) fn stop_job_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        (
            "project",
            "string",
            "Configured project id that must match the job project.",
            true,
        ),
        ("job_id", "string", "Runtime job id returned by run_job.", true),
        (
            "confirm",
            "boolean",
            "Must be true to stop or no-op an already-finished job; false returns confirmation_required.",
            false,
        ),
    ]))
}

pub(crate) fn job_status_input_schema() -> Value {
    object_schema(vec![
        ("job_id", "string", "Job id.", true),
        (
            "include_command_preview",
            "boolean",
            "Optional debug flag. Defaults to false; when true, includes bounded command_preview metadata. stdout/stderr bodies are never included.",
            false,
        ),
    ])
}

pub(crate) fn job_log_input_schema() -> Value {
    let mut schema = object_schema(vec![
        ("job_id", "string", "Job id.", true),
        (
            "offset",
            "integer",
            "Optional 1-based cursor returned by a previous call. Reads the next bounded segment.",
            false,
        ),
        (
            "tail_lines",
            "integer",
            "Optional number of trailing lines per stream. Defaults to 200 and is capped at 500.",
            false,
        ),
        (
            "after_observation_token",
            "string",
            "Opaque observation token returned by run_job, job_status, job_log, or job_tail. Return it unchanged. The token is bound to one Job; a server epoch change causes an immediate refresh.",
            false,
        ),
        (
            "wait_secs",
            "integer",
            "Optional bounded wait in seconds (1..=60). When both after_observation_token and wait_secs are supplied, this is a single bounded wait, not a subscription or streaming connection.",
            false,
        ),
    ]);
    schema["properties"]["after_observation_token"]["maxLength"] = json!(192);
    schema["properties"]["wait_secs"]["minimum"] = json!(1);
    schema["properties"]["wait_secs"]["maximum"] = json!(60);
    schema
}

pub(crate) fn list_jobs_input_schema() -> Value {
    object_schema(vec![
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
    ])
}
