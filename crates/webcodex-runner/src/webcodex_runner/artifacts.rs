use super::files::sha256_hex_bytes;
use super::output::{line_edit_stdout, CommandResult};
use crate::apply_edits_shared::is_lowercase_hex_sha256 as is_hex_sha256;
use crate::artifact_policy::MAX_MCP_IMAGE_BYTES;
#[cfg(test)]
use crate::runner_protocol::RunnerRequest;
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
#[cfg(test)]
use std::time::{Duration, SystemTime};
use std::time::{Instant, UNIX_EPOCH};
#[cfg(test)]
use webcodex_core::runner_operation::RunnerOperation;
use webcodex_core::runner_operation::{RunnerFileOperation, RunnerFilePayload};

mod inspection;
mod upload;

#[cfg(test)]
use inspection::ARTIFACT_STREAM_BUFFER_BYTES;
use inspection::{
    artifact_mime, artifact_mime_from_file, image_size, magic_mime, read_file_range_with_digest,
    read_limited, verify_upload_file, zip_entry_count,
};
#[cfg(test)]
use upload::{
    commit_artifact_upload_part, enforce_artifact_upload_begin_admission, read_upload_state,
    read_upload_state_file, sweep_artifact_upload_project, upload_paths, write_upload_state,
    ArtifactUploadProjectUsage, ArtifactUploadState, MAX_ACTIVE_ARTIFACT_UPLOADS_PER_PROJECT,
    MAX_ARTIFACT_UPLOAD_BYTES, MAX_ARTIFACT_UPLOAD_CHUNK_BYTES,
    MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT, MAX_ARTIFACT_UPLOAD_STATE_BYTES,
};
use upload::{
    handle_artifact_upload_abort, handle_artifact_upload_begin, handle_artifact_upload_chunk,
    handle_artifact_upload_finish, upload_error,
};

const DEFAULT_MAX_ARTIFACT_BYTES: usize = 10 * 1024 * 1024;
const MAX_ARTIFACT_EXPORT_BYTES: usize = 256 * 1024 * 1024;
const DEFAULT_ARTIFACT_READ_LENGTH: usize = 32 * 1024;
const MAX_ARTIFACT_EXPORT_CHUNK_BYTES: usize = 1024 * 1024;
static ARTIFACT_UPLOAD_STATE_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) fn is_artifact_request_kind(kind: &str) -> bool {
    matches!(
        kind,
        "file_save_project_artifact"
            | "file_read_project_artifact_metadata"
            | "file_read_project_artifact"
            | "file_read_project_artifact_export_chunk"
            | "file_artifact_upload_begin"
            | "file_artifact_upload_chunk"
            | "file_artifact_upload_finish"
            | "file_artifact_upload_abort"
    )
}

pub(crate) fn validate_artifact_runner_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if path.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    let p = Path::new(path);
    let bytes = path.as_bytes();
    let windows_drive_prefix =
        bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if p.has_root() || path.starts_with('\\') || windows_drive_prefix {
        return Err("path must be project-relative".to_string());
    }
    if path.split(['/', '\\']).any(|component| component == "..") {
        return Err("path cannot contain parent traversal".to_string());
    }
    if is_sensitive_artifact_path(path) {
        return Err(format!("refusing sensitive artifact path '{}'", path));
    }
    Ok(())
}

fn is_sensitive_artifact_path(path: &str) -> bool {
    webcodex_core::sensitive_paths::is_bulk_skipped_path(path)
}

fn parse_json_payload(request: &RunnerFilePayload) -> Result<Value, String> {
    let Some(content) = request.content.as_deref() else {
        return Err("invalid json: missing file-op payload".to_string());
    };
    serde_json::from_str(content).map_err(|e| format!("invalid json: {}", e))
}

fn parse_bool_field(payload: &Value, key: &str) -> Result<bool, String> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("{key} must be a boolean")),
    }
}

fn parse_usize_field(payload: &Value, key: &str, default: usize) -> Result<usize, String> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Number(number)) => number
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| format!("{key} must be an integer")),
        Some(Value::String(value)) => value
            .parse::<usize>()
            .map_err(|_| format!("{key} must be an integer")),
        Some(_) => Err(format!("{key} must be an integer")),
    }
}

fn parse_optional_usize_field(payload: &Value, key: &str) -> Result<Option<usize>, String> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .map(Some)
            .ok_or_else(|| format!("{key} must be an integer")),
        Some(Value::String(value)) => value
            .parse::<usize>()
            .map(Some)
            .map_err(|_| format!("{key} must be an integer")),
        Some(_) => Err(format!("{key} must be an integer")),
    }
}

fn parse_optional_clean_string(
    payload: &Value,
    key: &str,
    max_len: usize,
) -> Result<Option<String>, String> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.len() <= max_len && !value.contains('\0') => {
            Ok(Some(value.clone()))
        }
        Some(Value::String(_)) => Err(format!("{key} is invalid")),
        Some(_) => Err(format!("{key} must be a string")),
    }
}

fn parse_required_clean_string(
    payload: &Value,
    key: &str,
    max_len: usize,
) -> Result<String, String> {
    match payload.get(key) {
        Some(Value::String(value)) if value.len() <= max_len && !value.contains('\0') => {
            Ok(value.clone())
        }
        Some(Value::String(_)) => Err(format!("{key} is invalid")),
        Some(_) => Err(format!("{key} must be a string")),
        None => Err(format!("{key} is required")),
    }
}

fn project_root(request: &RunnerFilePayload) -> Result<std::path::PathBuf, String> {
    let Some(cwd) = request.cwd.as_deref() else {
        return Err("artifact request missing project root".to_string());
    };
    std::fs::canonicalize(cwd).map_err(|e| format!("project root does not exist: {}", e))
}

fn resolve_existing_target_in_project_root(
    resolved: &Path,
    root: &Path,
) -> Result<PathBuf, String> {
    let target = std::fs::canonicalize(resolved).map_err(|e| format!("read failed: {}", e))?;
    let relative = target
        .strip_prefix(root)
        .map_err(|_| "artifact path escapes project root".to_string())?;
    if is_sensitive_artifact_path(&relative.to_string_lossy()) {
        return Err("refusing sensitive artifact target".to_string());
    }
    Ok(target)
}

fn ensure_parent_in_project_root(resolved: &Path, root: &Path) -> Result<(), String> {
    let parent = resolved
        .parent()
        .ok_or_else(|| "target path has no parent directory".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("write failed: {}", e))?;
    let parent = std::fs::canonicalize(parent).map_err(|e| format!("write failed: {}", e))?;
    if parent != root && !parent.starts_with(root) {
        return Err("artifact path escapes project root".to_string());
    }
    Ok(())
}

