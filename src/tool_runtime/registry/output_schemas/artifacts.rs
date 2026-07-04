use serde_json::Value;

use super::common::{
    array_schema, nullable_schema, open_object_schema, schema_type, wrapped_output_schema,
};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
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
                schema_type("integer", "Offset to use for the next chunk."),
            ),
            (
                "truncated",
                schema_type("boolean", "True when more bytes remain after this chunk."),
            ),
            (
                "eof",
                schema_type("boolean", "True when this chunk reaches end of file."),
            ),
        ])),
        _ => None,
    }
}
