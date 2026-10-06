//! Dedicated DOCX presentation on the authenticated, version-fenced artifact transport.
use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};

pub(crate) const MAX_DOCX_BYTES: usize = 10 * 1024 * 1024;

fn docx_error(kind: &str, message: &str) -> ToolResult {
    ToolResult::err_with_output(message, json!({"error_kind": kind, "state_changed": false}))
}

fn validate_docx_target(
    path: &str,
    bytes: usize,
    sha256: &str,
    offset: usize,
) -> Result<(), ToolResult> {
    super::files::validate_artifact_file_path(path).map_err(|_| {
        docx_error(
            "policy_rejected",
            "DOCX path is not an allowed project-relative artifact path",
        )
    })?;
    if path.len() > 512 || !path.to_ascii_lowercase().ends_with(".docx") {
        return Err(docx_error(
            "not_docx",
            "Select a project-relative .docx file",
        ));
    }
    if !(4..=MAX_DOCX_BYTES).contains(&bytes) {
        return Err(docx_error(
            "docx_size_unavailable",
            "DOCX preview requires a file of 4 bytes to 10 MiB",
        ));
    }
    if !(sha256.len() == 64
        && sha256
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)))
        || offset >= bytes
    {
        return Err(docx_error(
            "invalid_docx_identity",
            "Invalid DOCX digest or byte offset",
        ));
    }
    Ok(())
}

fn validate_docx_segment(
    output: &Value,
    path: &str,
    bytes: usize,
    offset: usize,
    length: usize,
) -> Result<(), ToolResult> {
    if output.get("error_kind").and_then(Value::as_str) == Some("snapshot_changed") {
        return Err(docx_error(
            "snapshot_changed",
            "DOCX version changed; explicitly reopen the document",
        ));
    }
    if output.get("error").is_some() {
        return Err(docx_error(
            "docx_read_unavailable",
            "DOCX could not be read",
        ));
    }
    let next = offset + length;
    let encoded = output
        .get("content_base64")
        .and_then(Value::as_str)
        .filter(|value| value.len() <= length.div_ceil(3) * 4)
        .ok_or_else(|| docx_error("invalid_docx_chunk", "Invalid DOCX chunk bounds"))?;
    let decoded = general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| docx_error("invalid_docx_chunk", "Invalid DOCX chunk encoding"))?;
    if output["path"] != path
        || output["file_bytes"] != bytes
        || output["offset"] != offset
        || output["bytes_returned"] != length
        || output["next_offset"] != next
        || output["eof"] != (next == bytes)
        || decoded.len() != length
    {
        return Err(docx_error(
            "invalid_docx_chunk",
            "DOCX chunk identity or continuation mismatch",
        ));
    }
    if offset == 0 && !decoded.starts_with(b"PK\x03\x04") {
        return Err(docx_error(
            "not_docx",
            "File does not have a supported DOCX ZIP header",
        ));
    }
    Ok(())
}

impl ToolRuntime {
    pub(crate) async fn present_docx(
        &self,
        project: String,
        path: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Err(error) = super::files::validate_artifact_file_path(&path) {
            return docx_error("policy_rejected", &error);
        }
        if path.len() > 512 || !path.to_ascii_lowercase().ends_with(".docx") {
            return docx_error("not_docx", "Select a project-relative .docx file");
        }
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(error) => return error.into_tool_result(),
        };
        let metadata = self
            .export_project_artifact_metadata_resolved(&resolved, path.clone(), auth)
            .await;
        if !metadata.success {
            return metadata;
        }
        let bytes = metadata.output["bytes"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0);
        let sha256 = metadata.output["sha256"].as_str().unwrap_or_default();
        if let Err(error) = validate_docx_target(&path, bytes, sha256, 0) {
            return error;
        }
        if metadata.output["mime_type"] != webcodex_core::artifact_policy::DOCX_MIME {
            return docx_error("not_docx", "File is not a supported DOCX package");
        }
        let header = match self
            .read_project_artifact_export_chunk_internal(
                &resolved.resolved_id,
                &path,
                bytes,
                sha256,
                0,
                4,
                auth,
            )
            .await
        {
            Ok(output) => output,
            Err(_) => return docx_error("docx_read_unavailable", "DOCX could not be read"),
        };
        if let Err(error) = validate_docx_segment(&header, &path, bytes, 0, 4) {
            return error;
        }
        ToolResult::ok(json!({"docx_document": {
            "project": resolved.resolved_id, "path": path,
            "name": metadata.output["name"], "bytes": bytes, "sha256": sha256,
        }}))
    }
}
