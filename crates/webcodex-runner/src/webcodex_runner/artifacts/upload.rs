use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use webcodex_core::runner_operation::RunnerFilePayload;

use super::super::output::{line_edit_stdout, CommandResult};
use super::inspection::{artifact_mime_from_file, verify_upload_file};
use super::{
    ensure_existing_parent_in_project_root, ensure_parent_in_project_root, parse_bool_field,
    parse_json_payload, parse_optional_clean_string, parse_optional_usize_field,
    parse_required_clean_string, parse_usize_field, project_root, validate_artifact_runner_path,
};
use crate::apply_edits_shared::is_lowercase_hex_sha256 as is_hex_sha256;
use crate::artifact_policy::ooxml_extension_for_mime;

pub(super) const MAX_ARTIFACT_UPLOAD_BYTES: usize = 256 * 1024 * 1024;
pub(super) const MAX_ARTIFACT_UPLOAD_CHUNK_BYTES: usize = 1024 * 1024;
const ARTIFACT_UPLOAD_IDLE_TTL_SECS: u64 = 24 * 60 * 60;
pub(super) const MAX_ACTIVE_ARTIFACT_UPLOADS_PER_PROJECT: usize = 32;
pub(super) const MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT: usize = 512 * 1024 * 1024;
pub(super) const MAX_ARTIFACT_UPLOAD_STATE_BYTES: usize = 4 * 1024;

pub(super) fn commit_artifact_upload_part(
    part: &Path,
    target: &Path,
    overwrite: bool,
) -> Result<(), String> {
    if overwrite {
        return std::fs::rename(part, target).map_err(|e| format!("upload finish failed: {e}"));
    }
    match std::fs::hard_link(part, target) {
        Ok(()) => {
            // The final target now exists without replacing any concurrent writer.
            // If private-part cleanup fails, the target remains authoritative and a
            // later upload sweep can remove the orphaned private link safely.
            let _ = std::fs::remove_file(part);
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Err("file exists and overwrite is false".to_string())
        }
        Err(e) => Err(format!("upload finish failed: {e}")),
    }
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

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct ArtifactUploadState {
    pub(super) path: String,
    pub(super) expected_bytes: Option<usize>,
    pub(super) expected_sha256: Option<String>,
    pub(super) mime_type: Option<String>,
    pub(super) overwrite: bool,
    pub(super) max_bytes: usize,
}

pub(super) fn upload_paths(parent: &Path, upload_id: &str) -> (PathBuf, PathBuf) {
    (
        parent.join(format!(".wc-upload-{upload_id}.part")),
        parent.join(format!(".wc-upload-{upload_id}.json")),
    )
}

fn new_upload_id(attempt: usize) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("wc_upload_{}_{}_{}", std::process::id(), nanos, attempt)
}

pub(super) fn write_upload_state(
    sidecar: &Path,
    state: &ArtifactUploadState,
) -> Result<(), String> {
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

pub(super) fn read_upload_state_file(sidecar: &Path) -> Result<ArtifactUploadState, String> {
    let file = File::open(sidecar).map_err(|e| format!("upload not found: {e}"))?;
    let mut reader = file.take((MAX_ARTIFACT_UPLOAD_STATE_BYTES + 1) as u64);
    let mut data = Vec::new();
    reader
        .read_to_end(&mut data)
        .map_err(|e| format!("invalid upload state: {e}"))?;
    if data.len() > MAX_ARTIFACT_UPLOAD_STATE_BYTES {
        return Err("invalid upload state: sidecar too large".to_string());
    }
    serde_json::from_slice(&data).map_err(|e| format!("invalid upload state: {e}"))
}

pub(super) fn read_upload_state(
    sidecar: &Path,
    requested_path: &str,
) -> Result<ArtifactUploadState, String> {
    let state = read_upload_state_file(sidecar)?;
    if state.path != requested_path {
        return Err("upload_id does not belong to requested path".to_string());
    }
    Ok(state)
}

#[derive(Default)]
struct ArtifactUploadTempFiles {
    part: Option<PathBuf>,
    sidecar: Option<PathBuf>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ArtifactUploadProjectUsage {
    pub(super) active_uploads: usize,
    pub(super) reserved_bytes: usize,
}

fn upload_temp_id(name: &str, suffix: &str) -> Option<String> {
    let upload_id = name.strip_prefix(".wc-upload-")?.strip_suffix(suffix)?;
    validate_upload_id(upload_id).ok()?;
    Some(upload_id.to_string())
}

fn remove_upload_temp_file(path: &Path) -> Result<bool, String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("upload cleanup failed: {e}")),
    }
}