fn ensure_existing_parent_in_project_root(resolved: &Path, root: &Path) -> Result<(), String> {
    let parent = resolved
        .parent()
        .ok_or_else(|| "target path has no parent directory".to_string())?;
    let parent = std::fs::canonicalize(parent).map_err(|e| format!("upload failed: {}", e))?;
    if parent != root && !parent.starts_with(root) {
        return Err("artifact path escapes project root".to_string());
    }
    Ok(())
}

fn write_bytes_atomic_strict(path: &Path, data: &[u8], overwrite: bool) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "target path has no parent directory".to_string())?;
    let mut last_error = None;
    for attempt in 0..16 {
        let tmp = parent.join(format!(".wc-artifact-{}-{}", std::process::id(), attempt));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(data) {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(e.to_string());
                }
                if let Err(e) = file.sync_all() {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(e.to_string());
                }
                drop(file);
                if overwrite {
                    if let Err(e) = std::fs::rename(&tmp, path) {
                        let _ = std::fs::remove_file(&tmp);
                        return Err(e.to_string());
                    }
                } else {
                    match std::fs::hard_link(&tmp, path) {
                        Ok(()) => {
                            // The target link is now committed atomically without replacing an
                            // existing path. Cleanup failure may leave only the private temp link;
                            // it cannot invalidate or change the committed target.
                            let _ = std::fs::remove_file(&tmp);
                        }
                        Err(e) => {
                            let _ = std::fs::remove_file(&tmp);
                            if e.kind() == std::io::ErrorKind::AlreadyExists {
                                return Err("file exists and overwrite is false".to_string());
                            }
                            return Err(e.to_string());
                        }
                    }
                }
                if let Ok(dir) = std::fs::File::open(parent) {
                    let _ = dir.sync_all();
                }
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                last_error = Some(e.to_string());
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Err(last_error.unwrap_or_else(|| "could not create temporary artifact file".to_string()))
}

fn save_error(path: Option<&str>, msg: impl Into<String>) -> Value {
    json!({
        "path": path,
        "bytes_written": 0,
        "sha256": Value::Null,
        "mime_type": Value::Null,
        "error": msg.into(),
    })
}

fn metadata_error(path: Option<&str>, msg: impl Into<String>) -> Value {
    json!({
        "path": path,
        "bytes": 0,
        "sha256": Value::Null,
        "mime_type": Value::Null,
        "error": msg.into(),
    })
}

fn read_error(path: Option<&str>, msg: impl Into<String>) -> Value {
    json!({
        "path": path,
        "mime_type": Value::Null,
        "file_bytes": 0,
        "sha256": Value::Null,
        "offset": 0,
        "bytes_returned": 0,
        "content_base64": "",
        "next_offset": 0,
        "truncated": false,
        "eof": false,
        "error": msg.into(),
    })
}

fn read_snapshot_changed(path: &str, expected_sha256: &str, actual_sha256: &str) -> Value {
    let mut output = read_error(Some(path), "artifact snapshot changed");
    output["error_kind"] = json!("snapshot_changed");
    output["expected_sha256"] = json!(expected_sha256);
    output["actual_sha256"] = json!(actual_sha256);
    output
}

pub(crate) fn handle_artifact_file_operation(
    operation: &RunnerFileOperation,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let request = operation.payload();
    match operation {
        RunnerFileOperation::SaveProjectArtifact(_) => {
            handle_save_project_artifact(request, resolved, start)
        }
        RunnerFileOperation::ReadProjectArtifactMetadata(_) => {
            handle_read_project_artifact_metadata(request, resolved, start)
        }
        RunnerFileOperation::ReadProjectArtifact(_) => {
            handle_read_project_artifact(request, resolved, start)
        }
        RunnerFileOperation::ReadProjectArtifactExportChunk(_) => {
            handle_read_project_artifact_export_chunk(request, resolved, start)
        }
        RunnerFileOperation::ArtifactUploadBegin(_)
        | RunnerFileOperation::ArtifactUploadChunk(_)
        | RunnerFileOperation::ArtifactUploadFinish(_)
        | RunnerFileOperation::ArtifactUploadAbort(_) => {
            let _upload_guard = match ARTIFACT_UPLOAD_STATE_LOCK.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    return line_edit_stdout(
                        upload_error(
                            Some(request.path.as_str()),
                            None,
                            "artifact upload state lock unavailable",
                        ),
                        start,
                    )
                }
            };
            match operation {
                RunnerFileOperation::ArtifactUploadBegin(_) => {
                    handle_artifact_upload_begin(request, resolved, start)
                }
                RunnerFileOperation::ArtifactUploadChunk(_) => {
                    handle_artifact_upload_chunk(request, resolved, start)
                }
                RunnerFileOperation::ArtifactUploadFinish(_) => {
                    handle_artifact_upload_finish(request, resolved, start)
                }
                RunnerFileOperation::ArtifactUploadAbort(_) => {
                    handle_artifact_upload_abort(request, resolved, start)
                }
                _ => unreachable!("upload operation already typed"),
            }
        }
        _ => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some("file operation is not an artifact request".to_string()),
        },
    }
}

#[cfg(test)]
pub(crate) fn handle_artifact_file_request(
    request: &RunnerRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    match request.decode_operation() {
        Ok(RunnerOperation::File(operation)) => {
            handle_artifact_file_operation(&operation, resolved, start)
        }
        _ => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some("invalid artifact wire request".to_string()),
        },
    }
}

