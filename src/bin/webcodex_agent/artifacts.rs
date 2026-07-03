use super::files::sha256_hex_bytes;
use super::output::{line_edit_stdout, CommandResult};
use crate::shell_protocol::ShellAgentShellRequest;
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_MAX_ARTIFACT_BYTES: usize = 10 * 1024 * 1024;
const DEFAULT_ARTIFACT_READ_LENGTH: usize = 32 * 1024;
const DEFAULT_MAX_ARTIFACT_UPLOAD_CHUNK_BYTES: usize = 64 * 1024;

pub(crate) fn is_artifact_request_kind(kind: &str) -> bool {
    matches!(
        kind,
        "file_save_project_artifact"
            | "file_read_project_artifact_metadata"
            | "file_read_project_artifact"
            | "file_artifact_upload_begin"
            | "file_artifact_upload_chunk"
            | "file_artifact_upload_finish"
            | "file_artifact_upload_abort"
    )
}

pub(crate) fn validate_artifact_agent_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if path.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Err("path must be project-relative".to_string());
    }
    if p.components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("path cannot contain parent traversal".to_string());
    }
    if is_sensitive_artifact_path(path) {
        return Err(format!("refusing sensitive artifact path '{}'", path));
    }
    Ok(())
}

fn is_sensitive_artifact_path(path: &str) -> bool {
    for comp in path.to_lowercase().split('/') {
        if matches!(
            comp,
            ".git" | "target" | "node_modules" | "secrets" | "tokens"
        ) {
            return true;
        }
        if comp == ".env" || comp.starts_with(".env") || comp.ends_with(".pem") {
            return true;
        }
    }
    false
}

fn parse_json_payload(request: &ShellAgentShellRequest) -> Result<Value, String> {
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

fn is_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn validate_upload_id(upload_id: &str) -> Result<(), String> {
    if !upload_id.starts_with("wc_upload_") {
        return Err("upload_id must start with wc_upload_".to_string());
    }
    if upload_id.len() > 96 {
        return Err("upload_id too long".to_string());
    }
    if !upload_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("upload_id contains unsupported characters".to_string());
    }
    Ok(())
}

fn project_root(request: &ShellAgentShellRequest) -> Result<std::path::PathBuf, String> {
    let Some(cwd) = request.cwd.as_deref() else {
        return Err("artifact request missing project root".to_string());
    };
    std::fs::canonicalize(cwd).map_err(|e| format!("project root does not exist: {}", e))
}

