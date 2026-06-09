use super::apply_edit_request_with_metrics;
use super::context::{file_fingerprint, system_time_unix_ms};
use super::get_projects;
use super::security::is_sensitive_path;
use super::source::read_binary_from_url;
use super::truncate_string;
use super::types::{EditOperation, EditRequest, EditResponse, EditResponseMode};
use crate::action_sessions::{
    record_action_event, request_action_session_id, ActionAuditEventInput,
};
use crate::get_db;
use crate::projects::{canonicalize_and_verify, ProjectConfig};
use base64::Engine;
use salvo::prelude::*;
use serde_json::json;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

const MAX_EDIT_FILE_SIZE: u64 = 2 * 1024 * 1024;
const MAX_EDIT_TEXT_SIZE: usize = 200 * 1024;
const MAX_BINARY_ARTIFACT_SIZE: usize = 5 * 1024 * 1024;
const MAX_EDIT_DIFF_LEN: usize = 40_000;

pub(super) fn edit_error(error: String) -> EditResponse {
    EditResponse {
        success: false,
        changed_files: Vec::new(),
        diff: String::new(),
        diff_truncated: false,
        warnings: Vec::new(),
        error: Some(error),
    }
}

fn effective_response_mode(body: &EditRequest) -> EditResponseMode {
    body.response_mode.unwrap_or(EditResponseMode::Full)
}

pub(super) fn finalize_edit_response(
    mut response: EditResponse,
    body: &EditRequest,
) -> EditResponse {
    let (truncated_diff, diff_truncated) = truncate_string(response.diff, MAX_EDIT_DIFF_LEN);
    response.diff = truncated_diff;
    response.diff_truncated = diff_truncated;
    if diff_truncated {
        response
            .warnings
            .push("diff truncated; use git_diff or read_file to inspect".to_string());
    }
    match effective_response_mode(body) {
        EditResponseMode::Full => {}
        EditResponseMode::Summary => {
            response.diff.clear();
        }
        EditResponseMode::Minimal => {
            response.diff.clear();
            response.warnings.clear();
        }
    }
    response
}

pub(super) fn edit_path(edit: &EditOperation) -> &str {
    match edit {
        EditOperation::ReplaceText { path, .. }
        | EditOperation::ReplaceRange { path, .. }
        | EditOperation::AppendFile { path, .. }
        | EditOperation::CreateFile { path, .. }
        | EditOperation::WriteFile { path, .. }
        | EditOperation::CreateBinaryFile { path, .. }
        | EditOperation::WriteBinaryFile { path, .. }
        | EditOperation::CreateBinaryArtifact { path, .. }
        | EditOperation::WriteBinaryArtifact { path, .. }
        | EditOperation::CreateBinaryFileFromUpload { path, .. }
        | EditOperation::WriteBinaryFileFromUpload { path, .. }
        | EditOperation::CreateBinaryFileFromUrl { path, .. }
        | EditOperation::WriteBinaryFileFromUrl { path, .. } => path,
    }
}

pub(super) fn edit_text_len(edit: &EditOperation) -> usize {
    match edit {
        EditOperation::ReplaceText { new_text, .. } => new_text.len(),
        EditOperation::ReplaceRange { new_text, .. } => new_text.len(),
        EditOperation::AppendFile { text, .. } => text.len(),
        EditOperation::CreateFile { content, .. } => content.len(),
        EditOperation::WriteFile { content, .. } => content.len(),
        EditOperation::CreateBinaryFile { .. }
        | EditOperation::WriteBinaryFile { .. }
        | EditOperation::CreateBinaryArtifact { .. }
        | EditOperation::WriteBinaryArtifact { .. }
        | EditOperation::CreateBinaryFileFromUpload { .. }
        | EditOperation::WriteBinaryFileFromUpload { .. }
        | EditOperation::CreateBinaryFileFromUrl { .. }
        | EditOperation::WriteBinaryFileFromUrl { .. } => 0,
    }
}

