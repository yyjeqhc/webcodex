//! Dedicated PDF presentation on the authenticated, version-fenced artifact transport.
use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};

pub(crate) const MAX_PDF_BYTES: usize = 20 * 1024 * 1024;
pub(crate) const PDF_CHUNK_BYTES: usize = 128 * 1024;

fn pdf_error(kind: &str, message: &str) -> ToolResult {
    ToolResult::err_with_output(message, json!({"error_kind": kind, "state_changed": false}))
}

fn app_artifact_error(kind: &str, message: &str) -> ToolResult {
    ToolResult::err_with_output(message, json!({"error_kind": kind, "state_changed": false}))
}

fn validate_app_artifact_target(
    path: &str,
    bytes: usize,
    sha256: &str,
    offset: usize,
) -> Result<(), ToolResult> {
    super::files::validate_artifact_file_path(path)
        .map_err(|_| app_artifact_error("policy_rejected", "Artifact path is not allowed"))?;
    if path.len() > 512
        || bytes == 0
        || bytes > super::files::MAX_PROJECT_ARTIFACT_EXPORT_BYTES
        || offset >= bytes
        || sha256.len() != 64
        || !sha256
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(app_artifact_error(
            "invalid_artifact_identity",
            "Invalid presentation artifact identity or byte offset",
        ));
    }
    Ok(())
}

fn validate_app_artifact_segment(
    output: &Value,
    path: &str,
    bytes: usize,
    offset: usize,
    length: usize,
) -> Result<(), ToolResult> {
    if output.get("error_kind").and_then(Value::as_str) == Some("snapshot_changed") {
        return Err(app_artifact_error(
            "snapshot_changed",
            "Artifact version changed; explicitly reopen the presentation",
        ));
    }
    if output.get("error").is_some() {
        return Err(app_artifact_error(
            "artifact_read_unavailable",
            "Artifact could not be read",
        ));
    }
    let next = offset + length;
    let encoded = output
        .get("content_base64")
        .and_then(Value::as_str)
        .filter(|value| value.len() <= length.div_ceil(3) * 4)
        .ok_or_else(|| {
            app_artifact_error("invalid_artifact_chunk", "Invalid artifact chunk bounds")
        })?;
    let decoded = general_purpose::STANDARD.decode(encoded).map_err(|_| {
        app_artifact_error("invalid_artifact_chunk", "Invalid artifact chunk encoding")
    })?;
    if output["path"] != path
        || output["file_bytes"] != bytes
        || output["offset"] != offset
        || output["bytes_returned"] != length
        || output["next_offset"] != next
        || output["eof"] != (next == bytes)
        || decoded.len() != length
    {
        return Err(app_artifact_error(
            "invalid_artifact_chunk",
            "Artifact chunk identity or continuation mismatch",
        ));
    }
    Ok(())
}

fn validate_pdf_target(
    path: &str,
    bytes: usize,
    sha256: &str,
    offset: usize,
) -> Result<(), ToolResult> {
    super::files::validate_artifact_file_path(path).map_err(|_| {
        pdf_error(
            "policy_rejected",
            "PDF path is not an allowed project-relative artifact path",
        )
    })?;
    if path.len() > 512 || !path.to_ascii_lowercase().ends_with(".pdf") {
        return Err(pdf_error("not_pdf", "Select a project-relative .pdf file"));
    }
    if !(5..=MAX_PDF_BYTES).contains(&bytes) {
        return Err(pdf_error(
            "pdf_size_unavailable",
            "PDF preview requires a file of 5 bytes to 20 MiB",
        ));
    }
    if !(sha256.len() == 64
        && sha256
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)))
        || offset >= bytes
    {
        return Err(pdf_error(
            "invalid_pdf_identity",
            "Invalid PDF digest or byte offset",
        ));
    }
    Ok(())
}

fn validate_pdf_segment(
    output: &Value,
    path: &str,
    bytes: usize,
    offset: usize,
    length: usize,
) -> Result<(), ToolResult> {
    if output.get("error_kind").and_then(Value::as_str) == Some("snapshot_changed") {
        return Err(pdf_error(
            "snapshot_changed",
            "PDF version changed; explicitly reopen the document",
        ));
    }
    if output.get("error").is_some() {
        return Err(pdf_error("pdf_read_unavailable", "PDF could not be read"));
    }
    let next = offset + length;
    let encoded = output
        .get("content_base64")
        .and_then(Value::as_str)
        .filter(|value| value.len() <= length.div_ceil(3) * 4)
        .ok_or_else(|| pdf_error("invalid_pdf_chunk", "Invalid PDF chunk bounds"))?;
    let decoded = general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| pdf_error("invalid_pdf_chunk", "Invalid PDF chunk encoding"))?;
    if output["path"] != path
        || output["file_bytes"] != bytes
        || output["offset"] != offset
        || output["bytes_returned"] != length
        || output["next_offset"] != next
        || output["eof"] != (next == bytes)
        || decoded.len() != length
    {
        return Err(pdf_error(
            "invalid_pdf_chunk",
            "PDF chunk identity or continuation mismatch",
        ));
    }
    if offset == 0 && !decoded.starts_with(b"%PDF-") {
        return Err(pdf_error(
            "not_pdf",
            "File does not have a supported PDF header",
        ));
    }
    Ok(())
}