fn ensure_existing_target_in_project_root(resolved: &Path, root: &Path) -> Result<(), String> {
    let target = std::fs::canonicalize(resolved).map_err(|e| format!("read failed: {}", e))?;
    if target != root && !target.starts_with(root) {
        return Err("artifact path escapes project root".to_string());
    }
    Ok(())
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

fn write_bytes_atomic_strict(path: &Path, data: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "target path has no parent directory".to_string())?;
    let mut last_error = None;
    for attempt in 0..16 {
        let tmp = parent.join(format!(".pd-artifact-{}-{}", std::process::id(), attempt));
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
                if let Err(e) = std::fs::rename(&tmp, path) {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(e.to_string());
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

#[derive(Debug, Deserialize, Serialize)]
struct ArtifactUploadState {
    path: String,
    expected_bytes: Option<usize>,
    expected_sha256: Option<String>,
    mime_type: Option<String>,
    overwrite: bool,
    max_bytes: usize,
}

fn upload_paths(parent: &Path, upload_id: &str) -> (PathBuf, PathBuf) {
    (
        parent.join(format!(".pd-upload-{upload_id}.part")),
        parent.join(format!(".pd-upload-{upload_id}.json")),
    )
}

fn new_upload_id(attempt: usize) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("wc_upload_{}_{}_{}", std::process::id(), nanos, attempt)
}

fn write_upload_state(sidecar: &Path, state: &ArtifactUploadState) -> Result<(), String> {
    let data = serde_json::to_vec(state).map_err(|e| format!("upload state failed: {e}"))?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(sidecar)
        .map_err(|e| format!("upload state failed: {e}"))?;
    file.write_all(&data)
        .map_err(|e| format!("upload state failed: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("upload state failed: {e}"))?;
    Ok(())
}

fn read_upload_state(sidecar: &Path, requested_path: &str) -> Result<ArtifactUploadState, String> {
    let data = std::fs::read(sidecar).map_err(|e| format!("upload not found: {e}"))?;
    let state: ArtifactUploadState =
        serde_json::from_slice(&data).map_err(|e| format!("invalid upload state: {e}"))?;
    if state.path != requested_path {
        return Err("upload_id does not belong to requested path".to_string());
    }
    Ok(state)
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

fn upload_error(path: Option<&str>, upload_id: Option<&str>, msg: impl Into<String>) -> Value {
    json!({
        "path": path,
        "upload_id": upload_id,
        "received_bytes": 0,
        "expected_bytes": Value::Null,
        "expected_sha256": Value::Null,
        "sha256": Value::Null,
        "mime_type": Value::Null,
        "committed": false,
        "aborted": false,
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

fn magic_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(b"\xff\xd8") {
        Some("image/jpeg")
    } else if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else if data.starts_with(b"%PDF-") {
        Some("application/pdf")
    } else if data.starts_with(b"PK\x03\x04") || data.starts_with(b"PK\x05\x06") {
        Some("application/zip")
    } else {
        None
    }
}

fn extension_mime(path: &str) -> Option<&'static str> {
    let lower = path.to_lowercase();
    if lower.ends_with(".png") {
        Some("image/png")
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("image/jpeg")
    } else if lower.ends_with(".webp") {
        Some("image/webp")
    } else if lower.ends_with(".pdf") {
        Some("application/pdf")
    } else if lower.ends_with(".zip") {
        Some("application/zip")
    } else if lower.ends_with(".txt") {
        Some("text/plain")
    } else if lower.ends_with(".csv") {
        Some("text/csv")
    } else if lower.ends_with(".json") {
        Some("application/json")
    } else {
        None
    }
}

fn artifact_mime(path: &str, data: &[u8], sniff_json: bool) -> Option<String> {
    let mut mime = extension_mime(path);
    if let Some(magic) = magic_mime(data) {
        mime = Some(magic);
    } else if sniff_json {
        let first = data.iter().copied().find(|b| !b.is_ascii_whitespace());
        if matches!(first, Some(b'{') | Some(b'[')) {
            mime = Some("application/json");
        }
    }
    mime.map(str::to_string)
}

fn png_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() >= 24 && data.starts_with(b"\x89PNG\r\n\x1a\n") {
        let width = u32::from_be_bytes(data[16..20].try_into().ok()?);
        let height = u32::from_be_bytes(data[20..24].try_into().ok()?);
        Some((width, height))
    } else {
        None
    }
}

fn webp_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() >= 30
        && data.starts_with(b"RIFF")
        && &data[8..12] == b"WEBP"
        && &data[12..16] == b"VP8X"
    {
        let width =
            1 + u32::from(data[24]) + (u32::from(data[25]) << 8) + (u32::from(data[26]) << 16);
        let height =
            1 + u32::from(data[27]) + (u32::from(data[28]) << 8) + (u32::from(data[29]) << 16);
        Some((width, height))
    } else {
        None
    }
}

fn jpeg_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 4 || !data.starts_with(b"\xff\xd8") {
        return None;
    }
    let mut i = 2;
    while i + 9 < data.len() {
        if data[i] != 0xff {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        i += 2;
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            let height = u16::from_be_bytes(data[i + 3..i + 5].try_into().ok()?);
            let width = u16::from_be_bytes(data[i + 5..i + 7].try_into().ok()?);
            return Some((u32::from(width), u32::from(height)));
        }
        if i + 2 > data.len() {
            break;
        }
        let segment_len = usize::from(u16::from_be_bytes(data[i..i + 2].try_into().ok()?));
        if segment_len < 2 {
            break;
        }
        i = i.saturating_add(segment_len);
    }
    None
}

fn image_size(data: &[u8]) -> Option<(u32, u32)> {
    png_size(data)
        .or_else(|| jpeg_size(data))
        .or_else(|| webp_size(data))
}

fn zip_entry_count(data: &[u8]) -> Option<u16> {
    let min_eocd_len = 22;
    if data.len() < min_eocd_len {
        return None;
    }
    let search_start = data.len().saturating_sub(65_557);
    for i in (search_start..=data.len() - min_eocd_len).rev() {
        if &data[i..i + 4] == b"PK\x05\x06" {
            return Some(u16::from_le_bytes(data[i + 10..i + 12].try_into().ok()?));
        }
    }
    None
}

fn read_limited(path: &Path, max_bytes: usize) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("read failed: {}", e))?;
    let mut limited = file.take(max_bytes.saturating_add(1) as u64);
    let mut data = Vec::new();
    limited
        .read_to_end(&mut data)
        .map_err(|e| format!("read failed: {}", e))?;
    if data.len() > max_bytes {
        return Err("artifact too large to inspect".to_string());
    }
    Ok(data)
}

pub(crate) fn handle_artifact_file_request(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    match request.kind.as_str() {
        "file_save_project_artifact" => handle_save_project_artifact(request, resolved, start),
        "file_read_project_artifact_metadata" => {
            handle_read_project_artifact_metadata(request, resolved, start)
        }
        "file_read_project_artifact" => handle_read_project_artifact(request, resolved, start),
        "file_artifact_upload_begin" => handle_artifact_upload_begin(request, resolved, start),
        "file_artifact_upload_chunk" => handle_artifact_upload_chunk(request, resolved, start),
        "file_artifact_upload_finish" => handle_artifact_upload_finish(request, resolved, start),
        "file_artifact_upload_abort" => handle_artifact_upload_abort(request, resolved, start),
        _ => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(format!("unknown artifact request kind: {}", request.kind)),
        },
    }
}

