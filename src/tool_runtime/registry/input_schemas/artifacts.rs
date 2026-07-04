use serde_json::Value;

use super::common::{object_schema, with_optional_session_id};

pub(crate) fn save_project_artifact_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative output path.", true),
        (
            "content_base64",
            "string",
            "Base64-encoded binary content.",
            true,
        ),
        ("mime_type", "string", "Optional MIME type.", false),
        (
            "overwrite",
            "boolean",
            "Allow overwriting an existing file (default false).",
            false,
        ),
    ]))
}

pub(crate) fn read_project_artifact_metadata_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative artifact path.", true),
        (
            "allow_missing",
            "boolean",
            "When true, a missing artifact returns exists=false instead of an error.",
            false,
        ),
    ]))
}

pub(crate) fn read_project_artifact_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative artifact path.", true),
        (
            "encoding",
            "string",
            "Optional encoding; only base64 is supported (default base64).",
            false,
        ),
        (
            "offset",
            "integer",
            "Optional byte offset to start reading from; defaults to 0.",
            false,
        ),
        (
            "length",
            "integer",
            "Optional chunk length in bytes; defaults to 32768 and cannot exceed 65536.",
            false,
        ),
        (
            "max_bytes",
            "integer",
            "Compatibility alias/upper bound for length; cannot exceed 65536.",
            false,
        ),
    ]))
}

pub(crate) fn artifact_upload_begin_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative output path.", true),
        (
            "expected_bytes",
            "integer",
            "Optional final byte count guard.",
            false,
        ),
        (
            "expected_sha256",
            "string",
            "Optional final sha256 guard.",
            false,
        ),
        ("mime_type", "string", "Optional MIME type.", false),
        (
            "overwrite",
            "boolean",
            "Allow overwriting an existing file at finish (default false).",
            false,
        ),
    ]))
}

pub(crate) fn artifact_upload_chunk_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "path",
            "string",
            "Required project-relative path; must exactly match the path used in artifact_upload_begin to bind upload_id to the target.",
            true,
        ),
        (
            "upload_id",
            "string",
            "Opaque wc_upload_* id from artifact_upload_begin.",
            true,
        ),
        ("offset", "integer", "Expected current upload byte offset.", true),
        (
            "content_base64",
            "string",
            "Base64-encoded chunk; decoded chunk max is 65536 bytes.",
            true,
        ),
    ]))
}

pub(crate) fn artifact_upload_finish_input_schema() -> Value {
    artifact_upload_followup_input_schema()
}

pub(crate) fn artifact_upload_abort_input_schema() -> Value {
    artifact_upload_followup_input_schema()
}

fn artifact_upload_followup_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "path",
            "string",
            "Required project-relative path; must exactly match the path used in artifact_upload_begin to bind upload_id to the target.",
            true,
        ),
        (
            "upload_id",
            "string",
            "Opaque wc_upload_* id from artifact_upload_begin.",
            true,
        ),
    ]))
}
