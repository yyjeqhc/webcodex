use super::config::AgentPolicy;
use super::files::{resolve_requested_path, sha256_hex_bytes};
use super::output::{line_edit_stdout, CommandResult};
use crate::shell_protocol::ShellAgentShellRequest;
use serde::Deserialize;
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub(crate) fn is_line_edit_request_kind(kind: &str) -> bool {
    matches!(
        kind,
        "file_replace_line_range"
            | "file_insert_at_line"
            | "file_delete_line_range"
            | "file_replace_exact_block"
            | "file_insert_before_pattern"
            | "file_insert_after_pattern"
            | "file_replace_in_file"
            | "file_write_project_file"
            | "file_apply_text_edits"
    )
}

fn is_sensitive_line_edit_path(path: &str) -> bool {
    let mut components = path.split('/');
    components.any(|component| {
        matches!(
            component,
            ".git" | ".env" | "agent.toml" | "projects.d" | "secrets" | "target" | "node_modules"
        ) || component.starts_with(".env.")
            || component.ends_with(".env")
            || component.ends_with(".toml.bak")
            || component == "webcodex.env"
    })
}

pub(crate) fn validate_line_edit_agent_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if path.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    let raw = Path::new(path);
    if raw.is_absolute() {
        return Err("line edit path must be project-relative".to_string());
    }
    for component in raw.components() {
        match component {
            std::path::Component::Normal(_) => {}
            _ => return Err("line edit path must not escape the project".to_string()),
        }
    }
    if is_sensitive_line_edit_path(path) {
        return Err("refusing to edit sensitive path".to_string());
    }
    Ok(())
}

fn normalize_line_edit_text(text: &str) -> String {
    if text.is_empty() || text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{}\n", text)
    }
}

fn line_edit_text_line_count(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.lines().count()
    }
}

fn write_file_atomic_strict(
    path: &Path,
    content: &str,
    create_dirs: bool,
    tmp_prefix: &str,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "target path has no parent directory".to_string())?;
    if create_dirs {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let original_permissions = std::fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());
    let mut last_error = None;
    for attempt in 0..16 {
        let tmp = parent.join(format!("{tmp_prefix}-{}-{}", std::process::id(), attempt));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(content.as_bytes()) {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(e.to_string());
                }
                if let Err(e) = file.sync_all() {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(e.to_string());
                }
                drop(file);
                if let Some(permissions) = original_permissions.clone() {
                    if let Err(e) = std::fs::set_permissions(&tmp, permissions) {
                        let _ = std::fs::remove_file(&tmp);
                        return Err(e.to_string());
                    }
                }
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
    Err(last_error.unwrap_or_else(|| "could not create temporary file".to_string()))
}

fn write_file_atomic(path: &Path, content: &str) -> Result<(), String> {
    write_file_atomic_strict(path, content, false, ".pd-line")
}