fn handle_save_project_artifact(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_deref().unwrap_or_default();
    let payload = match parse_json_payload(request) {
        Ok(payload) => payload,
        Err(e) => return line_edit_stdout(save_error(None, e), start),
    };
    if let Err(e) = validate_artifact_agent_path(path) {
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
    if let Err(e) = write_bytes_atomic_strict(resolved, &data) {
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

fn handle_artifact_upload_begin(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_deref().unwrap_or_default();
    let payload = match parse_json_payload(request) {
        Ok(payload) => payload,
        Err(e) => return line_edit_stdout(upload_error(None, None, e), start),
    };
    if let Err(e) = validate_artifact_agent_path(path) {
        return line_edit_stdout(upload_error(Some(path), None, e), start);
    }
    let root = match project_root(request) {
        Ok(root) => root,
        Err(e) => return line_edit_stdout(upload_error(Some(path), None, e), start),
    };
    let max_bytes = match parse_usize_field(&payload, "max_bytes", DEFAULT_MAX_ARTIFACT_BYTES) {
        Ok(value) if value > 0 => value,
        Ok(_) => {
            return line_edit_stdout(
                upload_error(Some(path), None, "max_bytes must be >= 1"),
                start,
            )
        }
        Err(e) => return line_edit_stdout(upload_error(Some(path), None, e), start),
    };
    let expected_bytes = match parse_optional_usize_field(&payload, "expected_bytes") {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(upload_error(Some(path), None, e), start),
    };
    if expected_bytes.is_some_and(|bytes| bytes > max_bytes) {
        return line_edit_stdout(
            upload_error(Some(path), None, "expected_bytes exceeds max_bytes"),
            start,
        );
    }
    let expected_sha256 = match parse_optional_clean_string(&payload, "expected_sha256", 64) {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(upload_error(Some(path), None, e), start),
    };
    if expected_sha256
        .as_deref()
        .is_some_and(|sha256| !is_hex_sha256(sha256))
    {
        return line_edit_stdout(
            upload_error(
                Some(path),
                None,
                "expected_sha256 must be a lowercase 64-char hex sha256 digest",
            ),
            start,
        );
    }
    let mime_type = match parse_optional_clean_string(&payload, "mime_type", 128) {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(upload_error(Some(path), None, e), start),
    };
    let overwrite = match parse_bool_field(&payload, "overwrite") {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(upload_error(Some(path), None, e), start),
    };

    let exists = std::fs::symlink_metadata(resolved).is_ok();
    if exists && !overwrite {
        return line_edit_stdout(
            upload_error(Some(path), None, "file exists and overwrite is false"),
            start,
        );
    }
    if exists
        && std::fs::symlink_metadata(resolved)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
    {
        return line_edit_stdout(
            upload_error(
                Some(path),
                None,
                "refusing to overwrite symlink artifact path",
            ),
            start,
        );
    }
    if let Err(e) = ensure_parent_in_project_root(resolved, &root) {
        return line_edit_stdout(upload_error(Some(path), None, e), start);
    }
    if std::fs::symlink_metadata(resolved)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return line_edit_stdout(
            upload_error(
                Some(path),
                None,
                "refusing to overwrite symlink artifact path",
            ),
            start,
        );
    }
    let parent = match resolved.parent() {
        Some(parent) => parent,
        None => {
            return line_edit_stdout(
                upload_error(Some(path), None, "target path has no parent directory"),
                start,
            )
        }
    };
    let state = ArtifactUploadState {
        path: path.to_string(),
        expected_bytes,
        expected_sha256,
        mime_type,
        overwrite,
        max_bytes,
    };
    let mut last_error = None;
    for attempt in 0..16 {
        let upload_id = new_upload_id(attempt);
        let (part, sidecar) = upload_paths(parent, &upload_id);
        if sidecar.exists() {
            continue;
        }
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&part)
        {
            Ok(file) => {
                if let Err(e) = file.sync_all() {
                    let _ = std::fs::remove_file(&part);
                    return line_edit_stdout(
                        upload_error(
                            Some(path),
                            Some(&upload_id),
                            format!("upload begin failed: {e}"),
                        ),
                        start,
                    );
                }
                drop(file);
                if let Err(e) = write_upload_state(&sidecar, &state) {
                    let _ = std::fs::remove_file(&part);
                    return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start);
                }
                if let Ok(dir) = std::fs::File::open(parent) {
                    let _ = dir.sync_all();
                }
                return line_edit_stdout(
                    json!({
                        "upload_id": upload_id,
                        "path": path,
                        "received_bytes": 0,
                        "next_offset": 0,
                        "expected_bytes": state.expected_bytes,
                        "expected_sha256": state.expected_sha256,
                        "max_bytes": state.max_bytes,
                        "mime_type": state.mime_type,
                        "overwrite": state.overwrite,
                        "committed": false,
                    }),
                    start,
                );
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                last_error = Some(e.to_string());
            }
            Err(e) => {
                return line_edit_stdout(
                    upload_error(Some(path), None, format!("upload begin failed: {e}")),
                    start,
                )
            }
        }
    }
    line_edit_stdout(
        upload_error(
            Some(path),
            None,
            last_error.unwrap_or_else(|| "could not create upload session".to_string()),
        ),
        start,
    )
}

