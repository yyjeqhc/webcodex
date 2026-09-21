use serde_json::{json, Value};

use super::common::{
    array_schema, nullable_schema, open_object_schema, schema_type, suggested_tool_call_schema,
    wrapped_output_schema,
};

fn read_project_artifact_suggested_call_schema() -> Value {
    suggested_tool_call_schema(
        "read_project_artifact",
        json!({
            "type": "object",
            "description": "Parser-ready next ranged read of the same exact full-file artifact incarnation.",
            "additionalProperties": false,
            "properties": {
                "project": {"type": "string", "minLength": 1},
                "path": {"type": "string", "minLength": 1},
                "encoding": {"type": "string", "const": "base64"},
                "offset": {"type": "integer", "minimum": 0},
                "length": {"type": "integer", "minimum": 1, "maximum": 65536},
                "expected_sha256": {
                    "type": "string",
                    "minLength": 64,
                    "maxLength": 64,
                    "pattern": "^[0-9a-f]{64}$"
                },
                "session_id": {"type": "string", "pattern": "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"}
            },
            "required": [
                "project",
                "path",
                "encoding",
                "offset",
                "length",
                "expected_sha256"
            ]
        }),
        "Parser-ready advisory call for the next ranged read of the same exact artifact content snapshot. It grants no Project or Session authority.",
    )
}

fn project_artifact_suggested_call_schema() -> Value {
    suggested_tool_call_schema(
        "project_artifact",
        json!({
            "type": "object",
            "description": "Parser-ready continuation for one more bounded inspect of the same exact full-file artifact incarnation.",
            "additionalProperties": false,
            "properties": {
                "project": {"type": "string", "minLength": 1},
                "path": {"type": "string", "minLength": 1},
                "action": {"type": "string", "const": "inspect"},
                "offset": {"type": "integer", "minimum": 0},
                "length": {"type": "integer", "minimum": 1, "maximum": 65536},
                "expected_sha256": {
                    "type": "string",
                    "minLength": 64,
                    "maxLength": 64,
                    "pattern": "^[0-9a-f]{64}$"
                },
                "session_id": {"type": "string", "pattern": "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"}
            },
            "required": [
                "project",
                "path",
                "action",
                "offset",
                "length",
                "expected_sha256"
            ]
        }),
        "Parser-ready advisory continuation for project_artifact(action=inspect). It carries the observed full-file SHA-256 fence and grants no Project or Session authority.",
    )
}