pub(crate) fn handle_line_edit_file_request(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_deref().unwrap_or_default();
    let content = match std::fs::read(resolved) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(_) => {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "error": "file is not valid UTF-8",
                    }),
                    start,
                );
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "error": "file not found",
                }),
                start,
            );
        }
        Err(e) => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "error": format!("read failed: {}", e),
                }),
                start,
            );
        }
    };
    if content.contains('\0') {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": "file contains NUL bytes",
            }),
            start,
        );
    }
    if request
        .content
        .as_deref()
        .map(|text| text.contains('\0'))
        .unwrap_or(false)
    {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": "text cannot contain NUL bytes",
            }),
            start,
        );
    }
    if request
        .old_text
        .as_deref()
        .map(|old_text| old_text.contains('\0'))
        .unwrap_or(false)
    {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": "old_text cannot contain NUL bytes",
            }),
            start,
        );
    }
    if request
        .pattern
        .as_deref()
        .map(|pattern| pattern.contains('\0'))
        .unwrap_or(false)
    {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": "pattern cannot contain NUL bytes",
            }),
            start,
        );
    }
    if request
        .expected_prefix
        .as_deref()
        .map(|prefix| prefix.contains('\0'))
        .unwrap_or(false)
    {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": "expected prefix cannot contain NUL bytes",
            }),
            start,
        );
    }

    let lines: Vec<&str> = if content.is_empty() {
        Vec::new()
    } else {
        content.split_inclusive('\n').collect()
    };
    let total_lines = lines.len();
    let edit = match request.kind.as_str() {
        "file_replace_line_range" | "file_delete_line_range" => {
            let start_line = request.start_line.unwrap_or(0);
            let end_line = request.end_line.unwrap_or(0);
            if start_line == 0 || end_line < start_line || end_line > total_lines {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "start_line": start_line,
                        "end_line": end_line,
                        "error": "invalid line range",
                    }),
                    start,
                );
            }
            let old_text = lines[start_line - 1..end_line].concat();
            let replacement = if request.kind == "file_delete_line_range" {
                String::new()
            } else {
                normalize_line_edit_text(request.content.as_deref().unwrap_or_default())
            };
            let new_content = format!(
                "{}{}{}",
                lines[..start_line - 1].concat(),
                replacement,
                lines[end_line..].concat()
            );
            (
                old_text,
                new_content,
                end_line - start_line + 1,
                line_edit_text_line_count(&replacement),
                serde_json::json!({"start_line": start_line, "end_line": end_line}),
            )
        }
        "file_insert_at_line" => {
            let line = request.line.unwrap_or(0);
            if line == 0 || line > total_lines + 1 {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "line": line,
                        "error": "line out of range",
                    }),
                    start,
                );
            }
            let old_text = if line <= total_lines {
                lines[line - 1].to_string()
            } else {
                String::new()
            };
            let insertion =
                normalize_line_edit_text(request.content.as_deref().unwrap_or_default());
            let new_content = format!(
                "{}{}{}",
                lines[..line - 1].concat(),
                insertion,
                lines[line - 1..].concat()
            );
            (
                old_text,
                new_content,
                if line <= total_lines { 1 } else { 0 },
                line_edit_text_line_count(&insertion),
                serde_json::json!({"line": line}),
            )
        }
        "file_replace_exact_block" => {
            let old = request.old_text.as_deref().unwrap_or_default();
            let new = request.content.as_deref().unwrap_or_default();
            if old.is_empty() {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "error": "old_text must be non-empty",
                    }),
                    start,
                );
            }
            let before_sha256 = sha256_hex_bytes(content.as_bytes());
            if let Some(expected) = request.expected_sha256.as_deref() {
                if before_sha256 != expected {
                    return line_edit_stdout(
                        serde_json::json!({
                            "changed": false,
                            "path": path,
                            "before_sha256": before_sha256,
                            "error": "Rejected before write: expected_old_sha256 mismatch.\nNo files were modified.\nRetry guidance: read the file again to refresh context, then retry with the current file sha256.",
                        }),
                        start,
                    );
                }
            }
            let matches = content.matches(old).count();
            if matches == 0 {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "bytes_before": content.len(),
                        "matches_replaced": 0,
                        "error": format!("Rejected before write: old_text was not found exactly once in path {}.\nNo files were modified.\nRetry guidance: read the file again to refresh context, then retry with a more exact block.", path),
                    }),
                    start,
                );
            }
            if matches > 1 {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "bytes_before": content.len(),
                        "matches_replaced": 0,
                        "error": format!("Rejected before write: old_text matched {} times in path {}; expected exactly one match.\nNo files were modified.\nRetry guidance: make old_text more specific or use replace_line_range with guards.", matches, path),
                    }),
                    start,
                );
            }
            (
                old.to_string(),
                content.replacen(old, new, 1),
                1,
                1,
                serde_json::json!({"matches_replaced": 1}),
            )
        }
        "file_insert_before_pattern" | "file_insert_after_pattern" => {
            let pattern = request.pattern.as_deref().unwrap_or_default();
            let text = request.content.as_deref().unwrap_or_default();
            if pattern.is_empty() {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "error": "pattern must be non-empty literal pattern",
                    }),
                    start,
                );
            }
            if text.is_empty() {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "error": "Rejected before write: inserted text must not be empty.\nNo files were modified.\nRetry guidance: provide the exact text to insert, including any intended newlines.",
                    }),
                    start,
                );
            }
            let matches = content.matches(pattern).count();
            if matches == 0 {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "bytes_before": content.len(),
                        "pattern_matches": 0,
                        "error": format!("Rejected before write: pattern was not found exactly once in path {}.\nNo files were modified.\nRetry guidance: read the file again and retry with a more specific literal pattern.", path),
                    }),
                    start,
                );
            }
            if matches > 1 {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "path": path,
                        "bytes_before": content.len(),
                        "pattern_matches": matches,
                        "error": format!("Rejected before write: pattern matched {} times in path {}; expected exactly one match.\nNo files were modified.\nRetry guidance: use a more specific literal pattern or use insert_at_line with guards.", matches, path),
                    }),
                    start,
                );
            }
            let idx = content.find(pattern).unwrap_or(0);
            let insert_at = if request.kind == "file_insert_after_pattern" {
                idx + pattern.len()
            } else {
                idx
            };
            let mut new_content = String::with_capacity(content.len() + text.len());
            new_content.push_str(&content[..insert_at]);
            new_content.push_str(text);
            new_content.push_str(&content[insert_at..]);
            (
                pattern.to_string(),
                new_content,
                1,
                1,
                serde_json::json!({"pattern_matches": 1}),
            )
        }
        _ => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "error": "invalid operation",
                }),
                start,
            );
        }
    };

    let (old_text, new_content, old_line_count, new_line_count, coords) = edit;
    let old_sha256 = sha256_hex_bytes(old_text.as_bytes());
    let selected_text_sha_guard_applies = request.kind != "file_replace_exact_block";
    if selected_text_sha_guard_applies {
        if let Some(expected) = request.expected_sha256.as_deref() {
            if old_sha256 != expected {
                let err = if request.kind == "file_insert_at_line" {
                    "expected_anchor_sha256 mismatch"
                } else {
                    "expected_old_sha256 mismatch"
                };
                let mut out = serde_json::json!({
                    "changed": false,
                    "path": path,
                    "old_sha256": old_sha256,
                    "error": err,
                });
                merge_json_object(&mut out, coords.clone());
                return line_edit_stdout(out, start);
            }
        }
    }
    if let Some(prefix) = request.expected_prefix.as_deref() {
        if !old_text.starts_with(prefix) {
            let err = if request.kind == "file_insert_at_line" {
                "expected_anchor_prefix mismatch"
            } else {
                "expected_old_prefix mismatch"
            };
            let mut out = serde_json::json!({
                "changed": false,
                "path": path,
                "old_sha256": old_sha256,
                "error": err,
            });
            merge_json_object(&mut out, coords.clone());
            return line_edit_stdout(out, start);
        }
    }
    if let Err(e) = write_file_atomic(resolved, &new_content) {
        let mut out = serde_json::json!({
            "changed": false,
            "path": path,
            "old_sha256": old_sha256,
            "error": format!("write failed: {}", e),
        });
        merge_json_object(&mut out, coords.clone());
        return line_edit_stdout(out, start);
    }
    let new_sha256 = sha256_hex_bytes(new_content.as_bytes());
    let mut out = serde_json::json!({
        "path": path,
        "old_sha256": old_sha256,
        "new_sha256": new_sha256,
        "before_sha256": sha256_hex_bytes(content.as_bytes()),
        "after_sha256": new_sha256,
        "old_line_count": old_line_count,
        "new_line_count": new_line_count,
        "bytes_before": content.len(),
        "bytes_after": new_content.len(),
        "bytes_written": new_content.len(),
        "changed": new_content != content,
    });
    merge_json_object(&mut out, coords);
    line_edit_stdout(out, start)
}