fn handle_artifact_upload_chunk(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_deref().unwrap_or_default();
    let payload = match parse_json_payload(request) {
        Ok(payload) => payload,
        Err(e) => return line_edit_stdout(upload_error(None, None, e), start),
    };
    let upload_id = match parse_required_clean_string(&payload, "upload_id", 96) {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(upload_error(Some(path), None, e), start),
    };
    if let Err(e) = validate_upload_id(&upload_id) {
        return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start);
    }
    if let Err(e) = validate_artifact_agent_path(path) {
        return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start);
    }
    let offset = match parse_optional_usize_field(&payload, "offset") {
        Ok(Some(value)) => value,
        Ok(None) => {
            return line_edit_stdout(
                upload_error(Some(path), Some(&upload_id), "offset is required"),
                start,
            )
        }
        Err(e) => return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start),
    };
    let max_chunk_bytes = match parse_usize_field(
        &payload,
        "max_chunk_bytes",
        DEFAULT_MAX_ARTIFACT_UPLOAD_CHUNK_BYTES,
    ) {
        Ok(value) if value > 0 => value,
        Ok(_) => {
            return line_edit_stdout(
                upload_error(Some(path), Some(&upload_id), "max_chunk_bytes must be >= 1"),
                start,
            )
        }
        Err(e) => return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start),
    };
    let content_base64 = match payload.get("content_base64").and_then(Value::as_str) {
        Some(value) if !value.contains('\0') => value,
        _ => {
            return line_edit_stdout(
                upload_error(
                    Some(path),
                    Some(&upload_id),
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
                upload_error(Some(path), Some(&upload_id), format!("invalid base64: {e}")),
                start,
            )
        }
    };
    if data.is_empty() {
        return line_edit_stdout(
            upload_error(
                Some(path),
                Some(&upload_id),
                "decoded chunk must contain at least 1 byte",
            ),
            start,
        );
    }
    if data.len() > max_chunk_bytes {
        return line_edit_stdout(
            upload_error(Some(path), Some(&upload_id), "decoded chunk too large"),
            start,
        );
    }
    let root = match project_root(request) {
        Ok(root) => root,
        Err(e) => return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start),
    };
    if let Err(e) = ensure_existing_parent_in_project_root(resolved, &root) {
        return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start);
    }
    let parent = match resolved.parent() {
        Some(parent) => parent,
        None => {
            return line_edit_stdout(
                upload_error(
                    Some(path),
                    Some(&upload_id),
                    "target path has no parent directory",
                ),
                start,
            )
        }
    };
    let (part, sidecar) = upload_paths(parent, &upload_id);
    let state = match read_upload_state(&sidecar, path) {
        Ok(state) => state,
        Err(e) => return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start),
    };
    let received_bytes = match std::fs::metadata(&part) {
        Ok(metadata) => metadata.len() as usize,
        Err(e) => {
            return line_edit_stdout(
                upload_error(
                    Some(path),
                    Some(&upload_id),
                    format!("upload chunk failed: {e}"),
                ),
                start,
            )
        }
    };
    if received_bytes != offset {
        return line_edit_stdout(
            json!({
                "path": path,
                "upload_id": upload_id,
                "received_bytes": received_bytes,
                "next_offset": received_bytes,
                "expected_bytes": state.expected_bytes,
                "expected_sha256": state.expected_sha256,
                "max_bytes": state.max_bytes,
                "mime_type": state.mime_type,
                "committed": false,
                "error": "offset does not match received_bytes",
            }),
            start,
        );
    }
    let next_offset = match received_bytes.checked_add(data.len()) {
        Some(value) => value,
        None => {
            return line_edit_stdout(
                upload_error(Some(path), Some(&upload_id), "upload size overflow"),
                start,
            )
        }
    };
    if next_offset > state.max_bytes {
        return line_edit_stdout(
            upload_error(Some(path), Some(&upload_id), "upload exceeds max_bytes"),
            start,
        );
    }
    let mut file = match std::fs::OpenOptions::new().append(true).open(&part) {
        Ok(file) => file,
        Err(e) => {
            return line_edit_stdout(
                upload_error(
                    Some(path),
                    Some(&upload_id),
                    format!("upload chunk failed: {e}"),
                ),
                start,
            )
        }
    };
    if let Err(e) = file.write_all(&data) {
        return line_edit_stdout(
            upload_error(
                Some(path),
                Some(&upload_id),
                format!("upload chunk failed: {e}"),
            ),
            start,
        );
    }
    if let Err(e) = file.sync_all() {
        return line_edit_stdout(
            upload_error(
                Some(path),
                Some(&upload_id),
                format!("upload chunk failed: {e}"),
            ),
            start,
        );
    }
    line_edit_stdout(
        json!({
            "path": path,
            "upload_id": upload_id,
            "received_bytes": next_offset,
            "next_offset": next_offset,
            "expected_bytes": state.expected_bytes,
            "expected_sha256": state.expected_sha256,
            "max_bytes": state.max_bytes,
            "mime_type": state.mime_type,
            "committed": false,
        }),
        start,
    )
}

