use serde_json::{json, Value};

use super::common::{
    object_schema, with_optional_session_id, OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION,
};

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

pub(crate) fn import_conversation_files_to_project_input_schema() -> Value {
    let mut schema = object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        (
            "openaiFileIdRefs",
            "array",
            "Host-provided ChatGPT conversation attachment references.",
            true,
        ),
        (
            "output_dir",
            "string",
            "Optional project-relative output directory; defaults to artifacts/imports.",
            false,
        ),
        (
            "targets",
            "array",
            "Optional per-file output filenames, in attachment order.",
            false,
        ),
        (
            "overwrite",
            "boolean",
            "Allow overwriting existing files (default false).",
            false,
        ),
    ]));
    schema["properties"]["openaiFileIdRefs"] = json!({
        "type": "array",
        "minItems": 1,
        "maxItems": 10,
        "description": "ChatGPT host-populated conversation attachment references. Over MCP this field is declared through openai/fileParams; do not construct file ids or download URLs manually.",
        "items": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "download_url": {
                    "type": "string",
                    "description": "Temporary OpenAI-hosted download URL supplied by ChatGPT."
                },
                "file_id": {
                    "type": "string",
                    "description": "Host-supplied file id; WebCodex does not dereference it through an OpenAI file API."
                },
                "mime_type": {
                    "type": "string",
                    "description": "Host-supplied MIME type when available."
                },
                "file_name": {
                    "type": "string",
                    "description": "Host-supplied attachment filename when available."
                }
            },
            "required": ["download_url", "file_id"]
        }
    });
    schema
}

pub(crate) fn export_project_artifact_input_schema() -> Value {
    object_schema(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative artifact path.", true),
        (
            "session_id",
            "string",
            OPTIONAL_EXPLICIT_SESSION_ID_DESCRIPTION,
            false,
        ),
    ])
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
    ]))
}

pub(crate) fn artifact_upload_begin_input_schema() -> Value {
    object_schema(with_optional_session_id(vec![
        ("project", "string", "Agent-registered project id.", true),
        ("path", "string", "Project-relative output path.", true),
        (
            "expected_bytes",
            "integer",
            "Optional final byte count guard; cannot exceed 268435456 bytes (256 MiB).",
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
            "Base64-encoded chunk; decoded chunk max is 1048576 bytes (1 MiB).",
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
