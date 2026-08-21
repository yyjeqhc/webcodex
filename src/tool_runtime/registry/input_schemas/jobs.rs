use serde_json::{json, Value};

use super::common::{object_schema, with_optional_session_id};
use crate::shell_protocol::{
    DETACHED_IDEMPOTENCY_KEY_MAX_BYTES, JOB_TERMINAL_RETENTION_SECS, PROCESS_ARG_MAX_BYTES,
    PROCESS_ARG_MAX_COUNT, PROCESS_CWD_MAX_BYTES, PROCESS_EXECUTABLE_MAX_BYTES,
    PROCESS_STDIN_MAX_BYTES, PROCESS_TIMEOUT_MAX_SECS, RAW_SHELL_COMMAND_MAX_BYTES,
    SCRIPT_ARGV_MAX_BYTES, SCRIPT_ARG_MAX_BYTES, SCRIPT_ARG_MAX_COUNT, SCRIPT_CWD_MAX_BYTES,
    SCRIPT_MAX_BYTES, SCRIPT_STDIN_MAX_BYTES, SCRIPT_TIMEOUT_MAX_SECS,
};

pub(crate) fn run_process_input_schema() -> Value {
    let mut schema = object_schema(with_optional_session_id(vec![
        ("project", "string", "Configured project id.", true),
        (
            "executable",
            "string",
            "Native executable name or path. It is executed directly and never parsed as shell text. Windows .cmd/.bat files are rejected because they require shell semantics.",
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
            "Total process runtime budget in seconds (1..=3600, default 60). Short work returns synchronously; longer work keeps the same execution and returns job_id when durable structured execution is available.",
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

pub(crate) fn run_detached_process_input_schema() -> Value {
    let mut schema = run_process_input_schema();
    schema["properties"]["idempotency_key"] = json!({
        "type": "string",
        "minLength": 1,
        "maxLength": DETACHED_IDEMPOTENCY_KEY_MAX_BYTES,
        "description": format!("Required bounded caller-chosen key for this detached initiation. While the logical Job is active or retained for {JOB_TERMINAL_RETENTION_SECS} seconds after terminal completion, reusing the same key resolves to that Job and cannot redispatch its payload. After retained history expires the key may identify a new execution, so never reuse an expired key as a retry token. After Server restart an existing retained Job is returned for recovery rather than guessing that a resent body matches.")
    });
    schema["required"]
        .as_array_mut()
        .expect("run_process schema required array")
        .push(json!("idempotency_key"));
    schema["properties"]["cwd"]["description"] = json!(
        "Project-relative working directory. Omit, empty string, or '.' for the project root. Named Session SSH resources are unsupported for run_detached_process."
    );
    schema["properties"]["timeout_secs"]["description"] = json!(
        "Total detached process runtime budget in seconds (1..=3600, default 60). Admission returns the stable Job identity without waiting for terminal completion."
    );
    schema
}

pub(crate) fn run_script_input_schema() -> Value {
    let mut schema = object_schema(with_optional_session_id(vec![
        ("project", "string", "Configured project id.", true),
        (
            "language",
            "string",
            "Required semantic script language. The Runner selects the concrete interpreter; Session default_shell never overrides this field.",
            true,
        ),
        (
            "script",
            "string",
            "Bounded UTF-8 script content transported as typed data. It is written to a Runner-owned temporary file and never placed in a shell command string.",
            true,
        ),
        (
            "args",
            "array",
            "Ordered script arguments passed as independent argv values. Defaults to an empty array; values are never interpolated into the script body.",
            false,
        ),
        (
            "stdin",
            "string",
            "Optional bounded UTF-8 stdin payload piped independently to the script process. Null and omission both mean no stdin payload.",
            false,
        ),
        (
            "cwd",
            "string",
            "Project-relative working directory. Omit, empty string, or '.' for the project root. Named Session SSH resources are unsupported for run_script.",
            false,
        ),
        (
            "timeout_secs",
            "integer",
            "Total script runtime budget in seconds (1..=3600, default 60). Short work returns synchronously; longer work keeps the same execution and returns job_id when durable structured execution is available.",
            false,
        ),
        (
            "purpose",
            "string",
            "Declared execution intent: validation, test, build, format, release, diagnostic, operation, or other. This records evidence and never changes authorization.",
            false,
        ),
    ]));
    schema["properties"]["language"]["enum"] = json!(["sh", "bash", "powershell"]);
    schema["properties"]["script"]["minLength"] = json!(1);
    schema["properties"]["script"]["maxLength"] = json!(SCRIPT_MAX_BYTES);
    schema["properties"]["args"]["maxItems"] = json!(SCRIPT_ARG_MAX_COUNT);
    schema["properties"]["args"]["items"]["maxLength"] = json!(SCRIPT_ARG_MAX_BYTES);
    schema["properties"]["args"]["default"] = json!([]);
    schema["properties"]["args"]["description"] = json!(format!(
        "Ordered script arguments passed as independent argv values. Defaults to an empty array. At most {SCRIPT_ARG_MAX_COUNT} entries, each at most {SCRIPT_ARG_MAX_BYTES} UTF-8 bytes, with at most {SCRIPT_ARGV_MAX_BYTES} bytes total including one boundary byte per value; values are never interpolated into the script body."
    ));
    schema["properties"]["stdin"] = json!({
        "anyOf": [
            {"type": "string", "maxLength": SCRIPT_STDIN_MAX_BYTES},
            {"type": "null"}
        ],
        "description": "Optional bounded UTF-8 stdin payload piped independently to the script process; null and omission both mean no stdin payload."
    });
    schema["properties"]["cwd"]["maxLength"] = json!(SCRIPT_CWD_MAX_BYTES);
    schema["properties"]["timeout_secs"]["minimum"] = json!(1);
    schema["properties"]["timeout_secs"]["maximum"] = json!(SCRIPT_TIMEOUT_MAX_SECS);
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
            "Total command timeout in seconds (1..=120, default 60). Explicit values above 60 may hand off the same original execution as a durable Job when the Runner supports async shell Jobs; default 60-second calls remain synchronous.",
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
    schema["properties"]["command"]["maxLength"] = json!(RAW_SHELL_COMMAND_MAX_BYTES);
    schema["properties"]["command"]["description"] = json!(format!(
        "Shell command to run. At most {RAW_SHELL_COMMAND_MAX_BYTES} UTF-8 bytes; use run_script for larger program text and stdin/files/artifacts for large data."
    ));
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
    schema["properties"]["command"]["maxLength"] = json!(RAW_SHELL_COMMAND_MAX_BYTES);
    schema["properties"]["command"]["description"] = json!(format!(
        "Shell command to run asynchronously. At most {RAW_SHELL_COMMAND_MAX_BYTES} UTF-8 bytes; use run_script for larger program text and stdin/files/artifacts for large data."
    ));
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
            "Optional initial cwd. Without a named Session SSH resource it is project-relative; with one it is a remote path. Omission uses the Session default, then the project or SSH-resource default.",
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
    schema["properties"]["command"]["maxLength"] = json!(RAW_SHELL_COMMAND_MAX_BYTES);
    schema["properties"]["command"]["description"] = json!(format!(
        "One command evaluated by the existing long-lived shell. At most {RAW_SHELL_COMMAND_MAX_BYTES} UTF-8 bytes."
    ));
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
        ("job_id", "string", "Existing runtime Job id to stop.", true),
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
            "Opaque token from the latest Job snapshot. Return it unchanged. It is bound to one job_id, not execution identity; a Server epoch change causes an immediate refresh when that Job still exists.",
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

pub(crate) fn observe_jobs_input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "items": {
                "type": "array",
                "minItems": 1,
                "maxItems": 8,
                "description": "Existing Jobs to observe in input order. Duplicate job_id values are rejected.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "job_id": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Existing opaque runtime Job id."
                        },
                        "after_observation_token": {
                            "type": "string",
                            "maxLength": 192,
                            "description": "Optional opaque Job-bound token from the latest snapshot. Return it unchanged. A stale server epoch is immediately actionable."
                        }
                    },
                    "required": ["job_id"]
                }
            },
            "tail_lines": {
                "type": "integer",
                "minimum": 1,
                "maximum": 200,
                "default": 40,
                "description": "Global bounded trailing line count per stdout/stderr stream for every returned Job."
            },
            "wait_secs": {
                "type": "integer",
                "minimum": 1,
                "maximum": 60,
                "description": "Optional one shared bounded wait. It returns when any relevant Job changes and is never multiplied by item count."
            }
        },
        "required": ["items"]
    })
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