fn upload_pair_is_stale(
    part_metadata: &std::fs::Metadata,
    sidecar_metadata: &std::fs::Metadata,
    now: SystemTime,
    idle_ttl: Duration,
) -> bool {
    let newest_modified = match (part_metadata.modified(), sidecar_metadata.modified()) {
        (Ok(part), Ok(sidecar)) => Some(if part >= sidecar { part } else { sidecar }),
        (Ok(part), Err(_)) => Some(part),
        (Err(_), Ok(sidecar)) => Some(sidecar),
        (Err(_), Err(_)) => None,
    };
    newest_modified
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|age| age >= idle_ttl)
}

fn upload_scan_ignores_entry_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::NotFound | ErrorKind::PermissionDenied
    )
}

pub(super) fn sweep_artifact_upload_directory(
    directory: &Path,
    now: SystemTime,
    idle_ttl: Duration,
) -> Result<Vec<(String, usize)>, String> {
    let entries =
        std::fs::read_dir(directory).map_err(|error| format!("upload cleanup failed: {error}"))?;
    let mut uploads: BTreeMap<String, ArtifactUploadTempFiles> = BTreeMap::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if upload_scan_ignores_entry_error(&error) => continue,
            Err(error) => return Err(format!("upload cleanup failed: {error}")),
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if let Some(upload_id) = upload_temp_id(&name, ".part") {
            uploads.entry(upload_id).or_default().part = Some(entry.path());
            continue;
        }
        if let Some(upload_id) = upload_temp_id(&name, ".json") {
            uploads.entry(upload_id).or_default().sidecar = Some(entry.path());
        }
    }

    let mut active = Vec::new();
    let mut cleaned = false;
    for (upload_id, files) in uploads {
        match (files.part, files.sidecar) {
            (Some(part), Some(sidecar)) => {
                let part_metadata = match std::fs::metadata(&part) {
                    Ok(metadata) => metadata,
                    Err(e) if e.kind() == ErrorKind::NotFound => {
                        cleaned |= remove_upload_temp_file(&sidecar)?;
                        continue;
                    }
                    Err(e) => return Err(format!("upload cleanup failed: {e}")),
                };
                let sidecar_metadata = match std::fs::metadata(&sidecar) {
                    Ok(metadata) => metadata,
                    Err(e) if e.kind() == ErrorKind::NotFound => {
                        cleaned |= remove_upload_temp_file(&part)?;
                        continue;
                    }
                    Err(e) => return Err(format!("upload cleanup failed: {e}")),
                };
                if upload_pair_is_stale(&part_metadata, &sidecar_metadata, now, idle_ttl) {
                    cleaned |= remove_upload_temp_file(&part)?;
                    cleaned |= remove_upload_temp_file(&sidecar)?;
                    continue;
                }
                let state = match read_upload_state_file(&sidecar) {
                    Ok(state) => state,
                    Err(_) => {
                        cleaned |= remove_upload_temp_file(&part)?;
                        cleaned |= remove_upload_temp_file(&sidecar)?;
                        continue;
                    }
                };
                active.push((upload_id, state.max_bytes));
            }
            (Some(part), None) => cleaned |= remove_upload_temp_file(&part)?,
            (None, Some(sidecar)) => cleaned |= remove_upload_temp_file(&sidecar)?,
            (None, None) => {}
        }
    }
    if cleaned {
        if let Ok(dir) = std::fs::File::open(directory) {
            let _ = dir.sync_all();
        }
    }
    Ok(active)
}

// Keep quota accounting proportional to active uploads instead of recursively scanning the
// entire Project on every begin. Each target directory is swept at most once per Runner process
// to adopt non-stale persisted upload pairs after restart; chunk/finish/abort also adopt a
// resumed upload before mutating it.
#[derive(Debug, Default)]
pub(super) struct ArtifactUploadRuntimeState {
    active_by_project: BTreeMap<PathBuf, BTreeMap<String, usize>>,
    scanned_directories: BTreeSet<(PathBuf, PathBuf)>,
}

impl ArtifactUploadRuntimeState {
    pub(super) fn new() -> Self {
        Self::default()
    }

    fn project_usage(&self, root: &Path) -> Result<ArtifactUploadProjectUsage, String> {
        let mut usage = ArtifactUploadProjectUsage::default();
        let Some(active) = self.active_by_project.get(root) else {
            return Ok(usage);
        };
        usage.active_uploads = active.len();
        for max_bytes in active.values() {
            usage.reserved_bytes = usage
                .reserved_bytes
                .checked_add(*max_bytes)
                .ok_or_else(|| "artifact upload reserved byte count overflow".to_string())?;
        }
        Ok(usage)
    }