fn handle_artifact_upload_finish(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_deref().unwrap_or_default();
    let payload = match parse_json_payload(request) {
        Ok(payload) => payload,
        Err(e) => return line_edit_stdout(upload_error(None, None, e), start),
    };
    let upload_id = match parse_required_clean_string(&payload, "upload_id", 96) {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(upload_error(Some(path), None, e), start),
    };
    if let Err(e) = validate_upload_id(&upload_id) {
        return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start);
    }
    if let Err(e) = validate_artifact_agent_path(path) {
        return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start);
    }
    let root = match project_root(request) {
        Ok(root) => root,
        Err(e) => return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start),
    };
    if let Err(e) = ensure_existing_parent_in_project_root(resolved, &root) {
        return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start);
    }
    let parent = match resolved.parent() {
        Some(parent) => parent,
        None => {
            return line_edit_stdout(
                upload_error(
                    Some(path),
                    Some(&upload_id),
                    "target path has no parent directory",
                ),
                start,
            )
        }
    };
    let (part, sidecar) = upload_paths(parent, &upload_id);
    let state = match read_upload_state(&sidecar, path) {
        Ok(state) => state,
        Err(e) => return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start),
    };
    let data = match read_limited(&part, state.max_bytes) {
        Ok(data) => data,
        Err(e) => return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start),
    };
    let bytes = data.len();
    if state
        .expected_bytes
        .is_some_and(|expected| expected != bytes)
    {
        return line_edit_stdout(
            json!({
                "path": path,
                "upload_id": upload_id,
                "received_bytes": bytes,
                "expected_bytes": state.expected_bytes,
                "expected_sha256": state.expected_sha256,
                "sha256": sha256_hex_bytes(&data),
                "mime_type": state.mime_type,
                "committed": false,
                "error": "uploaded byte count does not match expected_bytes",
            }),
            start,
        );
    }
    let sha256 = sha256_hex_bytes(&data);
    if state
        .expected_sha256
        .as_deref()
        .is_some_and(|expected| expected != sha256)
    {
        return line_edit_stdout(
            json!({
                "path": path,
                "upload_id": upload_id,
                "received_bytes": bytes,
                "expected_bytes": state.expected_bytes,
                "expected_sha256": state.expected_sha256,
                "sha256": sha256,
                "mime_type": state.mime_type,
                "committed": false,
                "error": "uploaded sha256 does not match expected_sha256",
            }),
            start,
        );
    }
    let exists = std::fs::symlink_metadata(resolved).is_ok();
    if exists && !state.overwrite {
        return line_edit_stdout(
            upload_error(
                Some(path),
                Some(&upload_id),
                "file exists and overwrite is false",
            ),
            start,
        );
    }
    if exists
        && std::fs::symlink_metadata(resolved)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
    {
        return line_edit_stdout(
            upload_error(
                Some(path),
                Some(&upload_id),
                "refusing to overwrite symlink artifact path",
            ),
            start,
        );
    }
    if let Err(e) = std::fs::rename(&part, resolved) {
        return line_edit_stdout(
            upload_error(
                Some(path),
                Some(&upload_id),
                format!("upload finish failed: {e}"),
            ),
            start,
        );
    }
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    let _ = std::fs::remove_file(&sidecar);
    line_edit_stdout(
        json!({
            "path": path,
            "upload_id": upload_id,
            "bytes": bytes,
            "received_bytes": bytes,
            "expected_bytes": state.expected_bytes,
            "expected_sha256": state.expected_sha256,
            "sha256": sha256,
            "mime_type": state.mime_type.or_else(|| artifact_mime(path, &data, true)),
            "committed": true,
        }),
        start,
    )
}

