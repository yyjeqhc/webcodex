use super::security::is_sensitive_path;
use super::types::{EditOperation, EditResponse};
use std::collections::HashMap;
use std::path::Path;

pub(super) fn edit_error(error: String) -> EditResponse {
    EditResponse {
        success: false,
        changed_files: Vec::new(),
        diff: String::new(),
        warnings: Vec::new(),
        error: Some(error),
    }
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