fn parse_json_payload(request: &ShellAgentShellRequest) -> Result<serde_json::Value, String> {
    serde_json::from_str(request.content.as_deref().unwrap_or_default())
        .map_err(|e| format!("invalid json: {}", e))
}

fn parse_expected_replacements(payload: &serde_json::Value) -> Result<i64, ()> {
    let Some(value) = payload.get("expected_replacements") else {
        return Ok(1);
    };
    if let Some(n) = value.as_i64() {
        return Ok(n);
    }
    if let Some(n) = value.as_u64() {
        return i64::try_from(n).map_err(|_| ());
    }
    if let Some(s) = value.as_str() {
        return s.parse::<i64>().map_err(|_| ());
    }
    if let Some(b) = value.as_bool() {
        return Ok(if b { 1 } else { 0 });
    }
    Err(())
}

fn parse_bool_field(payload: &serde_json::Value, key: &str) -> Result<bool, String> {
    match payload.get(key) {
        None | Some(serde_json::Value::Null) => Ok(false),
        Some(serde_json::Value::Bool(v)) => Ok(*v),
        Some(_) => Err(format!("{key} must be a boolean")),
    }
}

pub(crate) fn handle_replace_in_file_request(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_deref().unwrap_or_default();
    let payload = match parse_json_payload(request) {
        Ok(payload) => payload,
        Err(e) => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "error": e,
                }),
                start,
            );
        }
    };

    let old = payload
        .get("old")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let new = match payload.get("new") {
        None => "",
        Some(value) => match value.as_str() {
            Some(value) => value,
            None => {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "error": "new must be a string without NUL",
                    }),
                    start,
                );
            }
        },
    };
    let expected = match parse_expected_replacements(&payload) {
        Ok(expected) => expected,
        Err(()) => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "error": "expected_replacements must be an integer",
                }),
                start,
            );
        }
    };
    let allow_multi = match parse_bool_field(&payload, "allow_multiple") {
        Ok(value) => value,
        Err(e) => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "error": e,
                }),
                start,
            );
        }
    };

    if old.is_empty() || old.contains('\0') {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "error": "old must be a non-empty string without NUL",
            }),
            start,
        );
    }
    if new.contains('\0') {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "error": "new must be a string without NUL",
            }),
            start,
        );
    }
    if expected < 1 {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "error": "expected_replacements must be >= 1",
            }),
            start,
        );
    }
    if !allow_multi && expected != 1 {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "error": "expected_replacements must be 1 when allow_multiple is false",
            }),
            start,
        );
    }

    let content = match std::fs::read(resolved) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(_) => {
                return line_edit_stdout(
                    serde_json::json!({
                        "changed": false,
                        "error": "file is not valid UTF-8",
                        "path": path,
                    }),
                    start,
                );
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "error": "file not found",
                    "path": path,
                }),
                start,
            );
        }
        Err(e) => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "error": format!("read failed: {}", e),
                }),
                start,
            );
        }
    };

    let before = sha256_hex_bytes(content.as_bytes());
    let count = content.matches(old).count();
    if count == 0 {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "before_sha256": before,
                "occurrences": 0,
                "error": "old not found in file",
            }),
            start,
        );
    }
    if count > 1 && !allow_multi {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "before_sha256": before,
                "occurrences": count,
                "error": "old appears multiple times and allow_multiple is false",
            }),
            start,
        );
    }
    if allow_multi && count as i64 != expected {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "before_sha256": before,
                "occurrences": count,
                "expected": expected,
                "error": "expected_replacements mismatch",
            }),
            start,
        );
    }

    let replacements = if allow_multi { expected as usize } else { 1 };
    let replaced = content.replacen(old, new, replacements);
    if let Err(e) = write_file_atomic_strict(resolved, &replaced, false, ".pd-rep") {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "before_sha256": before,
                "error": format!("write failed: {}", e),
            }),
            start,
        );
    }
    let after = sha256_hex_bytes(replaced.as_bytes());
    line_edit_stdout(
        serde_json::json!({
            "changed": true,
            "path": path,
            "replacements": replacements,
            "before_sha256": before,
            "after_sha256": after,
            "bytes_written": replaced.len(),
        }),
        start,
    )
}

fn write_project_file_error(path: serde_json::Value, error: String) -> serde_json::Value {
    serde_json::json!({
        "path": path,
        "created": false,
        "overwritten": false,
        "bytes_written": 0,
        "sha256": serde_json::Value::Null,
        "warning": serde_json::Value::Null,
        "error": error,
    })
}