fn handle_save_project_artifact(
    request: &RunnerFilePayload,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_str();
    let payload = match parse_json_payload(request) {
        Ok(payload) => payload,
        Err(e) => return line_edit_stdout(save_error(None, e), start),
    };
    if let Err(e) = validate_artifact_runner_path(path) {
        return line_edit_stdout(save_error(Some(path), e), start);
    }
    let root = match project_root(request) {
        Ok(root) => root,
        Err(e) => return line_edit_stdout(save_error(Some(path), e), start),
    };
    let content_base64 = match payload.get("content_base64").and_then(Value::as_str) {
        Some(value) if !value.contains('\0') => value,
        _ => {
            return line_edit_stdout(
                save_error(
                    Some(path),
                    "content_base64 must be a base64 string without NUL",
                ),
                start,
            )
        }
    };
    let data = match general_purpose::STANDARD.decode(content_base64.as_bytes()) {
        Ok(data) => data,
        Err(e) => {
            return line_edit_stdout(
                save_error(Some(path), format!("invalid base64: {e}")),
                start,
            )
        }
    };
    let max_bytes = match parse_usize_field(&payload, "max_bytes", DEFAULT_MAX_ARTIFACT_BYTES) {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(save_error(Some(path), e), start),
    };
    if data.len() > max_bytes {
        return line_edit_stdout(save_error(Some(path), "decoded artifact too large"), start);
    }
    let overwrite = match parse_bool_field(&payload, "overwrite") {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(save_error(Some(path), e), start),
    };
    let mime_type = payload
        .get("mime_type")
        .filter(|value| !value.is_null())
        .and_then(Value::as_str);

    let exists = std::fs::symlink_metadata(resolved).is_ok();
    if exists && !overwrite {
        return line_edit_stdout(
            save_error(Some(path), "file exists and overwrite is false"),
            start,
        );
    }
    if exists
        && std::fs::symlink_metadata(resolved)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
    {
        return line_edit_stdout(
            save_error(Some(path), "refusing to overwrite symlink artifact path"),
            start,
        );
    }
    if let Err(e) = ensure_parent_in_project_root(resolved, &root) {
        return line_edit_stdout(save_error(Some(path), e), start);
    }
    if std::fs::symlink_metadata(resolved)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return line_edit_stdout(
            save_error(Some(path), "refusing to overwrite symlink artifact path"),
            start,
        );
    }
    if let Err(e) = write_bytes_atomic_strict(resolved, &data, overwrite) {
        return line_edit_stdout(save_error(Some(path), format!("write failed: {e}")), start);
    }

    line_edit_stdout(
        json!({
            "path": path,
            "bytes_written": data.len(),
            "sha256": sha256_hex_bytes(&data),
            "mime_type": mime_type,
        }),
        start,
    )
}

fn handle_read_project_artifact_metadata(
    request: &RunnerFilePayload,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_str();
    let payload = match parse_json_payload(request) {
        Ok(payload) => payload,
        Err(e) => return line_edit_stdout(metadata_error(None, e), start),
    };
    if let Err(e) = validate_artifact_runner_path(path) {
        return line_edit_stdout(metadata_error(Some(path), e), start);
    }
    let root = match project_root(request) {
        Ok(root) => root,
        Err(e) => return line_edit_stdout(metadata_error(Some(path), e), start),
    };
    let allow_missing = match parse_bool_field(&payload, "allow_missing") {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(metadata_error(Some(path), e), start),
    };
    let target = match resolve_existing_target_in_project_root(resolved, &root) {
        Ok(target) => target,
        Err(e) => {
            let target_missing = matches!(
                std::fs::symlink_metadata(resolved),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound
            );
            if allow_missing && target_missing {
                return line_edit_stdout(
                    json!({
                        "path": path,
                        "exists": false,
                        "missing": true,
                    }),
                    start,
                );
            }
            return line_edit_stdout(metadata_error(Some(path), e), start);
        }
    };
    let resolved = target.as_path();
    let max_bytes = match parse_usize_field(&payload, "max_bytes", DEFAULT_MAX_ARTIFACT_BYTES) {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(metadata_error(Some(path), e), start),
    };
    if max_bytes > MAX_ARTIFACT_EXPORT_BYTES {
        return line_edit_stdout(
            metadata_error(
                Some(path),
                format!("artifact metadata maximum exceeds {MAX_ARTIFACT_EXPORT_BYTES} bytes"),
            ),
            start,
        );
    }
    if max_bytes > DEFAULT_MAX_ARTIFACT_BYTES {
        let initial_bytes = match std::fs::metadata(resolved)
            .ok()
            .and_then(|metadata| usize::try_from(metadata.len()).ok())
        {
            Some(bytes) => bytes,
            None => {
                return line_edit_stdout(
                    metadata_error(Some(path), "artifact size does not fit this platform"),
                    start,
                )
            }
        };
        if initial_bytes > max_bytes {
            return line_edit_stdout(
                metadata_error(Some(path), "artifact too large to inspect"),
                start,
            );
        }
        let (bytes, sha256) = match verify_upload_file(resolved, max_bytes) {
            Ok(verification) => verification,
            Err(e) => return line_edit_stdout(metadata_error(Some(path), e), start),
        };
        let final_metadata = match std::fs::metadata(resolved) {
            Ok(metadata) => metadata,
            Err(e) => {
                return line_edit_stdout(
                    metadata_error(Some(path), format!("stat failed: {e}")),
                    start,
                )
            }
        };
        let final_bytes = match usize::try_from(final_metadata.len()) {
            Ok(bytes) => bytes,
            Err(_) => {
                return line_edit_stdout(
                    metadata_error(Some(path), "artifact size does not fit this platform"),
                    start,
                )
            }
        };
        if bytes != initial_bytes || bytes != final_bytes {
            return line_edit_stdout(
                metadata_error(
                    Some(path),
                    "artifact size changed during metadata inspection",
                ),
                start,
            );
        }
        let mime_type = artifact_mime_from_file(path, resolved, false);
        let mut out = json!({
            "path": path,
            "exists": true,
            "missing": false,
            "bytes": bytes,
            "sha256": sha256,
            "mime_type": mime_type,
        });
        if let Ok(modified) = final_metadata.modified().and_then(|modified| {
            modified
                .duration_since(UNIX_EPOCH)
                .map_err(std::io::Error::other)
        }) {
            out["modified_at"] = json!(modified.as_secs());
        }
        return line_edit_stdout(out, start);
    }
    let data = match read_limited(resolved, max_bytes) {
        Ok(data) => data,
        Err(e) => return line_edit_stdout(metadata_error(Some(path), e), start),
    };
    let mime_type = artifact_mime(path, &data, false);
    let mut out = json!({
        "path": path,
        "exists": true,
        "missing": false,
        "bytes": data.len(),
        "sha256": sha256_hex_bytes(&data),
        "mime_type": mime_type,
    });
    if let Ok(modified) = std::fs::metadata(resolved)
        .and_then(|metadata| metadata.modified())
        .and_then(|modified| {
            modified
                .duration_since(UNIX_EPOCH)
                .map_err(std::io::Error::other)
        })
    {
        out["modified_at"] = json!(modified.as_secs());
    }
    if let Some((width, height)) = image_size(&data) {
        out["width"] = json!(width);
        out["height"] = json!(height);
    }
    if out["mime_type"].as_str() == Some("application/zip") {
        out["archive_entries_count"] = json!(zip_entry_count(&data));
    }
    line_edit_stdout(out, start)
}