fn handle_artifact_upload_abort(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_deref().unwrap_or_default();
    let payload = match parse_json_payload(request) {
        Ok(payload) => payload,
        Err(e) => return line_edit_stdout(upload_error(None, None, e), start),
    };
    let upload_id = match parse_required_clean_string(&payload, "upload_id", 96) {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(upload_error(Some(path), None, e), start),
    };
    if let Err(e) = validate_upload_id(&upload_id) {
        return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start);
    }
    if let Err(e) = validate_artifact_agent_path(path) {
        return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start);
    }
    let root = match project_root(request) {
        Ok(root) => root,
        Err(e) => return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start),
    };
    if let Err(e) = ensure_existing_parent_in_project_root(resolved, &root) {
        return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start);
    }
    let parent = match resolved.parent() {
        Some(parent) => parent,
        None => {
            return line_edit_stdout(
                upload_error(
                    Some(path),
                    Some(&upload_id),
                    "target path has no parent directory",
                ),
                start,
            )
        }
    };
    let (part, sidecar) = upload_paths(parent, &upload_id);
    let state = match read_upload_state(&sidecar, path) {
        Ok(state) => state,
        Err(e) => return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start),
    };
    let received_bytes = std::fs::metadata(&part)
        .map(|metadata| metadata.len() as usize)
        .unwrap_or(0);
    let _ = std::fs::remove_file(&part);
    let _ = std::fs::remove_file(&sidecar);
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    line_edit_stdout(
        json!({
            "path": path,
            "upload_id": upload_id,
            "received_bytes": received_bytes,
            "expected_bytes": state.expected_bytes,
            "expected_sha256": state.expected_sha256,
            "mime_type": state.mime_type,
            "committed": false,
            "aborted": true,
        }),
        start,
    )
}