    fn track_existing(&mut self, root: &Path, upload_id: &str, max_bytes: usize) {
        self.active_by_project
            .entry(root.to_path_buf())
            .or_default()
            .entry(upload_id.to_string())
            .or_insert(max_bytes);
    }

    fn release(&mut self, root: &Path, upload_id: &str) {
        let remove_project = self
            .active_by_project
            .get_mut(root)
            .map(|active| {
                active.remove(upload_id);
                active.is_empty()
            })
            .unwrap_or(false);
        if remove_project {
            self.active_by_project.remove(root);
        }
    }

    fn prepare_directory(&mut self, root: &Path, directory: &Path) -> Result<(), String> {
        let key = (root.to_path_buf(), directory.to_path_buf());
        if self.scanned_directories.contains(&key) {
            return Ok(());
        }
        let active = sweep_artifact_upload_directory(
            directory,
            SystemTime::now(),
            Duration::from_secs(ARTIFACT_UPLOAD_IDLE_TTL_SECS),
        )?;
        for (upload_id, max_bytes) in active {
            self.track_existing(root, &upload_id, max_bytes);
        }
        self.scanned_directories.insert(key);
        Ok(())
    }

    fn check_begin_admission(&self, root: &Path, max_bytes: usize) -> Result<(), String> {
        let usage = self.project_usage(root)?;
        enforce_artifact_upload_begin_admission(&usage, max_bytes)
    }
}

pub(super) fn enforce_artifact_upload_begin_admission(
    usage: &ArtifactUploadProjectUsage,
    requested_max_bytes: usize,
) -> Result<(), String> {
    if usage.active_uploads >= MAX_ACTIVE_ARTIFACT_UPLOADS_PER_PROJECT {
        return Err(format!(
            "artifact upload project reached active upload limit ({MAX_ACTIVE_ARTIFACT_UPLOADS_PER_PROJECT})"
        ));
    }
    let next_reserved_bytes = usage
        .reserved_bytes
        .checked_add(requested_max_bytes)
        .ok_or_else(|| "artifact upload reserved byte count overflow".to_string())?;
    if next_reserved_bytes > MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT {
        return Err(format!(
            "artifact upload project reserved byte quota exceeded ({MAX_ARTIFACT_UPLOAD_RESERVED_BYTES_PER_PROJECT})"
        ));
    }
    Ok(())
}