fn handle_read_project_artifact_export_chunk(
    request: &RunnerFilePayload,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_str();
    let payload = match parse_json_payload(request) {
        Ok(payload) => payload,
        Err(e) => return line_edit_stdout(read_error(None, e), start),
    };
    if let Err(e) = validate_artifact_runner_path(path) {
        return line_edit_stdout(read_error(Some(path), e), start);
    }
    let root = match project_root(request) {
        Ok(root) => root,
        Err(e) => return line_edit_stdout(read_error(Some(path), e), start),
    };
    let target = match resolve_existing_target_in_project_root(resolved, &root) {
        Ok(target) => target,
        Err(e) => {
            let msg = e.replacen("read failed", "stat failed", 1);
            return line_edit_stdout(read_error(Some(path), msg), start);
        }
    };
    let resolved = target.as_path();
    if payload.get("expected_file_bytes").is_none() {
        return line_edit_stdout(
            read_error(Some(path), "expected_file_bytes is required"),
            start,
        );
    }
    let expected_file_bytes = match parse_usize_field(&payload, "expected_file_bytes", 0) {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(read_error(Some(path), e), start),
    };
    let expected_sha256 =
        match payload.get("expected_sha256") {
            Some(Value::String(value)) if is_hex_sha256(value) => value.as_str(),
            _ => return line_edit_stdout(
                read_error(
                    Some(path),
                    "expected_sha256 is required and must be a lowercase 64-character hex digest",
                ),
                start,
            ),
        };
    if expected_file_bytes > MAX_ARTIFACT_EXPORT_BYTES {
        return line_edit_stdout(
            read_error(
                Some(path),
                format!(
                    "artifact is too large to export; maximum is {} bytes",
                    MAX_ARTIFACT_EXPORT_BYTES
                ),
            ),
            start,
        );
    }
    let offset = match parse_usize_field(&payload, "offset", 0) {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(read_error(Some(path), e), start),
    };
    let length = match parse_usize_field(&payload, "length", DEFAULT_ARTIFACT_READ_LENGTH) {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(read_error(Some(path), e), start),
    };
    if length == 0 || length > MAX_ARTIFACT_EXPORT_CHUNK_BYTES {
        return line_edit_stdout(
            read_error(
                Some(path),
                format!(
                    "length must be between 1 and {} bytes",
                    MAX_ARTIFACT_EXPORT_CHUNK_BYTES
                ),
            ),
            start,
        );
    }
    if offset > expected_file_bytes {
        return line_edit_stdout(
            read_error(Some(path), "offset exceeds artifact size"),
            start,
        );
    }
    let requested_end = match offset.checked_add(length) {
        Some(value) => value,
        None => return line_edit_stdout(read_error(Some(path), "offset + length overflow"), start),
    };
    let (file_bytes, actual_sha256, segment) =
        match read_file_range_with_digest(resolved, MAX_ARTIFACT_EXPORT_BYTES, offset, length) {
            Ok(result) => result,
            Err(e) => return line_edit_stdout(read_error(Some(path), e), start),
        };
    if file_bytes != expected_file_bytes || actual_sha256 != expected_sha256 {
        let mut output = read_snapshot_changed(path, expected_sha256, &actual_sha256);
        output["expected_file_bytes"] = json!(expected_file_bytes);
        output["actual_file_bytes"] = json!(file_bytes);
        return line_edit_stdout(output, start);
    }
    let next_offset = requested_end.min(file_bytes);
    if segment.len() != next_offset.saturating_sub(offset) {
        return line_edit_stdout(
            read_error(Some(path), "artifact range length changed during export"),
            start,
        );
    }
    let truncated = next_offset < file_bytes;
    line_edit_stdout(
        json!({
            "path": path,
            "file_bytes": file_bytes,
            "offset": offset,
            "bytes_returned": segment.len(),
            "content_base64": general_purpose::STANDARD.encode(segment),
            "next_offset": next_offset,
            "truncated": truncated,
            "eof": !truncated,
        }),
        start,
    )
}