impl ToolRuntime {
    pub(crate) async fn present_pdf(
        &self,
        project: String,
        path: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Err(error) = super::files::validate_artifact_file_path(&path) {
            return pdf_error("policy_rejected", &error);
        }
        if path.len() > 512 || !path.to_ascii_lowercase().ends_with(".pdf") {
            return pdf_error("not_pdf", "Select a project-relative .pdf file");
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
        if let Err(error) = validate_pdf_target(&path, bytes, sha256, 0) {
            return error;
        }
        let header = match self
            .read_project_artifact_export_chunk_internal(
                &resolved.resolved_id,
                &path,
                bytes,
                sha256,
                0,
                5,
                auth,
            )
            .await
        {
            Ok(output) => output,
            Err(_) => return pdf_error("pdf_read_unavailable", "PDF could not be read"),
        };
        if let Err(error) = validate_pdf_segment(&header, &path, bytes, 0, 5) {
            return error;
        }
        ToolResult::ok(json!({"pdf_document": {
            "project": resolved.resolved_id, "path": path,
            "name": metadata.output["name"], "bytes": bytes, "sha256": sha256,
        }}))
    }

    pub(crate) async fn read_app_artifact_chunk(
        &self,
        project: String,
        path: String,
        sha256: String,
        bytes: usize,
        byte_offset: usize,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Err(error) = validate_app_artifact_target(&path, bytes, &sha256, byte_offset) {
            return error;
        }
        let length = super::files::INTERNAL_ARTIFACT_TRANSFER_CHUNK_BYTES.min(bytes - byte_offset);
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(error) => return error.into_tool_result(),
        };
        let output = match self
            .read_project_artifact_export_chunk_internal(
                &resolved.resolved_id,
                &path,
                bytes,
                &sha256,
                byte_offset,
                length,
                auth,
            )
            .await
        {
            Ok(output) => output,
            Err(_) => {
                return app_artifact_error(
                    "artifact_read_unavailable",
                    "Artifact could not be read",
                )
            }
        };
        if let Err(error) =
            validate_app_artifact_segment(&output, &path, bytes, byte_offset, length)
        {
            return error;
        }
        let next = byte_offset + length;
        ToolResult::ok(json!({"artifact_chunk": {
            "project": resolved.resolved_id, "path": path, "sha256": sha256,
            "bytes_total": bytes, "byte_offset": byte_offset,
            "next_byte_offset": if next == bytes { None } else { Some(next) },
            "complete": next == bytes, "content_base64": output["content_base64"],
        }}))
    }

    pub(crate) async fn read_pdf_chunk(
        &self,
        project: String,
        path: String,
        sha256: String,
        bytes: usize,
        byte_offset: usize,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Err(error) = validate_pdf_target(&path, bytes, &sha256, byte_offset) {
            return error;
        }
        let length = PDF_CHUNK_BYTES.min(bytes - byte_offset);
        // Reauthorize each read: a digest never replaces ownership or path policy.
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(error) => return error.into_tool_result(),
        };
        let output = match self
            .read_project_artifact_export_chunk_internal(
                &resolved.resolved_id,
                &path,
                bytes,
                &sha256,
                byte_offset,
                length,
                auth,
            )
            .await
        {
            Ok(output) => output,
            Err(_) => return pdf_error("pdf_read_unavailable", "PDF could not be read"),
        };
        if let Err(error) = validate_pdf_segment(&output, &path, bytes, byte_offset, length) {
            return error;
        }
        let next = byte_offset + length;
        ToolResult::ok(json!({"pdf_chunk": {
            "project": resolved.resolved_id, "path": path, "sha256": sha256,
            "bytes_total": bytes, "byte_offset": byte_offset,
            "next_byte_offset": if next == bytes { None } else { Some(next) },
            "complete": next == bytes, "content_base64": output["content_base64"],
        }}))
    }
}