pub(crate) fn handle_write_project_file_request(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_deref().unwrap_or_default();
    let payload = match parse_json_payload(request) {
        Ok(payload) => payload,
        Err(e) => {
            return line_edit_stdout(write_project_file_error(serde_json::Value::Null, e), start);
        }
    };

    let content = match payload.get("content") {
        Some(value) => match value.as_str() {
            Some(value) => value,
            None => {
                return line_edit_stdout(
                    write_project_file_error(
                        serde_json::json!(path),
                        "content must be a UTF-8 string without NUL".to_string(),
                    ),
                    start,
                );
            }
        },
        None => "",
    };
    if content.contains('\0') {
        return line_edit_stdout(
            write_project_file_error(
                serde_json::json!(path),
                "content must be a UTF-8 string without NUL".to_string(),
            ),
            start,
        );
    }

    let overwrite = match parse_bool_field(&payload, "overwrite") {
        Ok(value) => value,
        Err(e) => {
            return line_edit_stdout(write_project_file_error(serde_json::json!(path), e), start);
        }
    };
    let exists = std::fs::symlink_metadata(resolved).is_ok();
    if exists && !overwrite {
        return line_edit_stdout(
            write_project_file_error(
                serde_json::json!(path),
                "file exists and overwrite is false".to_string(),
            ),
            start,
        );
    }

    let mut warning = serde_json::Value::Null;
    if exists && overwrite {
        let current = match std::fs::read(resolved) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(content) => content,
                Err(_) => {
                    return line_edit_stdout(
                        write_project_file_error(
                            serde_json::json!(path),
                            "existing file is not valid UTF-8".to_string(),
                        ),
                        start,
                    );
                }
            },
            Err(e) => {
                return line_edit_stdout(
                    write_project_file_error(
                        serde_json::json!(path),
                        format!("read failed: {}", e),
                    ),
                    start,
                );
            }
        };
        let expected_sha = payload
            .get("expected_sha256")
            .filter(|value| !value.is_null());
        if let Some(expected) = expected_sha {
            let current_sha = sha256_hex_bytes(current.as_bytes());
            if expected.as_str() != Some(current_sha.as_str()) {
                let mut out = write_project_file_error(
                    serde_json::json!(path),
                    "expected_sha256 mismatch".to_string(),
                );
                out["sha256"] = serde_json::json!(current_sha);
                return line_edit_stdout(out, start);
            }
        }

        let expected_prefix = payload
            .get("expected_content_prefix")
            .filter(|value| !value.is_null());
        if let Some(expected) = expected_prefix {
            if expected.as_str().map(|prefix| current.starts_with(prefix)) != Some(true) {
                return line_edit_stdout(
                    write_project_file_error(
                        serde_json::json!(path),
                        "expected_content_prefix mismatch".to_string(),
                    ),
                    start,
                );
            }
        }
        if expected_sha.is_none() && expected_prefix.is_none() {
            warning = serde_json::json!(
                "overwrite without expected_sha256 or expected_content_prefix; provide expected_sha256 for safer overwrites"
            );
        }
    }

    if let Err(e) = write_file_atomic_strict(resolved, content, true, ".pd-write") {
        return line_edit_stdout(
            write_project_file_error(serde_json::json!(path), format!("write failed: {}", e)),
            start,
        );
    }
    line_edit_stdout(
        serde_json::json!({
            "path": path,
            "created": !exists,
            "overwritten": exists,
            "bytes_written": content.len(),
            "sha256": sha256_hex_bytes(content.as_bytes()),
            "warning": warning,
        }),
        start,
    )
}

/// Maximum file size accepted by `file_apply_text_edits` on the agent side.
const APPLY_TEXT_EDITS_MAX_FILE_BYTES: usize = 2 * 1024 * 1024; // 2 MiB
/// Maximum number of edits in one `file_apply_text_edits` batch.
const APPLY_TEXT_EDITS_MAX_EDITS: usize = 20;
/// Maximum byte size of a single edit field on the agent side.
const APPLY_TEXT_EDITS_MAX_FIELD_BYTES: usize = 512 * 1024; // 512 KiB

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AgentTextEditKind {
    ReplaceExact,
    InsertAfter,
    InsertBefore,
    DeleteExact,
}

impl AgentTextEditKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::ReplaceExact => "replace_exact",
            Self::InsertAfter => "insert_after",
            Self::InsertBefore => "insert_before",
            Self::DeleteExact => "delete_exact",
        }
    }
}

#[derive(Debug, Deserialize)]
struct AgentTextEdit {
    kind: AgentTextEditKind,
    #[serde(default)]
    old_text: Option<String>,
    #[serde(default)]
    new_text: Option<String>,
    #[serde(default)]
    anchor_text: Option<String>,
}

const APPLY_TEXT_EDITS_MAX_CHANGES: usize = 16;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AgentFileChangeKind {
    Edit,
    Create,
    Delete,
    Rename,
}

impl AgentFileChangeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Edit => "edit",
            Self::Create => "create",
            Self::Delete => "delete",
            Self::Rename => "rename",
        }
    }
}