fn handle_read_project_artifact(
    request: &RunnerFilePayload,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_str();
    let payload = match parse_json_payload(request) {
        Ok(payload) => payload,
        Err(e) => return line_edit_stdout(read_error(None, e), start),
    };
    if let Err(e) = validate_artifact_runner_path(path) {
        return line_edit_stdout(read_error(Some(path), e), start);
    }
    let root = match project_root(request) {
        Ok(root) => root,
        Err(e) => return line_edit_stdout(read_error(Some(path), e), start),
    };
    let expected_sha256 = match payload.get("expected_sha256") {
        None => None,
        Some(Value::String(value)) if is_hex_sha256(value) => Some(value.as_str()),
        _ => {
            return line_edit_stdout(
                read_error(
                    Some(path),
                    "expected_sha256 must be a lowercase 64-character hex digest",
                ),
                start,
            )
        }
    };
    let target = match resolve_existing_target_in_project_root(resolved, &root) {
        Ok(target) => target,
        Err(e) => {
            let msg = e.replacen("read failed", "stat failed", 1);
            return line_edit_stdout(read_error(Some(path), msg), start);
        }
    };
    let resolved = target.as_path();
    let offset = match parse_usize_field(&payload, "offset", 0) {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(read_error(Some(path), e), start),
    };
    let mcp_image = match parse_bool_field(&payload, "mcp_image") {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(read_error(Some(path), e), start),
    };
    if mcp_image && offset != 0 {
        return line_edit_stdout(
            read_error(Some(path), "MCP image reads must start at offset 0"),
            start,
        );
    }
    let requested_length = match parse_usize_field(&payload, "length", DEFAULT_ARTIFACT_READ_LENGTH)
    {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(read_error(Some(path), e), start),
    };
    let length = if mcp_image {
        MAX_MCP_IMAGE_BYTES
    } else {
        requested_length
    };
    if length < 1 {
        return line_edit_stdout(read_error(Some(path), "length must be >= 1"), start);
    }
    let requested_max_file_bytes =
        match parse_usize_field(&payload, "max_file_bytes", DEFAULT_MAX_ARTIFACT_BYTES) {
            Ok(value) => value,
            Err(e) => return line_edit_stdout(read_error(Some(path), e), start),
        };
    let max_file_bytes = if mcp_image {
        requested_max_file_bytes.min(MAX_MCP_IMAGE_BYTES)
    } else {
        requested_max_file_bytes
    };
    if max_file_bytes < 1 {
        return line_edit_stdout(read_error(Some(path), "max_file_bytes must be >= 1"), start);
    }
    let file_bytes = match std::fs::metadata(resolved) {
        Ok(metadata) => metadata.len(),
        Err(e) => {
            return line_edit_stdout(read_error(Some(path), format!("stat failed: {e}")), start)
        }
    };
    if file_bytes > max_file_bytes as u64 {
        let message = if mcp_image {
            format!(
                "MCP image too large; maximum is {} bytes",
                MAX_MCP_IMAGE_BYTES
            )
        } else {
            "artifact too large to read; use metadata or a smaller artifact".to_string()
        };
        return line_edit_stdout(read_error(Some(path), message), start);
    }
    let data = match std::fs::read(resolved) {
        Ok(data) => data,
        Err(e) => {
            return line_edit_stdout(read_error(Some(path), format!("read failed: {e}")), start)
        }
    };
    let actual_sha256 = sha256_hex_bytes(&data);
    if let Some(expected_sha256) = expected_sha256 {
        if actual_sha256 != expected_sha256 {
            return line_edit_stdout(
                read_snapshot_changed(path, expected_sha256, &actual_sha256),
                start,
            );
        }
    }
    let mime_type = if mcp_image {
        match magic_mime(&data) {
            Some(mime @ ("image/png" | "image/jpeg" | "image/webp")) => Some(mime.to_string()),
            Some(mime) => {
                return line_edit_stdout(
                    read_error(
                        Some(path),
                        format!(
                            "unsupported MCP image MIME type '{mime}'; supported types are image/png, image/jpeg, and image/webp"
                        ),
                    ),
                    start,
                )
            }
            None => {
                return line_edit_stdout(
                    read_error(
                        Some(path),
                        "artifact content is not a supported PNG, JPEG, or WebP image",
                    ),
                    start,
                )
            }
        }
    } else {
        artifact_mime(path, &data, true)
    };
    let file_bytes = data.len();
    let (segment, next_offset, truncated) = if offset >= file_bytes {
        (&[][..], file_bytes, false)
    } else {
        let next_offset = offset.saturating_add(length).min(file_bytes);
        (
            &data[offset..next_offset],
            next_offset,
            next_offset < file_bytes,
        )
    };
    line_edit_stdout(
        json!({
            "path": path,
            "mime_type": mime_type,
            "file_bytes": file_bytes,
            "sha256": actual_sha256,
            "offset": offset,
            "bytes_returned": segment.len(),
            "content_base64": general_purpose::STANDARD.encode(segment),
            "next_offset": next_offset,
            "truncated": truncated,
            "eof": !truncated,
        }),
        start,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_runner_path_validation_is_cross_platform_and_uses_shared_sensitive_policy() {
        assert!(validate_artifact_runner_path("artifacts/report.bin").is_ok());
        for path in [
            "/absolute/report.bin",
            "\\rooted\\report.bin",
            "C:\\absolute\\report.bin",
            "C:drive-relative\\report.bin",
            "../report.bin",
            "nested\\..\\report.bin",
            ".git\\config",
            "secrets\\token.bin",
            "certs\\server.key",
            "config\\runner.toml",
            "project-registry\\demo.toml",
        ] {
            assert!(
                validate_artifact_runner_path(path).is_err(),
                "{path} should be rejected"
            );
        }
    }

    #[test]
    fn atomic_artifact_write_create_only_never_replaces_existing_target() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("artifact.bin");

        write_bytes_atomic_strict(&path, b"first", false).unwrap();
        let error = write_bytes_atomic_strict(&path, b"second", false).unwrap_err();
        assert!(error.contains("overwrite is false"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), b"first");

        write_bytes_atomic_strict(&path, b"third", true).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"third");
    }

    #[test]
    fn artifact_upload_create_only_commit_never_replaces_existing_target() {
        let temp = tempfile::tempdir().unwrap();
        let part = temp.path().join(".wc-upload-test.part");
        let target = temp.path().join("artifact.bin");
        std::fs::write(&part, b"upload").unwrap();
        std::fs::write(&target, b"concurrent").unwrap();

        let error = commit_artifact_upload_part(&part, &target, false).unwrap_err();
        assert_eq!(error, "file exists and overwrite is false");
        assert_eq!(std::fs::read(&target).unwrap(), b"concurrent");
        assert_eq!(std::fs::read(&part).unwrap(), b"upload");

        std::fs::remove_file(&target).unwrap();
        commit_artifact_upload_part(&part, &target, false).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"upload");
        assert!(!part.exists());
    }

    fn artifact_request(root: &Path, kind: &str, path: &str, payload: Value) -> RunnerRequest {
        RunnerRequest {
            shell: None,
            request_id: format!("req-{kind}"),
            client_id: "agent-1".to_string(),
            kind: kind.to_string(),
            job_id: None,
            cwd: Some(root.to_string_lossy().to_string()),
            path: Some(path.to_string()),
            content: Some(payload.to_string()),
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: String::new(),
            process: None,
            script: None,
            stdin: None,
            timeout_secs: 30,
            requested_by: "tester".to_string(),
            created_at: 0,
            validation: None,
            lsp: None,
            job_context: None,
            mcp_gateway: None,
            plugin_gateway: None,
            coding_agent: None,
            persistent_shell: None,
        }
    }

    fn artifact_output(result: CommandResult) -> Value {
        assert_eq!(result.exit_code, Some(0), "unexpected result: {result:?}");
        assert!(
            result.error.is_none(),
            "unexpected error: {:?}",
            result.error
        );
        serde_json::from_str(result.stdout.as_deref().expect("json stdout")).unwrap()
    }

    fn run_artifact_request(root: &Path, kind: &str, path: &str, payload: Value) -> Value {
        let request = artifact_request(root, kind, path, payload);
        let resolved = root.join(path);
        artifact_output(handle_artifact_file_request(
            &request,
            &resolved,
            Instant::now(),
        ))
    }

    fn test_upload_state(path: &str) -> ArtifactUploadState {
        ArtifactUploadState {
            path: path.to_string(),
            expected_bytes: None,
            expected_sha256: None,
            mime_type: None,
            overwrite: false,
            max_bytes: MAX_ARTIFACT_UPLOAD_BYTES,
        }
    }

    fn write_test_upload_pair(parent: &Path, upload_id: &str, path: &str, bytes: &[u8]) {
        let (part, sidecar) = upload_paths(parent, upload_id);
        std::fs::write(&part, bytes).unwrap();
        write_upload_state(&sidecar, &test_upload_state(path)).unwrap();
    }

    #[test]
    fn verify_upload_file_streams_across_multiple_buffers() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("large.part");
        let bytes = vec![b'x'; ARTIFACT_STREAM_BUFFER_BYTES * 2 + 17];
        std::fs::write(&path, &bytes).unwrap();

        let (verified_bytes, sha256) = verify_upload_file(&path, bytes.len()).unwrap();
        assert_eq!(verified_bytes, bytes.len());
        assert_eq!(sha256, sha256_hex_bytes(&bytes));
        assert_eq!(
            verify_upload_file(&path, bytes.len() - 1).unwrap_err(),
            "artifact too large to inspect"
        );
    }

    #[test]
    fn artifact_upload_project_sweep_cleans_orphans_and_stale_pairs() {
        let tmp = tempfile::tempdir().unwrap();
        let active_parent = tmp.path().join("artifacts/active");
        let orphan_parent = tmp.path().join("artifacts/orphans");
        std::fs::create_dir_all(&active_parent).unwrap();
        std::fs::create_dir_all(&orphan_parent).unwrap();
        let active_id = "wc_upload_active";
        write_test_upload_pair(
            &active_parent,
            active_id,
            "artifacts/active/active.bin",
            b"abc",
        );

        let (orphan_part, _) = upload_paths(&orphan_parent, "wc_upload_orphan_part");
        std::fs::write(&orphan_part, b"orphan").unwrap();
        let (_, orphan_sidecar) = upload_paths(&orphan_parent, "wc_upload_orphan_sidecar");
        write_upload_state(
            &orphan_sidecar,
            &test_upload_state("artifacts/orphans/orphan.bin"),
        )
        .unwrap();

        let now = SystemTime::now();
        let usage =
            sweep_artifact_upload_project(tmp.path(), now, Duration::from_secs(60)).unwrap();
        assert_eq!(
            usage,
            ArtifactUploadProjectUsage {
                active_uploads: 1,
                reserved_bytes: MAX_ARTIFACT_UPLOAD_BYTES,
            }
        );
        assert!(!orphan_part.exists());
        assert!(!orphan_sidecar.exists());

        let future = now + Duration::from_secs(61);
        let expired =
            sweep_artifact_upload_project(tmp.path(), future, Duration::from_secs(60)).unwrap();
        assert_eq!(expired, ArtifactUploadProjectUsage::default());
        let (part, sidecar) = upload_paths(&active_parent, active_id);
        assert!(!part.exists());
        assert!(!sidecar.exists());
    }

    #[test]
    fn artifact_upload_project_limits_bound_count_and_reserved_bytes() {
        let at_count_limit = ArtifactUploadProjectUsage {
            active_uploads: MAX_ACTIVE_ARTIFACT_UPLOADS_PER_PROJECT,
            reserved_bytes: 0,
        };
        assert!(enforce_artifact_upload_begin_admission(&at_count_limit, 1)
            .unwrap_err()
            .contains("active upload limit"));

        let nearly_full = ArtifactUploadProjectUsage {
            active_uploads: 1,
            reserved_bytes: MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT - 1,
        };
        enforce_artifact_upload_begin_admission(&nearly_full, 1).unwrap();
        assert!(enforce_artifact_upload_begin_admission(&nearly_full, 2)
            .unwrap_err()
            .contains("reserved byte quota exceeded"));
    }

    #[test]
    fn artifact_upload_begin_enforces_per_file_maximum() {
        let tmp = tempfile::tempdir().unwrap();
        let accepted_path = "artifacts/imports/max-file.bin";
        let accepted = run_artifact_request(
            tmp.path(),
            "file_artifact_upload_begin",
            accepted_path,
            json!({
                "path": accepted_path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": MAX_ARTIFACT_UPLOAD_BYTES,
            }),
        );
        assert_eq!(accepted["max_bytes"], MAX_ARTIFACT_UPLOAD_BYTES);
        let upload_id = accepted["upload_id"].as_str().unwrap().to_string();
        let aborted = run_artifact_request(
            tmp.path(),
            "file_artifact_upload_abort",
            accepted_path,
            json!({"path": accepted_path, "upload_id": upload_id}),
        );
        assert_eq!(aborted["aborted"], true);

        let rejected_path = "artifacts/imports/too-large-file.bin";
        let rejected = run_artifact_request(
            tmp.path(),
            "file_artifact_upload_begin",
            rejected_path,
            json!({
                "path": rejected_path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": MAX_ARTIFACT_UPLOAD_BYTES + 1,
            }),
        );
        assert_eq!(
            rejected["error"],
            format!("max_bytes exceeds upload maximum ({MAX_ARTIFACT_UPLOAD_BYTES})")
        );
    }

    #[test]
    fn artifact_upload_chunk_enforces_data_plane_maximum() {
        let tmp = tempfile::tempdir().unwrap();
        let path = "artifacts/imports/max-chunk.bin";
        let begin = run_artifact_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            json!({
                "path": path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": MAX_ARTIFACT_UPLOAD_CHUNK_BYTES * 2,
            }),
        );
        let upload_id = begin["upload_id"].as_str().unwrap().to_string();

        let rejected = run_artifact_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": "YQ==",
                "max_chunk_bytes": MAX_ARTIFACT_UPLOAD_CHUNK_BYTES + 1,
            }),
        );
        assert_eq!(
            rejected["error"],
            format!(
                "max_chunk_bytes exceeds upload chunk maximum ({MAX_ARTIFACT_UPLOAD_CHUNK_BYTES})"
            )
        );
        let (part, _) = upload_paths(tmp.path().join("artifacts/imports").as_path(), &upload_id);
        assert_eq!(std::fs::metadata(&part).unwrap().len(), 0);

        let bytes = vec![b'x'; MAX_ARTIFACT_UPLOAD_CHUNK_BYTES];
        let accepted = run_artifact_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": general_purpose::STANDARD.encode(&bytes),
                "max_chunk_bytes": MAX_ARTIFACT_UPLOAD_CHUNK_BYTES,
            }),
        );
        assert_eq!(accepted["received_bytes"], MAX_ARTIFACT_UPLOAD_CHUNK_BYTES);

        let aborted = run_artifact_request(
            tmp.path(),
            "file_artifact_upload_abort",
            path,
            json!({"path": path, "upload_id": upload_id}),
        );
        assert_eq!(aborted["aborted"], true);
    }

    #[test]
    fn artifact_upload_begin_rejects_project_active_upload_limit_across_directories() {
        let tmp = tempfile::tempdir().unwrap();
        for index in 0..MAX_ACTIVE_ARTIFACT_UPLOADS_PER_PROJECT {
            let parent = tmp.path().join(format!("artifacts/set-{index}"));
            std::fs::create_dir_all(&parent).unwrap();
            let upload_id = format!("wc_upload_limit_{index}");
            write_test_upload_pair(
                &parent,
                &upload_id,
                &format!("artifacts/set-{index}/existing.bin"),
                b"",
            );
        }

        let path = "artifacts/imports/new.bin";
        let output = run_artifact_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            json!({
                "path": path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": DEFAULT_MAX_ARTIFACT_BYTES,
            }),
        );
        assert!(output["error"]
            .as_str()
            .unwrap()
            .contains("active upload limit"));
    }

    #[test]
    fn artifact_upload_begin_rejects_project_reserved_byte_quota() {
        let tmp = tempfile::tempdir().unwrap();
        for index in 0..2 {
            let parent = tmp.path().join(format!("artifacts/quota-{index}"));
            std::fs::create_dir_all(&parent).unwrap();
            let upload_id = format!("wc_upload_quota_{index}");
            write_test_upload_pair(
                &parent,
                &upload_id,
                &format!("artifacts/quota-{index}/existing.bin"),
                b"",
            );
        }

        let path = "artifacts/imports/new.bin";
        let output = run_artifact_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            json!({
                "path": path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": DEFAULT_MAX_ARTIFACT_BYTES,
            }),
        );
        assert!(output["error"]
            .as_str()
            .unwrap()
            .contains("reserved byte quota exceeded"));
    }

    #[test]
    fn read_upload_state_rejects_requested_path_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let (_part, sidecar) = upload_paths(tmp.path(), "wc_upload_test_1");
        write_upload_state(
            &sidecar,
            &ArtifactUploadState {
                path: "artifacts/imports/a.bin".to_string(),
                expected_bytes: None,
                expected_sha256: None,
                mime_type: None,
                overwrite: false,
                max_bytes: DEFAULT_MAX_ARTIFACT_BYTES,
            },
        )
        .unwrap();

        let err = read_upload_state(&sidecar, "artifacts/imports/b.bin").unwrap_err();
        assert_eq!(err, "upload_id does not belong to requested path");
    }

    #[test]
    fn read_upload_state_rejects_oversized_sidecar() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, sidecar) = upload_paths(tmp.path(), "wc_upload_oversized_state");
        std::fs::write(&sidecar, vec![b'x'; MAX_ARTIFACT_UPLOAD_STATE_BYTES + 1]).unwrap();

        let err = read_upload_state_file(&sidecar).unwrap_err();
        assert_eq!(err, "invalid upload state: sidecar too large");
    }

    #[test]
    fn artifact_upload_chunk_rejects_legacy_oversized_reservation() {
        let tmp = tempfile::tempdir().unwrap();
        let path = "artifacts/imports/legacy-oversized.bin";
        let parent = tmp.path().join("artifacts/imports");
        std::fs::create_dir_all(&parent).unwrap();
        let upload_id = "wc_upload_legacy_oversized";
        let (part, sidecar) = upload_paths(&parent, upload_id);
        std::fs::write(&part, b"").unwrap();
        write_upload_state(
            &sidecar,
            &ArtifactUploadState {
                path: path.to_string(),
                expected_bytes: None,
                expected_sha256: None,
                mime_type: None,
                overwrite: false,
                max_bytes: MAX_ARTIFACT_UPLOAD_BYTES + 1,
            },
        )
        .unwrap();

        let output = run_artifact_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            json!({
                "path": path,
                "upload_id": upload_id,
                "offset": 0,
                "content_base64": "YQ==",
                "max_chunk_bytes": MAX_ARTIFACT_UPLOAD_CHUNK_BYTES,
            }),
        );
        assert_eq!(
            output["error"],
            "upload max_bytes exceeds per-file upload maximum"
        );
        assert_eq!(std::fs::metadata(&part).unwrap().len(), 0);
        assert!(sidecar.exists());
        assert!(!tmp.path().join(path).exists());
    }

    #[test]
    fn read_project_artifact_metadata_allow_missing_returns_successful_absence() {
        let tmp = tempfile::tempdir().unwrap();
        let path = "artifacts/smoke/missing.artifact";

        let missing = run_artifact_request(
            tmp.path(),
            "file_read_project_artifact_metadata",
            path,
            json!({"path": path, "allow_missing": true}),
        );

        assert_eq!(missing["path"], path);
        assert_eq!(missing["exists"], false);
        assert_eq!(missing["missing"], true);
        assert!(missing.get("error").is_none());
    }

    #[test]
    fn read_project_artifact_metadata_missing_without_allow_missing_keeps_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = "artifacts/smoke/missing.artifact";

        let missing = run_artifact_request(
            tmp.path(),
            "file_read_project_artifact_metadata",
            path,
            json!({"path": path}),
        );

        assert_eq!(missing["path"], path);
        assert!(missing["error"].as_str().unwrap().contains("read failed"));
        assert!(missing.get("exists").is_none());
    }

    #[test]
    fn read_project_artifact_metadata_existing_reports_exists_with_allow_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = "artifacts/smoke/existing.artifact";
        let resolved = tmp.path().join(path);
        std::fs::create_dir_all(resolved.parent().unwrap()).unwrap();
        std::fs::write(&resolved, b"hello").unwrap();

        let metadata = run_artifact_request(
            tmp.path(),
            "file_read_project_artifact_metadata",
            path,
            json!({"path": path, "allow_missing": true}),
        );

        assert_eq!(metadata["exists"], true);
        assert_eq!(metadata["missing"], false);
        assert_eq!(metadata["bytes"], 5);
        assert_eq!(metadata["sha256"], sha256_hex_bytes(b"hello"));
    }

    #[test]
    fn read_project_artifact_mcp_image_returns_complete_supported_formats() {
        let tmp = tempfile::tempdir().unwrap();
        let cases: [(&str, &[u8], &str); 3] = [
            (
                "docs/images/sample.png",
                b"\x89PNG\r\n\x1a\npng-body",
                "image/png",
            ),
            (
                "docs/images/sample.jpg",
                b"\xff\xd8\xff\xe0jpeg-body\xff\xd9",
                "image/jpeg",
            ),
            (
                "docs/images/sample.webp",
                b"RIFF\x08\x00\x00\x00WEBPwebp-body",
                "image/webp",
            ),
        ];

        for (path, bytes, expected_mime) in cases {
            let resolved = tmp.path().join(path);
            std::fs::create_dir_all(resolved.parent().unwrap()).unwrap();
            std::fs::write(&resolved, bytes).unwrap();
            let output = run_artifact_request(
                tmp.path(),
                "file_read_project_artifact",
                path,
                json!({
                    "path": path,
                    "offset": 0,
                    "length": MAX_MCP_IMAGE_BYTES,
                    "max_file_bytes": MAX_MCP_IMAGE_BYTES,
                    "mcp_image": true,
                }),
            );

            assert_eq!(output["mime_type"], expected_mime);
            assert_eq!(output["file_bytes"], bytes.len());
            assert_eq!(output["bytes_returned"], bytes.len());
            assert_eq!(
                output["content_base64"],
                general_purpose::STANDARD.encode(bytes)
            );
            assert_eq!(output["offset"], 0);
            assert_eq!(output["next_offset"], bytes.len());
            assert_eq!(output["truncated"], false);
            assert_eq!(output["eof"], true);
        }
    }

    #[test]
    fn read_project_artifact_mcp_image_fails_safely() {
        let tmp = tempfile::tempdir().unwrap();
        let unsupported_path = "docs/images/not-an-image.png";
        let unsupported = tmp.path().join(unsupported_path);
        std::fs::create_dir_all(unsupported.parent().unwrap()).unwrap();
        std::fs::write(&unsupported, b"%PDF-1.7\n").unwrap();
        let unsupported_output = run_artifact_request(
            tmp.path(),
            "file_read_project_artifact",
            unsupported_path,
            json!({
                "path": unsupported_path,
                "mcp_image": true,
            }),
        );
        assert!(unsupported_output["error"]
            .as_str()
            .unwrap()
            .contains("unsupported MCP image MIME type 'application/pdf'"));

        let too_large_path = "docs/images/too-large.png";
        let too_large = tmp.path().join(too_large_path);
        let mut oversized_bytes = vec![0u8; MAX_MCP_IMAGE_BYTES + 1];
        oversized_bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        std::fs::write(&too_large, oversized_bytes).unwrap();
        let too_large_output = run_artifact_request(
            tmp.path(),
            "file_read_project_artifact",
            too_large_path,
            json!({
                "path": too_large_path,
                "max_file_bytes": MAX_MCP_IMAGE_BYTES * 2,
                "mcp_image": true,
            }),
        );
        let too_large_error = too_large_output["error"].as_str().unwrap();
        assert!(too_large_error.contains("MCP image too large"));
        assert!(too_large_error.contains(&MAX_MCP_IMAGE_BYTES.to_string()));

        let missing_path = "docs/images/missing.png";
        let missing_output = run_artifact_request(
            tmp.path(),
            "file_read_project_artifact",
            missing_path,
            json!({
                "path": missing_path,
                "mcp_image": true,
            }),
        );
        assert!(missing_output["error"]
            .as_str()
            .unwrap()
            .contains("stat failed"));
    }

    #[test]
    fn artifact_upload_abort_reports_cleanup_and_no_final_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = "artifacts/smoke/abort.artifact";
        let upload_id = "wc_upload_test_abort";
        let resolved = tmp.path().join(path);
        let parent = resolved.parent().unwrap();
        std::fs::create_dir_all(parent).unwrap();
        let (part, sidecar) = upload_paths(parent, upload_id);
        std::fs::write(&part, b"partial").unwrap();
        write_upload_state(
            &sidecar,
            &ArtifactUploadState {
                path: path.to_string(),
                expected_bytes: None,
                expected_sha256: None,
                mime_type: Some("application/octet-stream".to_string()),
                overwrite: false,
                max_bytes: DEFAULT_MAX_ARTIFACT_BYTES,
            },
        )
        .unwrap();

        let output = run_artifact_request(
            tmp.path(),
            "file_artifact_upload_abort",
            path,
            json!({"path": path, "upload_id": upload_id}),
        );

        assert_eq!(output["aborted"], true);
        assert_eq!(output["temp_file_removed"], true);
        assert_eq!(output["sidecar_removed"], true);
        assert_eq!(output["final_file_touched"], false);
        assert_eq!(output["final_file_exists"], false);
        assert_eq!(
            output["changed_path_details"][0]["status"],
            "upload_aborted_no_final_file"
        );
        assert!(!part.exists());
        assert!(!sidecar.exists());
        assert!(!resolved.exists());
    }

    #[test]
    fn artifact_upload_abort_preserves_preexisting_final_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = "artifacts/smoke/preexisting.artifact";
        let upload_id = "wc_upload_test_preexisting";
        let resolved = tmp.path().join(path);
        let parent = resolved.parent().unwrap();
        std::fs::create_dir_all(parent).unwrap();
        std::fs::write(&resolved, b"keep").unwrap();
        let (part, sidecar) = upload_paths(parent, upload_id);
        std::fs::write(&part, b"partial").unwrap();
        write_upload_state(
            &sidecar,
            &ArtifactUploadState {
                path: path.to_string(),
                expected_bytes: None,
                expected_sha256: None,
                mime_type: None,
                overwrite: true,
                max_bytes: DEFAULT_MAX_ARTIFACT_BYTES,
            },
        )
        .unwrap();

        let output = run_artifact_request(
            tmp.path(),
            "file_artifact_upload_abort",
            path,
            json!({"path": path, "upload_id": upload_id}),
        );

        assert_eq!(output["final_file_touched"], false);
        assert_eq!(output["final_file_exists"], true);
        assert_eq!(
            output["changed_path_details"][0]["status"],
            "upload_aborted_final_file_preexisting"
        );
        assert_eq!(std::fs::read(&resolved).unwrap(), b"keep");
    }

    #[test]
    fn artifact_upload_begin_generic_binary_accepts_arbitrary_regular_extension() {
        for path in [
            "artifacts/smoke/raw.bin",
            "artifacts/smoke/raw.artifact",
            "artifacts/smoke/data.customblob",
            "artifacts/smoke/audio.mp3",
            "artifacts/smoke/video.mp4",
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let output = run_artifact_request(
                tmp.path(),
                "file_artifact_upload_begin",
                path,
                json!({
                    "path": path,
                    "mime_type": "application/octet-stream",
                    "max_bytes": DEFAULT_MAX_ARTIFACT_BYTES,
                }),
            );

            assert!(output["error"].is_null() || output.get("error").is_none());
            assert_eq!(output["path"], path);
            assert_eq!(output["committed"], false);
            assert!(output["upload_id"]
                .as_str()
                .unwrap()
                .starts_with("wc_upload_"));
        }
    }

    #[test]
    fn common_extensions_use_shared_export_mime_policy() {
        assert_eq!(
            crate::artifact_policy::preferred_mime_for_path("artifacts/audio.mp3"),
            Some("audio/mpeg")
        );
        assert_eq!(
            crate::artifact_policy::preferred_mime_for_path("artifacts/video.mp4"),
            Some("video/mp4")
        );
        assert_eq!(
            crate::artifact_policy::preferred_mime_for_path("README.md"),
            Some("text/markdown")
        );
        assert_eq!(
            crate::artifact_policy::preferred_mime_for_path("data.customblob"),
            None
        );
    }
}