fn project_artifact_output_schema() -> Value {
    let mut merged = wrapped_output_schema(vec![]);
    let target = merged["properties"]["output"]["properties"]
        .as_object_mut()
        .expect("project_artifact output properties");
    for specialist in [
        "read_project_artifact_metadata",
        "read_project_artifact",
        "export_project_artifact",
    ] {
        let source = output_schema_for_tool(specialist).expect("artifact specialist output schema");
        let properties = source["properties"]["output"]["properties"]
            .as_object()
            .expect("artifact specialist output properties");
        for (name, schema) in properties {
            target.entry(name.clone()).or_insert_with(|| schema.clone());
        }
    }
    target.insert(
        "suggested_call".to_string(),
        project_artifact_suggested_call_schema(),
    );
    target.insert(
        "content_delivery".to_string(),
        json!({
            "type": "string",
            "const": "mcp_image",
            "description": "MCP image action marker after native-image framing; image bytes are carried in an MCP image ContentBlock instead of structuredContent."
        }),
    );
    merged
}

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "project_artifact" => Some(project_artifact_output_schema()),
        "save_project_artifact" => Some(wrapped_output_schema(vec![
            (
                "path",
                schema_type("string", "Project-relative artifact path."),
            ),
            (
                "bytes_written",
                schema_type("integer", "Bytes written to the artifact path."),
            ),
            (
                "sha256",
                schema_type("string", "sha256 digest of the written artifact."),
            ),
            (
                "mime_type",
                nullable_schema("string", "Caller-provided MIME type, when provided."),
            ),
        ])),
        "import_conversation_files_to_project" => Some(wrapped_output_schema(vec![
            (
                "count",
                schema_type("integer", "Number of conversation attachments imported."),
            ),
            (
                "imported",
                array_schema(
                    open_object_schema("Imported conversation attachment result."),
                    "Per-file path, byte count, sha256, MIME type, project, and source name.",
                ),
            ),
        ])),
        "export_project_artifact" => Some(wrapped_output_schema(vec![
            // existing compatibility MCP export
            (
                "project",
                schema_type("string", "Canonical Runner-registered project id."),
            ),
            (
                "path",
                schema_type("string", "Project-relative artifact path."),
            ),
            ("bytes", schema_type("integer", "Artifact size in bytes.")),
            (
                "sha256",
                schema_type("string", "sha256 digest of the full artifact file."),
            ),
            (
                "mime_type",
                schema_type("string", "Validated artifact MIME type."),
            ),
            (
                "name",
                schema_type("string", "Safe basename presented by the MCP ResourceLink."),
            ),
        ])),
        "project_artifact_download_link" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Canonical Runner-registered project id.")),
            ("path", schema_type("string", "Project-relative artifact path.")),
            ("download_url", schema_type("string", "Short-lived one-shot HTTPS capability URL.")),
            ("expires_in_secs", schema_type("integer", "Capability lifetime in seconds.")),
            ("bytes", schema_type("integer", "Artifact size in bytes.")),
            ("sha256", schema_type("string", "sha256 digest of the exact artifact snapshot.")),
            ("mime_type", schema_type("string", "Validated artifact MIME type.")),
            ("name", schema_type("string", "Safe artifact basename.")),
        ])),
        "read_project_artifact_metadata" => Some(wrapped_output_schema(vec![
            (
                "path",
                schema_type("string", "Project-relative artifact path."),
            ),
            (
                "exists",
                schema_type("boolean", "True when the artifact exists."),
            ),
            (
                "missing",
                schema_type(
                    "boolean",
                    "True when allow_missing=true and the artifact was absent.",
                ),
            ),
            ("bytes", schema_type("integer", "Artifact size in bytes.")),
            (
                "sha256",
                schema_type("string", "sha256 digest of the full artifact file."),
            ),
            (
                "mime_type",
                nullable_schema("string", "Detected or inferred MIME type."),
            ),
            (
                "modified_at",
                schema_type(
                    "integer",
                    "File modification time as unix timestamp seconds.",
                ),
            ),
            (
                "width",
                schema_type("integer", "Image width, when cheaply detected."),
            ),
            (
                "height",
                schema_type("integer", "Image height, when cheaply detected."),
            ),
            (
                "archive_entries_count",
                nullable_schema("integer", "Zip entry count, when cheaply detected."),
            ),
        ])),
        "artifact_upload_begin" | "artifact_upload_chunk" => Some(wrapped_output_schema(vec![
            (
                "path",
                schema_type("string", "Project-relative artifact path."),
            ),
            (
                "upload_id",
                schema_type(
                    "string",
                    "Opaque upload id for later chunks, finish, or abort.",
                ),
            ),
            (
                "received_bytes",
                schema_type("integer", "Bytes currently received for this upload."),
            ),
            (
                "next_offset",
                schema_type("integer", "Offset to pass with the next chunk."),
            ),
            (
                "expected_bytes",
                nullable_schema("integer", "Expected final byte count, when provided."),
            ),
            (
                "expected_sha256",
                nullable_schema("string", "Expected final sha256, when provided."),
            ),
            (
                "max_bytes",
                schema_type("integer", "Maximum upload size in bytes."),
            ),
            (
                "mime_type",
                nullable_schema("string", "Caller-provided MIME type, when provided."),
            ),
            (
                "committed",
                schema_type("boolean", "False until artifact_upload_finish succeeds."),
            ),
        ])),
        "artifact_upload_finish" => Some(wrapped_output_schema(vec![
            (
                "path",
                schema_type("string", "Project-relative artifact path."),
            ),
            ("upload_id", schema_type("string", "Committed upload id.")),
            (
                "bytes",
                schema_type("integer", "Final artifact size in bytes."),
            ),
            (
                "received_bytes",
                schema_type("integer", "Bytes received before commit."),
            ),
            (
                "expected_bytes",
                nullable_schema("integer", "Expected final byte count, when provided."),
            ),
            (
                "expected_sha256",
                nullable_schema("string", "Expected final sha256, when provided."),
            ),
            (
                "sha256",
                schema_type("string", "sha256 digest of the committed artifact."),
            ),
            (
                "mime_type",
                nullable_schema(
                    "string",
                    "Detected, inferred, or caller-provided MIME type.",
                ),
            ),
            (
                "committed",
                schema_type("boolean", "True when commit completed."),
            ),
        ])),
        "artifact_upload_abort" => Some(wrapped_output_schema(vec![
            (
                "path",
                schema_type("string", "Project-relative artifact path."),
            ),
            ("upload_id", schema_type("string", "Aborted upload id.")),
            (
                "received_bytes",
                schema_type("integer", "Bytes discarded from the temporary upload."),
            ),
            (
                "expected_bytes",
                nullable_schema("integer", "Expected final byte count, when provided."),
            ),
            (
                "expected_sha256",
                nullable_schema("string", "Expected final sha256, when provided."),
            ),
            (
                "mime_type",
                nullable_schema("string", "Caller-provided MIME type, when provided."),
            ),
            (
                "committed",
                schema_type("boolean", "False for aborted uploads."),
            ),
            (
                "aborted",
                schema_type("boolean", "True when temporary upload files were removed."),
            ),
            (
                "temp_file_removed",
                schema_type(
                    "boolean",
                    "True when the temporary upload part file was removed.",
                ),
            ),
            (
                "sidecar_removed",
                schema_type(
                    "boolean",
                    "True when the temporary upload sidecar was removed.",
                ),
            ),
            (
                "final_file_touched",
                schema_type(
                    "boolean",
                    "Always false; abort does not touch the final target path.",
                ),
            ),
            (
                "final_file_exists",
                schema_type("boolean", "Read-only final target existence after abort."),
            ),
            (
                "changed_path_details",
                array_schema(
                    open_object_schema("Path cleanup status detail."),
                    "Abort cleanup path status details.",
                ),
            ),
        ])),
        "read_project_artifact" => Some(wrapped_output_schema(vec![
            (
                "path",
                schema_type("string", "Project-relative artifact path."),
            ),
            (
                "mime_type",
                nullable_schema("string", "Detected or inferred MIME type."),
            ),
            (
                "file_bytes",
                schema_type("integer", "Total file size in bytes."),
            ),
            (
                "sha256",
                schema_type("string", "sha256 digest of the full artifact file."),
            ),
            ("offset", schema_type("integer", "Requested byte offset.")),
            (
                "bytes_returned",
                schema_type("integer", "Number of bytes returned in this chunk."),
            ),
            (
                "content_base64",
                schema_type("string", "Base64-encoded content for this chunk only."),
            ),
            (
                "next_offset",
                schema_type(
                    "integer",
                    "Domain metadata for the next chunk offset. Models should continue through suggested_call, which also carries the observed full-file SHA-256 snapshot fence.",
                ),
            ),
            (
                "truncated",
                schema_type("boolean", "True when more bytes remain after this chunk."),
            ),
            (
                "eof",
                schema_type("boolean", "True when this chunk reaches end of file."),
            ),
            (
                "suggested_call",
                read_project_artifact_suggested_call_schema(),
            ),
        ])),
        _ => None,
    }
}