#[derive(Debug, Deserialize)]
struct AgentFileChange {
    kind: AgentFileChangeKind,
    path: String,
    #[serde(default)]
    to_path: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    edits: Vec<AgentTextEdit>,
    #[serde(default)]
    expected_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AgentApplyTextEditsPayload {
    changes: Vec<AgentFileChange>,
    #[serde(default)]
    dry_run: Option<bool>,
}

struct PlannedFileChange {
    index: usize,
    kind: AgentFileChangeKind,
    path: String,
    to_path: Option<String>,
    resolved: PathBuf,
    resolved_to: Option<PathBuf>,
    original: Option<String>,
    replacement: Option<String>,
    permissions: Option<std::fs::Permissions>,
    old_sha256: Option<String>,
    new_sha256: Option<String>,
    edit_summaries: Vec<serde_json::Value>,
    would_change: bool,
}

struct AppliedFileChange {
    plan_index: usize,
    created_dirs: Vec<PathBuf>,
}

fn edit_plan(
    original: &str,
    edits: &[AgentTextEdit],
) -> Result<(String, Vec<serde_json::Value>), (usize, &'static str, String)> {
    if edits.is_empty() || edits.len() > APPLY_TEXT_EDITS_MAX_EDITS {
        return Err((
            0,
            "edit",
            format!(
                "edits must contain 1..={} entries",
                APPLY_TEXT_EDITS_MAX_EDITS
            ),
        ));
    }
    let mut ops: Vec<(usize, usize, String, usize)> = Vec::with_capacity(edits.len());
    for (index, edit) in edits.iter().enumerate() {
        let kind = &edit.kind;
        let (needle, replacement): (&str, String) = match kind {
            AgentTextEditKind::ReplaceExact => {
                let old = edit
                    .old_text
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        (
                            index,
                            kind.as_str(),
                            "old_text must be non-empty".to_string(),
                        )
                    })?;
                if edit.anchor_text.is_some() {
                    return Err((
                        index,
                        kind.as_str(),
                        "anchor_text is not allowed".to_string(),
                    ));
                }
                (old, edit.new_text.clone().unwrap_or_default())
            }
            AgentTextEditKind::DeleteExact => {
                let old = edit
                    .old_text
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        (
                            index,
                            kind.as_str(),
                            "old_text must be non-empty".to_string(),
                        )
                    })?;
                if edit.new_text.is_some() || edit.anchor_text.is_some() {
                    return Err((
                        index,
                        kind.as_str(),
                        "new_text and anchor_text are not allowed".to_string(),
                    ));
                }
                (old, String::new())
            }
            AgentTextEditKind::InsertBefore | AgentTextEditKind::InsertAfter => {
                let anchor = edit
                    .anchor_text
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        (
                            index,
                            kind.as_str(),
                            "anchor_text must be non-empty".to_string(),
                        )
                    })?;
                let new_text = edit
                    .new_text
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        (
                            index,
                            kind.as_str(),
                            "new_text must be non-empty".to_string(),
                        )
                    })?;
                if edit.old_text.is_some() {
                    return Err((index, kind.as_str(), "old_text is not allowed".to_string()));
                }
                (anchor, new_text.to_string())
            }
        };
        if needle.contains('\0') || replacement.contains('\0') {
            return Err((
                index,
                kind.as_str(),
                "edit text cannot contain NUL bytes".to_string(),
            ));
        }
        if needle.len() > APPLY_TEXT_EDITS_MAX_FIELD_BYTES
            || replacement.len() > APPLY_TEXT_EDITS_MAX_FIELD_BYTES
        {
            return Err((index, kind.as_str(), "edit field is too large".to_string()));
        }
        let matches = original.matches(needle).count();
        if matches != 1 {
            return Err((
                index,
                kind.as_str(),
                if matches == 0 {
                    "match text was not found".to_string()
                } else {
                    format!("match text matched {matches} times")
                },
            ));
        }
        let start = original.find(needle).expect("unique match counted above");
        let end = start + needle.len();
        let (range_start, range_end) = match kind {
            AgentTextEditKind::InsertBefore => (start, start),
            AgentTextEditKind::InsertAfter => (end, end),
            _ => (start, end),
        };
        ops.push((range_start, range_end, replacement, index));
    }
    ops.sort_by_key(|&(start, end, _, index)| (start, end, index));
    for pair in ops.windows(2) {
        if pair[1].0 < pair[0].1 {
            return Err((
                pair[1].3,
                edits[pair[1].3].kind.as_str(),
                "edits overlap".to_string(),
            ));
        }
    }
    let mut replacement = String::with_capacity(original.len() + 64);
    let mut cursor = 0usize;
    let mut summaries = Vec::with_capacity(ops.len());
    for &(start, end, ref text, index) in &ops {
        replacement.push_str(&original[cursor..start]);
        replacement.push_str(text);
        cursor = end;
        let old_start_line = 1 + original[..start].matches('\n').count();
        let mut old_end_line = 1 + original[..end].matches('\n').count();
        if end > start && original.as_bytes().get(end - 1) == Some(&b'\n') {
            old_end_line = old_end_line.saturating_sub(1).max(old_start_line);
        }
        if end == start {
            old_end_line = old_start_line;
        }
        summaries.push(serde_json::json!({
            "index": index,
            "kind": edits[index].kind.as_str(),
            "old_start_line": old_start_line,
            "old_end_line": old_end_line,
            "new_line_count": if text.is_empty() { 0 } else { text.lines().count() },
        }));
    }
    replacement.push_str(&original[cursor..]);
    Ok((replacement, summaries))
}

fn batch_error(
    change_index: Option<usize>,
    kind: Option<&str>,
    path: Option<&str>,
    code: &str,
    message: impl Into<String>,
    start: Instant,
) -> CommandResult {
    line_edit_stdout(
        serde_json::json!({
            "changed": false,
            "error_kind": code,
            "change_index": change_index,
            "kind": kind,
            "path": path,
            "error": format!(
                "Rejected transactional file batch: {}. No files were modified. Retry guidance: refresh file hashes/content, correct the failing change, and retry the whole batch.",
                message.into()
            ),
        }),
        start,
    )
}

fn read_batch_file(path: &Path) -> Result<(String, std::fs::Permissions, String), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "file not found".to_string()
        } else {
            format!("metadata failed: {error}")
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("path must be a regular non-symlink file".to_string());
    }
    if metadata.len() as usize > APPLY_TEXT_EDITS_MAX_FILE_BYTES {
        return Err(format!(
            "file exceeds {} bytes",
            APPLY_TEXT_EDITS_MAX_FILE_BYTES
        ));
    }
    let bytes = std::fs::read(path).map_err(|error| format!("read failed: {error}"))?;
    let sha256 = sha256_hex_bytes(&bytes);
    let content = String::from_utf8(bytes).map_err(|_| "file is not valid UTF-8".to_string())?;
    if content.contains('\0') {
        return Err("file contains NUL bytes".to_string());
    }
    Ok((content, metadata.permissions(), sha256))
}

fn require_batch_path_absent(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Err("target already exists".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("target metadata failed: {error}")),
    }
}

fn canonical_batch_identity(path: &Path) -> Result<PathBuf, String> {
    let mut suffix = Vec::new();
    let mut cursor = path;
    loop {
        match std::fs::symlink_metadata(cursor) {
            Ok(_) => {
                let mut identity = std::fs::canonicalize(cursor)
                    .map_err(|error| format!("path canonicalization failed: {error}"))?;
                for component in suffix.iter().rev() {
                    identity.push(component);
                }
                return Ok(identity);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let component = cursor
                    .file_name()
                    .ok_or_else(|| "path has no existing ancestor".to_string())?;
                suffix.push(component.to_os_string());
                cursor = cursor
                    .parent()
                    .ok_or_else(|| "path has no existing ancestor".to_string())?;
            }
            Err(error) => return Err(format!("path metadata failed: {error}")),
        }
    }
}

#[derive(Debug)]
struct ApplyChangeFailure {
    message: String,
    rollback_complete: bool,
}

impl ApplyChangeFailure {
    fn new(message: impl Into<String>, rollback_complete: bool) -> Self {
        Self {
            message: message.into(),
            rollback_complete,
        }
    }
}

impl From<String> for ApplyChangeFailure {
    fn from(message: String) -> Self {
        Self::new(message, true)
    }
}