pub(super) fn upload_error(
    path: Option<&str>,
    upload_id: Option<&str>,
    msg: impl Into<String>,
) -> Value {
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

pub(super) fn handle_artifact_upload_begin(
    request: &RunnerFilePayload,
    resolved: &Path,
    start: Instant,
    runtime: &mut ArtifactUploadRuntimeState,
) -> CommandResult {
    let path = request.path.as_str();
    let payload = match parse_json_payload(request) {
        Ok(payload) => payload,
        Err(e) => return line_edit_stdout(upload_error(None, None, e), start),
    };
    if let Err(e) = validate_artifact_runner_path(path) {
        return line_edit_stdout(upload_error(Some(path), None, e), start);
    }
    let root = match project_root(request) {
        Ok(root) => root,
        Err(e) => return line_edit_stdout(upload_error(Some(path), None, e), start),
    };
    let max_bytes = match parse_usize_field(&payload, "max_bytes", MAX_ARTIFACT_UPLOAD_BYTES) {
        Ok(value) if value > 0 => value,
        Ok(_) => {
            return line_edit_stdout(
                upload_error(Some(path), None, "max_bytes must be >= 1"),
                start,
            )
        }
        Err(e) => return line_edit_stdout(upload_error(Some(path), None, e), start),
    };
    if max_bytes > MAX_ARTIFACT_UPLOAD_BYTES {
        return line_edit_stdout(
            upload_error(
                Some(path),
                None,
                format!("max_bytes exceeds upload maximum ({MAX_ARTIFACT_UPLOAD_BYTES})"),
            ),
            start,
        );
    }
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
    if let Err(e) = runtime.prepare_directory(&root, parent) {
        return line_edit_stdout(upload_error(Some(path), None, e), start);
    }
    if let Err(e) = runtime.check_begin_admission(&root, max_bytes) {
        return line_edit_stdout(upload_error(Some(path), None, e), start);
    }
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
                runtime.track_existing(&root, &upload_id, state.max_bytes);
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

pub(super) fn handle_artifact_upload_chunk(
    request: &RunnerFilePayload,
    resolved: &Path,
    start: Instant,
    runtime: &mut ArtifactUploadRuntimeState,
) -> CommandResult {
    let path = request.path.as_str();
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
    if let Err(e) = validate_artifact_runner_path(path) {
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
    let max_chunk_bytes =
        match parse_usize_field(&payload, "max_chunk_bytes", MAX_ARTIFACT_UPLOAD_CHUNK_BYTES) {
            Ok(value) if value > 0 => value,
            Ok(_) => {
                return line_edit_stdout(
                    upload_error(Some(path), Some(&upload_id), "max_chunk_bytes must be >= 1"),
                    start,
                )
            }
            Err(e) => {
                return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start)
            }
        };
    if max_chunk_bytes > MAX_ARTIFACT_UPLOAD_CHUNK_BYTES {
        return line_edit_stdout(
            upload_error(
                Some(path),
                Some(&upload_id),
                format!(
                    "max_chunk_bytes exceeds upload chunk maximum ({MAX_ARTIFACT_UPLOAD_CHUNK_BYTES})"
                ),
            ),
            start,
        );
    }
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
    if state.max_bytes > MAX_ARTIFACT_UPLOAD_BYTES {
        return line_edit_stdout(
            upload_error(
                Some(path),
                Some(&upload_id),
                "upload max_bytes exceeds per-file upload maximum",
            ),
            start,
        );
    }
    runtime.track_existing(&root, &upload_id, state.max_bytes);
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

pub(super) fn handle_artifact_upload_finish(
    request: &RunnerFilePayload,
    resolved: &Path,
    start: Instant,
    runtime: &mut ArtifactUploadRuntimeState,
) -> CommandResult {
    let path = request.path.as_str();
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
    if let Err(e) = validate_artifact_runner_path(path) {
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
    runtime.track_existing(&root, &upload_id, state.max_bytes);
    let (bytes, sha256) = match verify_upload_file(&part, state.max_bytes) {
        Ok(verification) => verification,
        Err(e) => return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start),
    };
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
                "sha256": sha256,
                "mime_type": state.mime_type,
                "committed": false,
                "error": "uploaded byte count does not match expected_bytes",
            }),
            start,
        );
    }
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
    let detected_mime = artifact_mime_from_file(path, &part, true);
    if let Some(claimed_ooxml_mime) = state
        .mime_type
        .as_deref()
        .filter(|mime| ooxml_extension_for_mime(mime).is_some())
    {
        if detected_mime.as_deref() != Some(claimed_ooxml_mime) {
            return line_edit_stdout(
                upload_error(
                    Some(path),
                    Some(&upload_id),
                    "OOXML MIME type does not match uploaded package content",
                ),
                start,
            );
        }
    }
    let presentation_mime = detected_mime.or_else(|| state.mime_type.clone());
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
    if let Err(e) = commit_artifact_upload_part(&part, resolved, state.overwrite) {
        return line_edit_stdout(upload_error(Some(path), Some(&upload_id), e), start);
    }
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    let _ = std::fs::remove_file(&sidecar);
    runtime.release(&root, &upload_id);
    line_edit_stdout(
        json!({
            "path": path,
            "upload_id": upload_id,
            "bytes": bytes,
            "received_bytes": bytes,
            "expected_bytes": state.expected_bytes,
            "expected_sha256": state.expected_sha256,
            "sha256": sha256,
            "mime_type": presentation_mime,
            "committed": true,
        }),
        start,
    )
}

pub(super) fn handle_artifact_upload_abort(
    request: &RunnerFilePayload,
    resolved: &Path,
    start: Instant,
    runtime: &mut ArtifactUploadRuntimeState,
) -> CommandResult {
    let path = request.path.as_str();
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
    if let Err(e) = validate_artifact_runner_path(path) {
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
    runtime.track_existing(&root, &upload_id, state.max_bytes);
    let received_bytes = std::fs::metadata(&part)
        .map(|metadata| metadata.len() as usize)
        .unwrap_or(0);
    let temp_file_removed = std::fs::remove_file(&part).is_ok();
    let sidecar_removed = std::fs::remove_file(&sidecar).is_ok();
    runtime.release(&root, &upload_id);
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    let final_file_exists = std::fs::symlink_metadata(resolved).is_ok();
    let changed_status = if final_file_exists {
        "upload_aborted_final_file_preexisting"
    } else {
        "upload_aborted_no_final_file"
    };
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
            "temp_file_removed": temp_file_removed,
            "sidecar_removed": sidecar_removed,
            "final_file_touched": false,
            "final_file_exists": final_file_exists,
            "changed_path_details": [{
                "path": path,
                "status": changed_status,
            }],
        }),
        start,
    )
}