fn edit_kind(edit: &EditOperation) -> &'static str {
    match edit {
        EditOperation::ReplaceText { .. }
        | EditOperation::ReplaceRange { .. }
        | EditOperation::AppendFile { .. }
        | EditOperation::CreateFile { .. }
        | EditOperation::WriteFile { .. } => "text",
        EditOperation::CreateBinaryFile { .. }
        | EditOperation::WriteBinaryFile { .. }
        | EditOperation::CreateBinaryArtifact { .. }
        | EditOperation::WriteBinaryArtifact { .. }
        | EditOperation::CreateBinaryFileFromUpload { .. }
        | EditOperation::WriteBinaryFileFromUpload { .. }
        | EditOperation::CreateBinaryFileFromUrl { .. }
        | EditOperation::WriteBinaryFileFromUrl { .. } => "binary",
    }
}

pub(super) fn validate_no_mixed_edit_kinds(edits: &[EditOperation]) -> Result<(), String> {
    let mut kinds: HashMap<&str, &'static str> = HashMap::new();
    for edit in edits {
        let path = edit_path(edit);
        let kind = edit_kind(edit);
        if let Some(previous) = kinds.insert(path, kind) {
            if previous != kind {
                return Err(format!(
                    "cannot mix text and binary edits for the same path: {}",
                    path
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "path has no parent directory".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent directory: {}", e))
}

pub(super) fn validate_edit_path(rel_path: &str) -> Result<(), String> {
    if rel_path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if rel_path.starts_with('/') {
        return Err("Absolute paths are not allowed".to_string());
    }
    if rel_path.contains("..") {
        return Err("Path traversal (..) is not allowed".to_string());
    }
    if is_sensitive_path(rel_path) {
        return Err(format!("Cannot modify sensitive path: {}", rel_path));
    }
    Ok(())
}

pub(super) fn simple_binary_diff(path: &str, old_len: Option<usize>, new_len: usize) -> String {
    match old_len {
        Some(old_len) => format!(
            "diff --git a/{0} b/{0}\nBinary files a/{0} and b/{0} differ\n# old size: {1} bytes\n# new size: {2} bytes\n",
            path, old_len, new_len
        ),
        None => format!(
            "diff --git a/{0} b/{0}\nnew file mode 100644\nBinary file b/{0} added\n# new size: {1} bytes\n",
            path, new_len
        ),
    }
}

pub(super) fn simple_file_diff(path: &str, old: Option<&str>, new: &str) -> String {
    let mut out = format!("diff --git a/{0} b/{0}\n--- a/{0}\n+++ b/{0}\n", path);
    out.push_str("@@\n");
    if let Some(old) = old {
        for line in old.lines() {
            out.push_str(&format!("-{}\n", line));
        }
    } else {
        out.push_str("--- /dev/null\n");
    }
    for line in new.lines() {
        out.push_str(&format!("+{}\n", line));
    }
    out
}

pub(super) fn resolve_edit_path(
    root: &Path,
    rel_path: &str,
    must_exist: bool,
) -> Result<PathBuf, String> {
    validate_edit_path(rel_path)?;
    let full_path = root.join(rel_path);
    if must_exist {
        return canonicalize_and_verify(&full_path, root);
    }
    let parent = full_path
        .parent()
        .ok_or_else(|| "path has no parent directory".to_string())?;
    let mut ancestor = parent;
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| "path has no existing parent directory".to_string())?;
    }
    canonicalize_and_verify(ancestor, root)?;
    Ok(full_path)
}

pub(super) fn read_edit_file(path: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Failed to stat file: {}", e))?;
    if meta.len() > MAX_EDIT_FILE_SIZE {
        return Err(format!(
            "File is too large for edit API: {} bytes",
            meta.len()
        ));
    }
    std::fs::read_to_string(path).map_err(|e| format!("Failed to read UTF-8 text file: {}", e))
}

fn validate_expected_fingerprints_local(root: &Path, body: &EditRequest) -> Result<(), String> {
    if body.expected_fingerprints.is_empty() {
        return Ok(());
    }
    let edited_paths = body
        .edits
        .iter()
        .map(|edit| edit_path(edit).to_string())
        .collect::<BTreeSet<_>>();
    for (rel_path, expected) in &body.expected_fingerprints {
        if !edited_paths.contains(rel_path) {
            return Err(format!(
                "expected_fingerprints contains non-edited path: {}",
                rel_path
            ));
        }
        validate_edit_path(rel_path)?;
        let full_path = resolve_edit_path(root, rel_path, true)?;
        let metadata = std::fs::metadata(&full_path)
            .map_err(|e| format!("Failed to stat {}: {}", rel_path, e))?;
        if !metadata.is_file() {
            return Err(format!(
                "expected_fingerprints path is not a file: {}",
                rel_path
            ));
        }
        let modified_unix_ms = metadata
            .modified()
            .ok()
            .and_then(system_time_unix_ms)
            .unwrap_or(0);
        let actual = file_fingerprint("local-v1", rel_path, metadata.len(), modified_unix_ms);
        if actual != expected.trim() {
            return Err(format!(
                "fingerprint mismatch for {}: expected {}, actual {}",
                rel_path,
                expected.trim(),
                actual
            ));
        }
    }
    Ok(())
}

pub(super) fn replace_nth(
    content: &str,
    old_text: &str,
    new_text: &str,
    occurrence: Option<usize>,
) -> Result<String, String> {
    if old_text.is_empty() {
        return Err("old_text cannot be empty".to_string());
    }
    let matches: Vec<usize> = content
        .match_indices(old_text)
        .map(|(idx, _)| idx)
        .collect();
    if matches.is_empty() {
        return Err("old_text was not found".to_string());
    }
    let selected = match occurrence {
        Some(n) if n == 0 => return Err("occurrence is 1-based and must be >= 1".to_string()),
        Some(n) if n <= matches.len() => matches[n - 1],
        Some(n) => {
            return Err(format!(
                "occurrence {} exceeds match count {}",
                n,
                matches.len()
            ))
        }
        None if matches.len() == 1 => matches[0],
        None => {
            return Err(format!(
                "old_text matched {} times; specify occurrence",
                matches.len()
            ))
        }
    };
    let mut output = String::new();
    output.push_str(&content[..selected]);
    output.push_str(new_text);
    output.push_str(&content[selected + old_text.len()..]);
    Ok(output)
}

pub(super) fn replace_line_range(
    content: &str,
    start_line: usize,
    end_line: usize,
    new_text: &str,
) -> Result<String, String> {
    if start_line == 0 || end_line == 0 || start_line > end_line {
        return Err(
            "start_line and end_line must be 1-based and start_line <= end_line".to_string(),
        );
    }
    let had_trailing_newline = content.ends_with('\n');
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    if end_line > lines.len() {
        return Err(format!(
            "line range {}-{} exceeds file line count {}",
            start_line,
            end_line,
            lines.len()
        ));
    }
    let replacement: Vec<String> = if new_text.is_empty() {
        Vec::new()
    } else {
        new_text
            .trim_end_matches('\n')
            .lines()
            .map(|l| l.to_string())
            .collect()
    };
    lines.splice(start_line - 1..end_line, replacement);
    let mut output = lines.join("\n");
    if had_trailing_newline || new_text.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

pub(super) fn load_edit_content(
    root: &Path,
    rel_path: &str,
    paths: &mut HashMap<String, PathBuf>,
    originals: &mut HashMap<String, Option<String>>,
    current: &mut HashMap<String, Option<String>>,
) -> Result<String, String> {
    if let Some(Some(content)) = current.get(rel_path) {
        return Ok(content.clone());
    }
    let full_path = resolve_edit_path(root, rel_path, true)?;
    let content = read_edit_file(&full_path)?;
    paths.insert(rel_path.to_string(), full_path);
    originals
        .entry(rel_path.to_string())
        .or_insert_with(|| Some(content.clone()));
    current.insert(rel_path.to_string(), Some(content.clone()));
    Ok(content)
}

pub(super) fn local_apply_project_edit(proj: &ProjectConfig, body: &EditRequest) -> EditResponse {
    let root = proj.root();
    if !root.exists() {
        return edit_error("Project root does not exist".to_string());
    }
    if let Err(e) = validate_expected_fingerprints_local(&root, body) {
        return edit_error(e);
    }
    let mut paths: HashMap<String, PathBuf> = HashMap::new();
    let mut originals: HashMap<String, Option<String>> = HashMap::new();
    let mut current: HashMap<String, Option<String>> = HashMap::new();
    let mut binary_originals: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    let mut binary_current: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    let mut changed = BTreeSet::new();
    for edit in &body.edits {
        let rel_path = edit_path(edit).to_string();
        if let Err(e) = validate_edit_path(&rel_path) {
            return edit_error(e);
        }
        if edit_text_len(edit) > MAX_EDIT_TEXT_SIZE {
            return edit_error(format!(
                "edit text for {} exceeds {} bytes",
                rel_path, MAX_EDIT_TEXT_SIZE
            ));
        }
        match edit {
            EditOperation::ReplaceText {
                old_text,
                new_text,
                occurrence,
                ..
            } => {
                let before = match load_edit_content(
                    &root,
                    &rel_path,
                    &mut paths,
                    &mut originals,
                    &mut current,
                ) {
                    Ok(c) => c,
                    Err(e) => return edit_error(e),
                };
                let after = match replace_nth(&before, old_text, new_text, *occurrence) {
                    Ok(c) => c,
                    Err(e) => return edit_error(e),
                };
                current.insert(rel_path.clone(), Some(after));
            }
            EditOperation::ReplaceRange {
                start_line,
                end_line,
                new_text,
                ..
            } => {
                let before = match load_edit_content(
                    &root,
                    &rel_path,
                    &mut paths,
                    &mut originals,
                    &mut current,
                ) {
                    Ok(c) => c,
                    Err(e) => return edit_error(e),
                };
                let after = match replace_line_range(&before, *start_line, *end_line, new_text) {
                    Ok(c) => c,
                    Err(e) => return edit_error(e),
                };
                current.insert(rel_path.clone(), Some(after));
            }
            EditOperation::AppendFile { text, .. } => {
                let mut before = match load_edit_content(
                    &root,
                    &rel_path,
                    &mut paths,
                    &mut originals,
                    &mut current,
                ) {
                    Ok(c) => c,
                    Err(e) => return edit_error(e),
                };
                before.push_str(text);
                current.insert(rel_path.clone(), Some(before));
            }
            EditOperation::CreateFile { content, .. } => {
                let full_path = match resolve_edit_path(&root, &rel_path, false) {
                    Ok(p) => p,
                    Err(e) => return edit_error(e),
                };
                if full_path.exists() || matches!(current.get(&rel_path), Some(Some(_))) {
                    return edit_error(format!("File already exists: {}", rel_path));
                }
                paths.insert(rel_path.clone(), full_path);
                originals.entry(rel_path.clone()).or_insert(None);
                current.insert(rel_path.clone(), Some(content.clone()));
            }
            EditOperation::WriteFile {
                content,
                allow_overwrite,
                ..
            } => {
                let full_path = match resolve_edit_path(&root, &rel_path, false) {
                    Ok(p) => p,
                    Err(e) => return edit_error(e),
                };
                if full_path.exists() && !allow_overwrite {
                    return edit_error(format!(
                        "File exists and allow_overwrite is false: {}",
                        rel_path
                    ));
                }
                let old = if full_path.exists() {
                    match read_edit_file(&full_path) {
                        Ok(c) => Some(c),
                        Err(e) => return edit_error(e),
                    }
                } else {
                    None
                };
                paths.insert(rel_path.clone(), full_path);
                originals.entry(rel_path.clone()).or_insert(old);
                current.insert(rel_path.clone(), Some(content.clone()));
            }
            EditOperation::CreateBinaryFile { base64_content, .. }
            | EditOperation::CreateBinaryArtifact { base64_content, .. } => {
                let bytes = match decode_binary_artifact(base64_content, &rel_path) {
                    Ok(bytes) => bytes,
                    Err(e) => return edit_error(e),
                };
                let full_path = match resolve_edit_path(&root, &rel_path, false) {
                    Ok(p) => p,
                    Err(e) => return edit_error(e),
                };
                if full_path.exists() || matches!(binary_current.get(&rel_path), Some(Some(_))) {
                    return edit_error(format!("File already exists: {}", rel_path));
                }
                paths.insert(rel_path.clone(), full_path);
                binary_originals.entry(rel_path.clone()).or_insert(None);
                binary_current.insert(rel_path.clone(), Some(bytes));
            }
            EditOperation::WriteBinaryFile {
                base64_content,
                allow_overwrite,
                ..
            }
            | EditOperation::WriteBinaryArtifact {
                base64_content,
                allow_overwrite,
                ..
            } => {
                let bytes = match decode_binary_artifact(base64_content, &rel_path) {
                    Ok(bytes) => bytes,
                    Err(e) => return edit_error(e),
                };
                let full_path = match resolve_edit_path(&root, &rel_path, false) {
                    Ok(p) => p,
                    Err(e) => return edit_error(e),
                };
                if full_path.exists() && !allow_overwrite {
                    return edit_error(format!(
                        "File exists and allow_overwrite is false: {}",
                        rel_path
                    ));
                }
                let old = if full_path.exists() {
                    match std::fs::read(&full_path) {
                        Ok(bytes) => Some(bytes),
                        Err(e) => return edit_error(format!("Failed to read binary file: {}", e)),
                    }
                } else {
                    None
                };
                paths.insert(rel_path.clone(), full_path);
                binary_originals.entry(rel_path.clone()).or_insert(old);
                binary_current.insert(rel_path.clone(), Some(bytes));
            }
            EditOperation::CreateBinaryFileFromUpload { source_file, .. } => {
                let bytes = match read_binary_from_upload(&root, source_file, &rel_path) {
                    Ok(bytes) => bytes,
                    Err(e) => return edit_error(e),
                };
                let full_path = match resolve_edit_path(&root, &rel_path, false) {
                    Ok(p) => p,
                    Err(e) => return edit_error(e),
                };
                if full_path.exists() || matches!(binary_current.get(&rel_path), Some(Some(_))) {
                    return edit_error(format!("File already exists: {}", rel_path));
                }
                paths.insert(rel_path.clone(), full_path);
                binary_originals.entry(rel_path.clone()).or_insert(None);
                binary_current.insert(rel_path.clone(), Some(bytes));
            }
            EditOperation::WriteBinaryFileFromUpload {
                source_file,
                allow_overwrite,
                ..
            } => {
                let bytes = match read_binary_from_upload(&root, source_file, &rel_path) {
                    Ok(bytes) => bytes,
                    Err(e) => return edit_error(e),
                };
                let full_path = match resolve_edit_path(&root, &rel_path, false) {
                    Ok(p) => p,
                    Err(e) => return edit_error(e),
                };
                if full_path.exists() && !allow_overwrite {
                    return edit_error(format!(
                        "File exists and allow_overwrite is false: {}",
                        rel_path
                    ));
                }
                let old = if full_path.exists() {
                    match std::fs::read(&full_path) {
                        Ok(bytes) => Some(bytes),
                        Err(e) => return edit_error(format!("Failed to read binary file: {}", e)),
                    }
                } else {
                    None
                };
                paths.insert(rel_path.clone(), full_path);
                binary_originals.entry(rel_path.clone()).or_insert(old);
                binary_current.insert(rel_path.clone(), Some(bytes));
            }
            EditOperation::CreateBinaryFileFromUrl { source_url, .. } => {
                let bytes = match read_binary_from_url(source_url, &rel_path) {
                    Ok(bytes) => bytes,
                    Err(e) => return edit_error(e),
                };
                let full_path = match resolve_edit_path(&root, &rel_path, false) {
                    Ok(p) => p,
                    Err(e) => return edit_error(e),
                };
                if full_path.exists() || matches!(binary_current.get(&rel_path), Some(Some(_))) {
                    return edit_error(format!("File already exists: {}", rel_path));
                }
                paths.insert(rel_path.clone(), full_path);
                binary_originals.entry(rel_path.clone()).or_insert(None);
                binary_current.insert(rel_path.clone(), Some(bytes));
            }
            EditOperation::WriteBinaryFileFromUrl {
                source_url,
                allow_overwrite,
                ..
            } => {
                let bytes = match read_binary_from_url(source_url, &rel_path) {
                    Ok(bytes) => bytes,
                    Err(e) => return edit_error(e),
                };
                let full_path = match resolve_edit_path(&root, &rel_path, false) {
                    Ok(p) => p,
                    Err(e) => return edit_error(e),
                };
                if full_path.exists() && !allow_overwrite {
                    return edit_error(format!(
                        "File exists and allow_overwrite is false: {}",
                        rel_path
                    ));
                }
                let old = if full_path.exists() {
                    match std::fs::read(&full_path) {
                        Ok(bytes) => Some(bytes),
                        Err(e) => return edit_error(format!("Failed to read binary file: {}", e)),
                    }
                } else {
                    None
                };
                paths.insert(rel_path.clone(), full_path);
                binary_originals.entry(rel_path.clone()).or_insert(old);
                binary_current.insert(rel_path.clone(), Some(bytes));
            }
        }
        changed.insert(rel_path);
    }
    let changed_files: Vec<String> = changed.into_iter().collect();
    let mut diff = String::new();
    for path in &changed_files {
        if let Some(Some(new_content)) = current.get(path) {
            diff.push_str(&simple_file_diff(
                path,
                originals.get(path).and_then(|v| v.as_deref()),
                new_content,
            ));
        } else if let Some(Some(new_bytes)) = binary_current.get(path) {
            diff.push_str(&simple_binary_diff(
                path,
                binary_originals
                    .get(path)
                    .and_then(|v| v.as_ref())
                    .map(|v| v.len()),
                new_bytes.len(),
            ));
        }
    }
    if !body.dry_run {
        for path in &changed_files {
            if let (Some(full_path), Some(Some(new_content))) = (paths.get(path), current.get(path))
            {
                if let Err(e) = ensure_parent_dir(full_path) {
                    return edit_error(e);
                }
                if let Err(e) = std::fs::write(full_path, new_content) {
                    return edit_error(format!("Failed to write {}: {}", path, e));
                }
            } else if let (Some(full_path), Some(Some(new_bytes))) =
                (paths.get(path), binary_current.get(path))
            {
                if let Err(e) = ensure_parent_dir(full_path) {
                    return edit_error(e);
                }
                if let Err(e) = std::fs::write(full_path, new_bytes) {
                    return edit_error(format!("Failed to write binary {}: {}", path, e));
                }
            }
        }
    }
    finalize_edit_response(
        EditResponse {
            success: true,
            changed_files,
            diff,
            diff_truncated: false,
            warnings: Vec::new(),
            error: None,
        },
        body,
    )
}

pub(super) fn decode_binary_artifact(
    base64_content: &str,
    rel_path: &str,
) -> Result<Vec<u8>, String> {
    if base64_content.len() > MAX_BINARY_ARTIFACT_SIZE * 2 {
        return Err(format!(
            "base64 content for {} is too large; maximum decoded size is {} bytes",
            rel_path, MAX_BINARY_ARTIFACT_SIZE
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_content)
        .map_err(|e| format!("Invalid base64 content for {}: {}", rel_path, e))?;
    if bytes.len() > MAX_BINARY_ARTIFACT_SIZE {
        return Err(format!(
            "binary content for {} exceeds {} bytes",
            rel_path, MAX_BINARY_ARTIFACT_SIZE
        ));
    }
    Ok(bytes)
}

pub(super) fn validate_binary_size(bytes: Vec<u8>, label: &str) -> Result<Vec<u8>, String> {
    if bytes.len() > MAX_BINARY_ARTIFACT_SIZE {
        return Err(format!(
            "binary content for {} exceeds {} bytes",
            label, MAX_BINARY_ARTIFACT_SIZE
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_request(mode: Option<EditResponseMode>) -> EditRequest {
        EditRequest {
            project: "demo".to_string(),
            reason: None,
            dry_run: false,
            response_mode: mode,
            expected_fingerprints: Default::default(),
            edits: Vec::new(),
        }
    }

    fn local_test_project(root: &Path) -> ProjectConfig {
        ProjectConfig {
            path: root.display().to_string(),
            executor: Default::default(),
            host: None,
            ssh_hosts: Vec::new(),
            user: None,
            allow_patch: true,
            allow_command_requests: false,
            allow_raw_command_requests: false,
            default_apply_patch_backend: None,
            allowed_checks: Vec::new(),
            checks: None,
            commands: HashMap::new(),
        }
    }

    #[test]
    fn finalize_edit_response_summary_omits_diff() {
        let resp = finalize_edit_response(
            EditResponse {
                success: true,
                changed_files: vec!["a.txt".to_string()],
                diff: "hello".to_string(),
                diff_truncated: false,
                warnings: vec!["note".to_string()],
                error: None,
            },
            &empty_request(Some(EditResponseMode::Summary)),
        );
        assert!(resp.diff.is_empty());
        assert_eq!(resp.warnings, vec!["note".to_string()]);
    }

    #[test]
    fn finalize_edit_response_minimal_omits_diff_and_warnings() {
        let resp = finalize_edit_response(
            EditResponse {
                success: true,
                changed_files: vec!["a.txt".to_string()],
                diff: "hello".to_string(),
                diff_truncated: false,
                warnings: vec!["note".to_string()],
                error: None,
            },
            &empty_request(Some(EditResponseMode::Minimal)),
        );
        assert!(resp.diff.is_empty());
        assert!(resp.warnings.is_empty());
    }

    #[test]
    fn finalize_edit_response_full_truncates_large_diff() {
        let resp = finalize_edit_response(
            EditResponse {
                success: true,
                changed_files: vec!["a.txt".to_string()],
                diff: "x".repeat(MAX_EDIT_DIFF_LEN + 100),
                diff_truncated: false,
                warnings: Vec::new(),
                error: None,
            },
            &empty_request(Some(EditResponseMode::Full)),
        );
        assert!(resp.diff_truncated);
        assert!(resp.diff.len() <= MAX_EDIT_DIFF_LEN);
        assert!(resp.warnings.iter().any(|w| w.contains("diff truncated")));
    }

    #[test]
    fn local_edit_rejects_stale_expected_fingerprint() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("a.txt");
        std::fs::write(&file, "hello\n").unwrap();
        let metadata = std::fs::metadata(&file).unwrap();
        let modified_unix_ms = metadata
            .modified()
            .ok()
            .and_then(system_time_unix_ms)
            .unwrap_or(0);
        let actual = file_fingerprint("local-v1", "a.txt", metadata.len(), modified_unix_ms);
        let proj = local_test_project(tmp.path());

        let mut stale = std::collections::BTreeMap::new();
        stale.insert("a.txt".to_string(), "local-v1-stale".to_string());
        let rejected = local_apply_project_edit(
            &proj,
            &EditRequest {
                project: "demo".to_string(),
                reason: None,
                dry_run: false,
                response_mode: Some(EditResponseMode::Minimal),
                expected_fingerprints: stale,
                edits: vec![EditOperation::ReplaceText {
                    path: "a.txt".to_string(),
                    old_text: "hello".to_string(),
                    new_text: "goodbye".to_string(),
                    occurrence: None,
                }],
            },
        );
        assert!(!rejected.success);
        assert!(rejected.error.unwrap().contains("fingerprint mismatch"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");

        let mut expected = std::collections::BTreeMap::new();
        expected.insert("a.txt".to_string(), actual);
        let accepted = local_apply_project_edit(
            &proj,
            &EditRequest {
                project: "demo".to_string(),
                reason: None,
                dry_run: false,
                response_mode: Some(EditResponseMode::Minimal),
                expected_fingerprints: expected,
                edits: vec![EditOperation::ReplaceText {
                    path: "a.txt".to_string(),
                    old_text: "hello".to_string(),
                    new_text: "goodbye".to_string(),
                    occurrence: None,
                }],
            },
        );
        assert!(accepted.success);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "goodbye\n");
    }
}

pub(super) fn allowed_upload_roots(project_root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![
        project_root.to_path_buf(),
        std::env::temp_dir(),
        PathBuf::from("/tmp"),
        PathBuf::from("/var/tmp"),
        PathBuf::from("/mnt/data"),
    ];
    if let Ok(drop_data) = std::env::var("DROP_DATA") {
        roots.push(PathBuf::from(drop_data).join("uploads"));
    }
    roots
}

pub(super) fn read_binary_from_upload(
    project_root: &Path,
    source_file: &str,
    rel_path: &str,
) -> Result<Vec<u8>, String> {
    if source_file.is_empty() {
        return Err("source_file cannot be empty".to_string());
    }
    if source_file.contains("..") {
        return Err("source_file path traversal is not allowed".to_string());
    }
    if is_sensitive_path(source_file) {
        return Err("source_file cannot reference a sensitive path".to_string());
    }
    let source_path = PathBuf::from(source_file);
    let full = if source_path.is_absolute() {
        source_path
    } else {
        project_root.join(source_path)
    };
    let canonical = full
        .canonicalize()
        .map_err(|e| format!("Failed to access source_file: {}", e))?;
    let mut allowed = false;
    for root in allowed_upload_roots(project_root) {
        if let Ok(root) = root.canonicalize() {
            if canonical.starts_with(&root) {
                allowed = true;
                break;
            }
        }
    }
    if !allowed {
        return Err("source_file is outside allowed upload/temp directories".to_string());
    }
    let meta =
        std::fs::metadata(&canonical).map_err(|e| format!("Failed to stat source_file: {}", e))?;
    if !meta.is_file() {
        return Err("source_file must be a regular file".to_string());
    }
    if meta.len() as usize > MAX_BINARY_ARTIFACT_SIZE {
        return Err(format!(
            "source_file for {} exceeds {} bytes",
            rel_path, MAX_BINARY_ARTIFACT_SIZE
        ));
    }
    let bytes =
        std::fs::read(&canonical).map_err(|e| format!("Failed to read source_file: {}", e))?;
    validate_binary_size(bytes, rel_path)
}

#[handler]
pub async fn codex_edit(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let started_at = chrono::Utc::now().timestamp();
    let audit_clock = std::time::Instant::now();
    let audit_db = get_db(depot);
    let explicit_session_id = request_action_session_id(req);
    let Some(projects) = get_projects(depot) else {
        res.render(Json(edit_error("Projects not configured".to_string())));
        return;
    };
    let body: EditRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(edit_error(format!("Invalid JSON: {}", e))));
            return;
        }
    };
    let proj = match projects.get_project(&body.project) {
        Ok(p) => p,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(edit_error(e)));
            return;
        }
    };
    if !proj.allow_patch() {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(edit_error(
            "Edit is not allowed for this project".to_string(),
        )));
        return;
    }
    if body.edits.is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(edit_error("edits cannot be empty".to_string())));
        return;
    }
    if let Err(e) = validate_no_mixed_edit_kinds(&body.edits) {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(edit_error(e)));
        return;
    }
    for edit in &body.edits {
        if let Err(e) = validate_edit_path(edit_path(edit)) {
            res.status_code(StatusCode::FORBIDDEN);
            res.render(Json(edit_error(e)));
            return;
        }
        if edit_text_len(edit) > MAX_EDIT_TEXT_SIZE {
            res.status_code(StatusCode::PAYLOAD_TOO_LARGE);
            res.render(Json(edit_error(format!(
                "edit text for {} exceeds {} bytes",
                edit_path(edit),
                MAX_EDIT_TEXT_SIZE
            ))));
            return;
        }
    }
    let response = apply_edit_request_with_metrics(&projects, proj, &body, "applyProjectEdit");
    if let Some(db) = audit_db.as_ref() {
        let ended_at = chrono::Utc::now().timestamp();
        let changed_files = response.changed_files.clone();
        let edit_types = body
            .edits
            .iter()
            .map(|edit| match edit {
                EditOperation::ReplaceText { .. } => "replace_text",
                EditOperation::ReplaceRange { .. } => "replace_range",
                EditOperation::AppendFile { .. } => "append_file",
                EditOperation::CreateFile { .. } => "create_file",
                EditOperation::WriteFile { .. } => "write_file",
                EditOperation::CreateBinaryFile { .. } => "create_binary_file",
                EditOperation::WriteBinaryFile { .. } => "write_binary_file",
                EditOperation::CreateBinaryArtifact { .. } => "create_binary_artifact",
                EditOperation::WriteBinaryArtifact { .. } => "write_binary_artifact",
                EditOperation::CreateBinaryFileFromUpload { .. } => {
                    "create_binary_file_from_upload"
                }
                EditOperation::WriteBinaryFileFromUpload { .. } => "write_binary_file_from_upload",
                EditOperation::CreateBinaryFileFromUrl { .. } => "create_binary_file_from_url",
                EditOperation::WriteBinaryFileFromUrl { .. } => "write_binary_file_from_url",
            })
            .collect::<Vec<_>>();
        record_action_event(
            db,
            ActionAuditEventInput {
                explicit_session_id,
                session_title: None,
                endpoint: "/api/codex/edit".to_string(),
                action_name: "applyProjectEdit".to_string(),
                operation: Some(edit_types.join(",")),
                project: Some(body.project.clone()),
                status: if response.success {
                    "success".to_string()
                } else {
                    "failed".to_string()
                },
                http_status: Some(res.status_code.unwrap_or(StatusCode::OK).as_u16() as i64),
                started_at,
                ended_at,
                duration_ms: audit_clock.elapsed().as_millis() as i64,
                error_summary: response.error.clone(),
                warning_summary: if response.warnings.is_empty() {
                    None
                } else {
                    Some(response.warnings.join(" | "))
                },
                changed_files,
                ids: json!({}),
                summary: json!({
                    "edit_types": edit_types,
                    "paths": body.edits.iter().map(|edit| edit_path(edit).to_string()).collect::<Vec<_>>(),
                    "response_mode": body.response_mode,
                    "dry_run": body.dry_run,
                    "diff_truncated": response.diff_truncated,
                }),
                request_bytes: None,
                response_bytes: None,
            },
        );
    }
    res.render(Json(response));
}