fn cleanup_created_dirs(created_dirs: &[PathBuf]) -> bool {
    let mut complete = true;
    for directory in created_dirs {
        if let Err(error) = std::fs::remove_dir(directory) {
            if error.kind() != std::io::ErrorKind::NotFound {
                complete = false;
            }
        }
    }
    complete
}

fn create_parent_dirs(path: &Path) -> Result<Vec<PathBuf>, ApplyChangeFailure> {
    let parent = path
        .parent()
        .ok_or_else(|| "target path has no parent directory".to_string())?;
    let mut missing = Vec::new();
    let mut cursor = parent;
    while !cursor.exists() {
        missing.push(cursor.to_path_buf());
        cursor = cursor
            .parent()
            .ok_or_else(|| "target parent has no existing ancestor".to_string())?;
    }
    let mut created = Vec::new();
    for directory in missing.iter().rev() {
        if let Err(error) = std::fs::create_dir(directory) {
            created.reverse();
            let rollback_complete = cleanup_created_dirs(&created);
            return Err(ApplyChangeFailure::new(
                error.to_string(),
                rollback_complete,
            ));
        }
        created.push(directory.clone());
    }
    Ok(missing)
}

fn write_new_file_atomic(path: &Path, content: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "target path has no parent directory".to_string())?;
    for attempt in 0..16 {
        let temporary = parent.join(format!(".pd-batch-new-{}-{attempt}", std::process::id()));
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        };
        if let Err(error) = file
            .write_all(content.as_bytes())
            .and_then(|_| file.sync_all())
        {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.to_string());
        }
        drop(file);
        match std::fs::hard_link(&temporary, path) {
            Ok(()) => {
                let _ = std::fs::remove_file(&temporary);
                return Ok(());
            }
            Err(error) => {
                let _ = std::fs::remove_file(&temporary);
                return Err(error.to_string());
            }
        }
    }
    Err("could not allocate a temporary batch file".to_string())
}