fn handle_read_project_artifact_metadata(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_deref().unwrap_or_default();
    let payload = match parse_json_payload(request) {
        Ok(payload) => payload,
        Err(e) => return line_edit_stdout(metadata_error(None, e), start),
    };
    if let Err(e) = validate_artifact_agent_path(path) {
        return line_edit_stdout(metadata_error(Some(path), e), start);
    }
    let root = match project_root(request) {
        Ok(root) => root,
        Err(e) => return line_edit_stdout(metadata_error(Some(path), e), start),
    };
    if let Err(e) = ensure_existing_target_in_project_root(resolved, &root) {
        return line_edit_stdout(metadata_error(Some(path), e), start);
    }
    let max_bytes = match parse_usize_field(&payload, "max_bytes", DEFAULT_MAX_ARTIFACT_BYTES) {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(metadata_error(Some(path), e), start),
    };
    let data = match read_limited(resolved, max_bytes) {
        Ok(data) => data,
        Err(e) => return line_edit_stdout(metadata_error(Some(path), e), start),
    };
    let mime_type = artifact_mime(path, &data, false);
    let mut out = json!({
        "path": path,
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

fn handle_read_project_artifact(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_deref().unwrap_or_default();
    let payload = match parse_json_payload(request) {
        Ok(payload) => payload,
        Err(e) => return line_edit_stdout(read_error(None, e), start),
    };
    if let Err(e) = validate_artifact_agent_path(path) {
        return line_edit_stdout(read_error(Some(path), e), start);
    }
    let root = match project_root(request) {
        Ok(root) => root,
        Err(e) => return line_edit_stdout(read_error(Some(path), e), start),
    };
    if let Err(e) = ensure_existing_target_in_project_root(resolved, &root) {
        let msg = e.replacen("read failed", "stat failed", 1);
        return line_edit_stdout(read_error(Some(path), msg), start);
    }
    let offset = match parse_usize_field(&payload, "offset", 0) {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(read_error(Some(path), e), start),
    };
    let length = match parse_usize_field(&payload, "length", DEFAULT_ARTIFACT_READ_LENGTH) {
        Ok(value) => value,
        Err(e) => return line_edit_stdout(read_error(Some(path), e), start),
    };
    if length < 1 {
        return line_edit_stdout(read_error(Some(path), "length must be >= 1"), start);
    }
    let max_file_bytes =
        match parse_usize_field(&payload, "max_file_bytes", DEFAULT_MAX_ARTIFACT_BYTES) {
            Ok(value) => value,
            Err(e) => return line_edit_stdout(read_error(Some(path), e), start),
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
        return line_edit_stdout(
            read_error(
                Some(path),
                "artifact too large to read; use metadata or a smaller artifact",
            ),
            start,
        );
    }
    let data = match std::fs::read(resolved) {
        Ok(data) => data,
        Err(e) => {
            return line_edit_stdout(read_error(Some(path), format!("read failed: {e}")), start)
        }
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
            "mime_type": artifact_mime(path, &data, true),
            "file_bytes": file_bytes,
            "sha256": sha256_hex_bytes(&data),
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
