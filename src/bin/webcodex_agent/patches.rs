use super::files::sha256_hex_bytes;
use super::output::{line_edit_stdout, CommandResult};
use crate::shell_protocol::ShellAgentShellRequest;
use serde::Deserialize;
use std::io::Write;
use std::path::Path;
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

#[derive(Debug, Deserialize)]
struct AgentApplyTextEditsPayload {
    edits: Vec<AgentTextEdit>,
    #[serde(default)]
    dry_run: Option<bool>,
    #[serde(default)]
    expected_file_sha256: Option<String>,
}

pub(crate) fn handle_apply_text_edits_file_request(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let path = request.path.as_deref().unwrap_or_default();
    let payload_json = request.content.as_deref().unwrap_or_default();
    let payload: AgentApplyTextEditsPayload = match serde_json::from_str(payload_json) {
        Ok(p) => p,
        Err(e) => {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "error": format!("invalid edits payload: {}", e),
                }),
                start,
            );
        }
    };
    let dry_run = payload.dry_run.unwrap_or(false);
    if payload.edits.is_empty() {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": "edits must contain at least one edit",
            }),
            start,
        );
    }
    if payload.edits.len() > APPLY_TEXT_EDITS_MAX_EDITS {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": format!(
                    "too many edits; maximum is {}",
                    APPLY_TEXT_EDITS_MAX_EDITS
                ),
            }),
            start,
        );
    }

    // Read + UTF-8 validate the original file.
    let bytes = match std::fs::read(resolved) {
        Ok(b) => b,
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
    if bytes.len() > APPLY_TEXT_EDITS_MAX_FILE_BYTES {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": format!(
                    "file too large; maximum is {} bytes",
                    APPLY_TEXT_EDITS_MAX_FILE_BYTES
                ),
            }),
            start,
        );
    }
    let original = match String::from_utf8(bytes) {
        Ok(s) => s,
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
    };
    if original.contains('\0') {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "error": "file contains NUL bytes",
            }),
            start,
        );
    }

    let old_sha256 = sha256_hex_bytes(original.as_bytes());
    if let Some(expected) = payload.expected_file_sha256.as_deref() {
        if old_sha256 != expected {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "dry_run": dry_run,
                    "old_sha256": old_sha256,
                    "error": "Rejected before write: expected_file_sha256 mismatch.\nNo files were modified.\nRetry guidance: read the file again to refresh context, then retry with the current file sha256.",
                }),
                start,
            );
        }
    }

    // Resolve each edit to (start, end, replacement, index) against original.
    let mut ops: Vec<(usize, usize, String, usize)> = Vec::with_capacity(payload.edits.len());
    for (index, edit) in payload.edits.iter().enumerate() {
        let kind = &edit.kind;
        let (needle, replacement): (&str, String) = match kind {
            AgentTextEditKind::ReplaceExact => {
                let old = match edit.old_text.as_deref().filter(|v| !v.is_empty()) {
                    Some(v) => v,
                    None => {
                        return apply_text_edits_error(
                            path,
                            index,
                            kind.as_str(),
                            "old_text must be non-empty",
                            start,
                        );
                    }
                };
                let new = edit.new_text.clone().unwrap_or_default();
                (old, new)
            }
            AgentTextEditKind::DeleteExact => {
                let old = match edit.old_text.as_deref().filter(|v| !v.is_empty()) {
                    Some(v) => v,
                    None => {
                        return apply_text_edits_error(
                            path,
                            index,
                            kind.as_str(),
                            "old_text must be non-empty",
                            start,
                        );
                    }
                };
                (old, String::new())
            }
            AgentTextEditKind::InsertBefore | AgentTextEditKind::InsertAfter => {
                let anchor = match edit.anchor_text.as_deref().filter(|v| !v.is_empty()) {
                    Some(v) => v,
                    None => {
                        return apply_text_edits_error(
                            path,
                            index,
                            kind.as_str(),
                            "anchor_text must be non-empty",
                            start,
                        );
                    }
                };
                let new = match edit.new_text.as_deref().filter(|v| !v.is_empty()) {
                    Some(v) => v.to_string(),
                    None => {
                        return apply_text_edits_error(
                            path,
                            index,
                            kind.as_str(),
                            "new_text must be non-empty",
                            start,
                        );
                    }
                };
                (anchor, new)
            }
        };
        if needle.contains('\0') {
            return apply_text_edits_error(
                path,
                index,
                kind.as_str(),
                "match text cannot contain NUL bytes",
                start,
            );
        }
        if needle.len() > APPLY_TEXT_EDITS_MAX_FIELD_BYTES
            || replacement.len() > APPLY_TEXT_EDITS_MAX_FIELD_BYTES
        {
            return apply_text_edits_error(path, index, kind.as_str(), "field too large", start);
        }
        let matches = original.matches(needle).count();
        if matches == 0 {
            return apply_text_edits_error(
                path,
                index,
                kind.as_str(),
                "match text was not found",
                start,
            );
        }
        if matches > 1 {
            return apply_text_edits_error(
                path,
                index,
                kind.as_str(),
                &format!(
                    "match text matched {} times; refusing ambiguous edit",
                    matches
                ),
                start,
            );
        }
        let start_off = original.find(needle).expect("unique match already counted");
        let end_off = start_off + needle.len();
        let (range_start, range_end) = match kind {
            AgentTextEditKind::InsertBefore => (start_off, start_off),
            AgentTextEditKind::InsertAfter => (end_off, end_off),
            _ => (start_off, end_off),
        };
        ops.push((range_start, range_end, replacement, index));
    }

    ops.sort_by_key(|&(s, e, _, i)| (s, e, i));
    for w in ops.windows(2) {
        let (_, e1, _, _) = w[0];
        let (s2, _, _, _) = w[1];
        if s2 < e1 {
            return line_edit_stdout(
                serde_json::json!({
                    "changed": false,
                    "path": path,
                    "dry_run": dry_run,
                    "error": "Rejected before write: edits overlap; refusing ambiguous atomic edit batch.\nNo files were modified.\nRetry guidance: read the file again and ensure edit match ranges do not overlap.",
                }),
                start,
            );
        }
    }

    // Build the new content by slicing the original at op boundaries.
    let mut new_content = String::with_capacity(original.len() + 64);
    let mut cursor = 0usize;
    let mut edit_summaries: Vec<serde_json::Value> = Vec::with_capacity(ops.len());
    for &(start_off, end_off, ref replacement, index) in &ops {
        new_content.push_str(&original[cursor..start_off]);
        new_content.push_str(replacement);
        cursor = end_off;
        let edit = &payload.edits[index];
        let old_start_line = 1 + original[..start_off].matches('\n').count();
        let mut old_end_line = 1 + original[..end_off].matches('\n').count();
        if end_off > start_off
            && end_off <= original.len()
            && original.as_bytes()[end_off - 1] == b'\n'
        {
            old_end_line = old_end_line.saturating_sub(1).max(old_start_line);
        }
        if end_off == start_off {
            old_end_line = old_start_line;
        }
        let new_line_count = if replacement.is_empty() {
            0
        } else {
            replacement.lines().count()
        };
        edit_summaries.push(serde_json::json!({
            "index": index,
            "kind": edit.kind.as_str(),
            "old_start_line": old_start_line,
            "old_end_line": old_end_line,
            "new_line_count": new_line_count,
        }));
    }
    new_content.push_str(&original[cursor..]);

    let new_sha256 = sha256_hex_bytes(new_content.as_bytes());
    let changed = new_content != original;

    if dry_run {
        return line_edit_stdout(
            serde_json::json!({
                "path": path,
                "dry_run": true,
                "applied_count": payload.edits.len(),
                "old_sha256": old_sha256,
                "new_sha256": new_sha256,
                "changed": false,
                "would_change": changed,
                "edits": edit_summaries,
                "changed_paths": [path],
            }),
            start,
        );
    }

    if !changed {
        return line_edit_stdout(
            serde_json::json!({
                "path": path,
                "dry_run": false,
                "applied_count": payload.edits.len(),
                "old_sha256": old_sha256,
                "new_sha256": new_sha256,
                "changed": false,
                "would_change": false,
                "edits": edit_summaries,
                "changed_paths": [],
            }),
            start,
        );
    }

    if let Err(e) = write_file_atomic(resolved, &new_content) {
        return line_edit_stdout(
            serde_json::json!({
                "changed": false,
                "path": path,
                "dry_run": false,
                "old_sha256": old_sha256,
                "error": format!("write failed: {}", e),
            }),
            start,
        );
    }
    line_edit_stdout(
        serde_json::json!({
            "path": path,
            "dry_run": false,
            "applied_count": payload.edits.len(),
            "old_sha256": old_sha256,
            "new_sha256": new_sha256,
            "changed": true,
            "would_change": true,
            "edits": edit_summaries,
            "changed_paths": [path],
        }),
        start,
    )
}

fn apply_text_edits_error(
    path: &str,
    index: usize,
    kind: &str,
    msg: &str,
    start: Instant,
) -> CommandResult {
    line_edit_stdout(
        serde_json::json!({
            "changed": false,
            "path": path,
            "error_kind": match kind {
                "replace_exact" | "delete_exact" => "match_error",
                _ => "match_error",
            },
            "edit_index": index,
            "kind": kind,
            "message": format!(
                "Rejected before write: edit {} ({}): {}.\nNo files were modified.\nRetry guidance: read the file again to refresh context, then retry with a more exact match text.",
                index, kind, msg
            ),
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