fn rollback_change(plan: &PlannedFileChange) -> Result<(), String> {
    match plan.kind {
        AgentFileChangeKind::Edit => {
            if plan.would_change {
                write_file_atomic(&plan.resolved, plan.original.as_deref().unwrap_or_default())?;
            }
        }
        AgentFileChangeKind::Create => {
            if plan.resolved.exists() {
                std::fs::remove_file(&plan.resolved).map_err(|error| error.to_string())?;
            }
        }
        AgentFileChangeKind::Delete => {
            write_new_file_atomic(&plan.resolved, plan.original.as_deref().unwrap_or_default())?;
            if let Some(permissions) = plan.permissions.clone() {
                std::fs::set_permissions(&plan.resolved, permissions)
                    .map_err(|error| error.to_string())?;
            }
        }
        AgentFileChangeKind::Rename => {
            let destination = plan
                .resolved_to
                .as_deref()
                .ok_or_else(|| "rename rollback missing destination".to_string())?;
            if destination.exists() && !plan.resolved.exists() {
                std::fs::hard_link(destination, &plan.resolved)
                    .map_err(|error| error.to_string())?;
                std::fs::remove_file(destination).map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
}

fn require_planned_source_unchanged(plan: &PlannedFileChange) -> Result<(), ApplyChangeFailure> {
    if matches!(plan.kind, AgentFileChangeKind::Create) {
        return Ok(());
    }
    let (_, _, current_sha256) = read_batch_file(&plan.resolved)?;
    if plan.old_sha256.as_deref() != Some(current_sha256.as_str()) {
        return Err(ApplyChangeFailure::new(
            "source changed after batch preflight",
            true,
        ));
    }
    Ok(())
}

fn apply_change(plan: &PlannedFileChange) -> Result<Vec<PathBuf>, ApplyChangeFailure> {
    require_planned_source_unchanged(plan)?;
    match plan.kind {
        AgentFileChangeKind::Edit => {
            if plan.would_change {
                write_file_atomic(
                    &plan.resolved,
                    plan.replacement.as_deref().unwrap_or_default(),
                )?;
            }
            Ok(Vec::new())
        }
        AgentFileChangeKind::Create => {
            let created_dirs = create_parent_dirs(&plan.resolved)?;
            if let Err(error) = write_new_file_atomic(
                &plan.resolved,
                plan.replacement.as_deref().unwrap_or_default(),
            ) {
                let rollback_complete = cleanup_created_dirs(&created_dirs);
                return Err(ApplyChangeFailure::new(error, rollback_complete));
            }
            Ok(created_dirs)
        }
        AgentFileChangeKind::Delete => {
            std::fs::remove_file(&plan.resolved).map_err(|error| error.to_string())?;
            Ok(Vec::new())
        }
        AgentFileChangeKind::Rename => {
            let destination = plan
                .resolved_to
                .as_deref()
                .ok_or_else(|| "rename destination missing".to_string())?;
            let created_dirs = create_parent_dirs(destination)?;
            if let Err(error) = std::fs::hard_link(&plan.resolved, destination) {
                let rollback_complete = cleanup_created_dirs(&created_dirs);
                return Err(ApplyChangeFailure::new(
                    error.to_string(),
                    rollback_complete,
                ));
            }
            if let Err(error) = std::fs::remove_file(&plan.resolved) {
                let destination_cleanup = std::fs::remove_file(destination);
                let directories_cleaned = cleanup_created_dirs(&created_dirs);
                let rollback_complete = destination_cleanup.is_ok() && directories_cleaned;
                let message = match destination_cleanup {
                    Ok(()) => error.to_string(),
                    Err(cleanup_error) => format!(
                        "failed to remove rename source ({error}); destination cleanup also failed ({cleanup_error})"
                    ),
                };
                return Err(ApplyChangeFailure::new(message, rollback_complete));
            }
            Ok(created_dirs)
        }
    }
}

pub(crate) fn handle_apply_text_edits_file_request(
    policy: &AgentPolicy,
    request: &ShellAgentShellRequest,
    start: Instant,
) -> CommandResult {
    let payload: AgentApplyTextEditsPayload =
        match serde_json::from_str(request.content.as_deref().unwrap_or_default()) {
            Ok(payload) => payload,
            Err(error) => {
                return batch_error(
                    None,
                    None,
                    None,
                    "invalid_payload",
                    format!("invalid JSON payload: {error}"),
                    start,
                )
            }
        };
    if payload.changes.is_empty() || payload.changes.len() > APPLY_TEXT_EDITS_MAX_CHANGES {
        return batch_error(
            None,
            None,
            None,
            "invalid_change_count",
            format!("changes must contain 1..={APPLY_TEXT_EDITS_MAX_CHANGES} entries"),
            start,
        );
    }
    let dry_run = payload.dry_run.unwrap_or(false);
    let mut touched = HashSet::new();
    let mut plans = Vec::with_capacity(payload.changes.len());
    for (index, change) in payload.changes.iter().enumerate() {
        if let Err(error) = validate_line_edit_agent_path(&change.path) {
            return batch_error(
                Some(index),
                Some(change.kind.as_str()),
                Some(&change.path),
                "invalid_path",
                error,
                start,
            );
        }
        let resolved = match resolve_requested_path(policy, request.cwd.as_deref(), &change.path) {
            Ok(path) => path,
            Err(error) => {
                return batch_error(
                    Some(index),
                    Some(change.kind.as_str()),
                    Some(&change.path),
                    "path_policy_rejected",
                    error,
                    start,
                )
            }
        };
        let resolved_identity = match canonical_batch_identity(&resolved) {
            Ok(identity) => identity,
            Err(error) => {
                return batch_error(
                    Some(index),
                    Some(change.kind.as_str()),
                    Some(&change.path),
                    "path_policy_rejected",
                    error,
                    start,
                )
            }
        };
        if !touched.insert(resolved_identity) {
            return batch_error(
                Some(index),
                Some(change.kind.as_str()),
                Some(&change.path),
                "path_overlap",
                "source/destination paths may appear only once after path resolution",
                start,
            );
        }
        let resolved_to = if let Some(to_path) = change.to_path.as_deref() {
            if let Err(error) = validate_line_edit_agent_path(to_path) {
                return batch_error(
                    Some(index),
                    Some(change.kind.as_str()),
                    Some(to_path),
                    "invalid_path",
                    error,
                    start,
                );
            }
            match resolve_requested_path(policy, request.cwd.as_deref(), to_path) {
                Ok(path) => {
                    let identity = match canonical_batch_identity(&path) {
                        Ok(identity) => identity,
                        Err(error) => {
                            return batch_error(
                                Some(index),
                                Some(change.kind.as_str()),
                                Some(to_path),
                                "path_policy_rejected",
                                error,
                                start,
                            )
                        }
                    };
                    if !touched.insert(identity) {
                        return batch_error(
                            Some(index),
                            Some(change.kind.as_str()),
                            Some(to_path),
                            "path_overlap",
                            "source/destination paths may appear only once after path resolution",
                            start,
                        );
                    }
                    Some(path)
                }
                Err(error) => {
                    return batch_error(
                        Some(index),
                        Some(change.kind.as_str()),
                        Some(to_path),
                        "path_policy_rejected",
                        error,
                        start,
                    )
                }
            }
        } else {
            None
        };

        let planned = match change.kind {
            AgentFileChangeKind::Create => {
                if change.to_path.is_some()
                    || change.expected_sha256.is_some()
                    || !change.edits.is_empty()
                {
                    return batch_error(
                        Some(index),
                        Some("create"),
                        Some(&change.path),
                        "invalid_fields",
                        "create allows only path and content",
                        start,
                    );
                }
                let content = match change.content.as_deref() {
                    Some(content)
                        if !content.contains('\0')
                            && content.len() <= APPLY_TEXT_EDITS_MAX_FILE_BYTES =>
                    {
                        content.to_string()
                    }
                    Some(_) => {
                        return batch_error(
                            Some(index),
                            Some("create"),
                            Some(&change.path),
                            "invalid_content",
                            "content contains NUL or exceeds the file-size limit",
                            start,
                        )
                    }
                    None => {
                        return batch_error(
                            Some(index),
                            Some("create"),
                            Some(&change.path),
                            "invalid_fields",
                            "content is required",
                            start,
                        )
                    }
                };
                if let Err(error) = require_batch_path_absent(&resolved) {
                    return batch_error(
                        Some(index),
                        Some("create"),
                        Some(&change.path),
                        "path_exists",
                        error,
                        start,
                    );
                }
                let new_sha256 = sha256_hex_bytes(content.as_bytes());
                PlannedFileChange {
                    index,
                    kind: change.kind,
                    path: change.path.clone(),
                    to_path: None,
                    resolved,
                    resolved_to: None,
                    original: None,
                    replacement: Some(content),
                    permissions: None,
                    old_sha256: None,
                    new_sha256: Some(new_sha256),
                    edit_summaries: Vec::new(),
                    would_change: true,
                }
            }
            AgentFileChangeKind::Edit
            | AgentFileChangeKind::Delete
            | AgentFileChangeKind::Rename => {
                let (original, permissions, old_sha256) = match read_batch_file(&resolved) {
                    Ok(file) => file,
                    Err(error) => {
                        return batch_error(
                            Some(index),
                            Some(change.kind.as_str()),
                            Some(&change.path),
                            "read_failed",
                            error,
                            start,
                        )
                    }
                };
                if change.expected_sha256.as_deref() != Some(old_sha256.as_str()) {
                    return batch_error(
                        Some(index),
                        Some(change.kind.as_str()),
                        Some(&change.path),
                        "sha256_conflict",
                        format!("expected_sha256 does not match current sha256 {old_sha256}"),
                        start,
                    );
                }
                match change.kind {
                    AgentFileChangeKind::Edit => {
                        if change.to_path.is_some() || change.content.is_some() {
                            return batch_error(
                                Some(index),
                                Some("edit"),
                                Some(&change.path),
                                "invalid_fields",
                                "edit does not allow to_path or content",
                                start,
                            );
                        }
                        let (replacement, summaries) = match edit_plan(&original, &change.edits) {
                            Ok(plan) => plan,
                            Err((edit_index, edit_kind, error)) => {
                                return line_edit_stdout(
                                    serde_json::json!({
                                        "changed": false,
                                        "error_kind": "edit_conflict",
                                        "change_index": index,
                                        "edit_index": edit_index,
                                        "kind": edit_kind,
                                        "path": change.path,
                                        "error": format!(
                                            "Rejected transactional file batch: {error}. No files were modified. Retry guidance: read this file again and use an exact unique anchor."
                                        ),
                                    }),
                                    start,
                                )
                            }
                        };
                        let new_sha256 = sha256_hex_bytes(replacement.as_bytes());
                        let would_change = replacement != original;
                        PlannedFileChange {
                            index,
                            kind: change.kind,
                            path: change.path.clone(),
                            to_path: None,
                            resolved,
                            resolved_to: None,
                            original: Some(original),
                            replacement: Some(replacement),
                            permissions: Some(permissions),
                            old_sha256: Some(old_sha256),
                            new_sha256: Some(new_sha256),
                            edit_summaries: summaries,
                            would_change,
                        }
                    }
                    AgentFileChangeKind::Delete => {
                        if change.to_path.is_some()
                            || change.content.is_some()
                            || !change.edits.is_empty()
                        {
                            return batch_error(
                                Some(index),
                                Some("delete"),
                                Some(&change.path),
                                "invalid_fields",
                                "delete allows only path and expected_sha256",
                                start,
                            );
                        }
                        PlannedFileChange {
                            index,
                            kind: change.kind,
                            path: change.path.clone(),
                            to_path: None,
                            resolved,
                            resolved_to: None,
                            original: Some(original),
                            replacement: None,
                            permissions: Some(permissions),
                            old_sha256: Some(old_sha256),
                            new_sha256: None,
                            edit_summaries: Vec::new(),
                            would_change: true,
                        }
                    }
                    AgentFileChangeKind::Rename => {
                        if change.content.is_some() || !change.edits.is_empty() {
                            return batch_error(
                                Some(index),
                                Some("rename"),
                                Some(&change.path),
                                "invalid_fields",
                                "rename allows only path, to_path, and expected_sha256",
                                start,
                            );
                        }
                        let destination = match resolved_to.as_ref() {
                            Some(destination) if destination != &resolved => destination,
                            _ => {
                                return batch_error(
                                    Some(index),
                                    Some("rename"),
                                    Some(&change.path),
                                    "invalid_destination",
                                    "to_path is required and must differ from path",
                                    start,
                                )
                            }
                        };
                        if let Err(error) = require_batch_path_absent(destination) {
                            return batch_error(
                                Some(index),
                                Some("rename"),
                                change.to_path.as_deref(),
                                "path_exists",
                                error,
                                start,
                            );
                        }
                        PlannedFileChange {
                            index,
                            kind: change.kind,
                            path: change.path.clone(),
                            to_path: change.to_path.clone(),
                            resolved,
                            resolved_to,
                            original: Some(original),
                            replacement: None,
                            permissions: Some(permissions),
                            old_sha256: Some(old_sha256.clone()),
                            new_sha256: Some(old_sha256),
                            edit_summaries: Vec::new(),
                            would_change: true,
                        }
                    }
                    AgentFileChangeKind::Create => unreachable!(),
                }
            }
        };
        plans.push(planned);
    }

    let mut changed_paths = Vec::new();
    for plan in &plans {
        if !plan.would_change {
            continue;
        }
        changed_paths.push(plan.path.clone());
        if let Some(to_path) = &plan.to_path {
            changed_paths.push(to_path.clone());
        }
    }
    let would_change = plans.iter().any(|plan| plan.would_change);
    if !dry_run {
        let mut applied = Vec::new();
        for (plan_index, plan) in plans.iter().enumerate() {
            if !plan.would_change {
                continue;
            }
            match apply_change(plan) {
                Ok(created_dirs) => applied.push(AppliedFileChange {
                    plan_index,
                    created_dirs,
                }),
                Err(error) => {
                    let mut rollback_complete = error.rollback_complete;
                    for applied_change in applied.iter().rev() {
                        if let Err(rollback_error) =
                            rollback_change(&plans[applied_change.plan_index])
                        {
                            let _ = rollback_error;
                            rollback_complete = false;
                        }
                        if !cleanup_created_dirs(&applied_change.created_dirs) {
                            rollback_complete = false;
                        }
                    }
                    return line_edit_stdout(
                        serde_json::json!({
                            "changed": !rollback_complete,
                            "error_kind": "transaction_failed",
                            "change_index": plan.index,
                            "kind": plan.kind.as_str(),
                            "path": plan.path,
                            "rollback_complete": rollback_complete,
                            "error": if rollback_complete {
                                format!("Transactional file batch failed and was rolled back: {}", error.message)
                            } else {
                                format!("Transactional file batch failed and rollback was incomplete: {}", error.message)
                            },
                        }),
                        start,
                    );
                }
            }
        }
    }

    let files = plans
        .iter()
        .map(|plan| {
            serde_json::json!({
                "index": plan.index,
                "kind": plan.kind.as_str(),
                "path": plan.path,
                "to_path": plan.to_path,
                "old_sha256": plan.old_sha256,
                "new_sha256": plan.new_sha256,
                "changed": !dry_run && plan.would_change,
                "would_change": plan.would_change,
                "edits": plan.edit_summaries,
            })
        })
        .collect::<Vec<_>>();
    line_edit_stdout(
        serde_json::json!({
            "dry_run": dry_run,
            "applied_count": plans.len(),
            "changed": !dry_run && would_change,
            "would_change": would_change,
            "files": files,
            "changed_paths": changed_paths,
        }),
        start,
    )
}

fn merge_json_object(target: &mut serde_json::Value, source: serde_json::Value) {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
}
