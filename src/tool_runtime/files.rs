use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

use super::helpers::{
    run_command_sync, shell_escape_simple, shell_join_paths, validate_limited_cleanup_paths,
    validate_project_relative_path,
};
use super::types::ToolResult;
use super::ToolRuntime;
use crate::projects::ProjectConfig;
use crate::shell_protocol::{ShellFileOpRequest, ShellRunRequest};

#[cfg(test)]
pub(crate) fn read_file_content_result(
    content: String,
    start_line: Option<usize>,
    limit: Option<usize>,
) -> ToolResult {
    read_file_content_result_with_options(content, start_line, limit, false)
}

pub(crate) fn read_file_content_result_with_options(
    content: String,
    start_line: Option<usize>,
    limit: Option<usize>,
    with_line_numbers: bool,
) -> ToolResult {
    let all_lines: Vec<&str> = content.lines().collect();
    let total_lines = all_lines.len();
    let eff_start = start_line.unwrap_or(1).max(1);
    let eff_limit = limit.unwrap_or(2000).clamp(1, 2000);
    if eff_start > total_lines {
        let mut output = json!({
            "content": "",
            "total_lines": total_lines,
            "start_line": eff_start,
            "limit": eff_limit,
        });
        if with_line_numbers {
            add_line_number_fields(&mut output, eff_start, &Vec::<&str>::new());
        }
        return ToolResult::ok(output);
    }
    let start_idx = eff_start - 1;
    let end_idx = (start_idx + eff_limit).min(total_lines);
    let selected_lines = &all_lines[start_idx..end_idx];
    let slice = selected_lines.join("\n");
    let mut output = json!({
        "content": slice,
        "total_lines": total_lines,
        "start_line": eff_start,
        "limit": eff_limit,
    });
    if with_line_numbers {
        add_line_number_fields(&mut output, eff_start, selected_lines);
    }
    ToolResult::ok(output)
}

#[cfg(test)]
pub(crate) fn read_file_agent_stdout_result(
    stdout: String,
    start_line: Option<usize>,
    limit: Option<usize>,
) -> ToolResult {
    read_file_agent_stdout_result_with_options(stdout, start_line, limit, false)
}

pub(crate) fn read_file_agent_stdout_result_with_options(
    stdout: String,
    start_line: Option<usize>,
    limit: Option<usize>,
    with_line_numbers: bool,
) -> ToolResult {
    let trimmed = stdout.trim();
    if let Ok(mut value) = serde_json::from_str::<Value>(trimmed) {
        if value.get("format").and_then(|format| format.as_str())
            == Some("webcodex.file_read_range.v1")
        {
            if with_line_numbers {
                add_agent_read_file_line_number_fields(&mut value, start_line, limit);
            }
            return ToolResult::ok(value);
        }
    }
    read_file_content_result_with_options(stdout, start_line, limit, with_line_numbers)
}

pub(crate) fn effective_read_file_range(
    start_line: Option<usize>,
    limit: Option<usize>,
) -> (usize, usize, usize) {
    let eff_start = start_line.unwrap_or(1).max(1);
    let eff_limit = limit.unwrap_or(2000).clamp(1, 2000);
    let eff_end = eff_start.saturating_add(eff_limit).saturating_sub(1);
    (eff_start, eff_limit, eff_end)
}

/// Parse the stdout of a best-effort agent `file_read` for an instruction
/// candidate. Recognizes the `webcodex.file_read_range.v1` JSON envelope
/// (which carries the true `total_lines` of the file) and falls back to
/// treating stdout as raw text (where the returned line count is a lower
/// bound on the true total). Returns `None` for empty/unusable output so the
/// caller skips to the next candidate.
fn parse_instruction_agent_stdout(stdout: String) -> Option<(String, usize)> {
    let trimmed = stdout.trim();
    if !trimmed.is_empty() {
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            if value.get("format").and_then(|format| format.as_str())
                == Some("webcodex.file_read_range.v1")
            {
                let content = value.get("content").and_then(|c| c.as_str())?.to_string();
                let total_lines = value
                    .get("total_lines")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as usize;
                if content_is_empty_instruction(&content) {
                    return None;
                }
                return Some((content, total_lines));
            }
        }
    }
    if content_is_empty_instruction(&stdout) {
        return None;
    }
    let total_lines = stdout.lines().count();
    Some((stdout, total_lines))
}

/// True when an instruction body carries no meaningful content (empty or
/// whitespace-only). Empty instruction files are skipped so a later candidate
/// can win.
fn content_is_empty_instruction(content: &str) -> bool {
    content.trim().is_empty()
}

fn add_line_number_fields<T: AsRef<str>>(output: &mut Value, start_line: usize, texts: &[T]) {
    let lines: Vec<Value> = texts
        .iter()
        .enumerate()
        .map(|(idx, text)| {
            json!({
                "line": start_line.saturating_add(idx),
                "text": text.as_ref(),
            })
        })
        .collect();
    let numbered_text = texts
        .iter()
        .enumerate()
        .map(|(idx, text)| format!("{} | {}", start_line.saturating_add(idx), text.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(obj) = output.as_object_mut() {
        obj.insert("numbered_text".to_string(), Value::String(numbered_text));
        obj.insert("lines".to_string(), Value::Array(lines));
    }
}

fn add_agent_read_file_line_number_fields(
    output: &mut Value,
    request_start_line: Option<usize>,
    request_limit: Option<usize>,
) {
    let content = output
        .get("content")
        .and_then(|content| content.as_str())
        .unwrap_or("");
    let start_line = output
        .get("start_line")
        .and_then(|line| line.as_u64())
        .and_then(|line| usize::try_from(line).ok())
        .or(request_start_line)
        .unwrap_or(1)
        .max(1);
    let limit = output
        .get("limit")
        .and_then(|limit| limit.as_u64())
        .and_then(|limit| usize::try_from(limit).ok())
        .or(request_limit)
        .unwrap_or(2000)
        .clamp(1, 2000);
    let selected_count = output
        .get("total_lines")
        .and_then(|total| total.as_u64())
        .and_then(|total| usize::try_from(total).ok())
        .map(|total_lines| {
            if start_line > total_lines {
                0
            } else {
                total_lines
                    .saturating_sub(start_line)
                    .saturating_add(1)
                    .min(limit)
            }
        });
    let texts = line_texts_from_content(content, selected_count);
    add_line_number_fields(output, start_line, &texts);
}

fn line_texts_from_content(content: &str, selected_count: Option<usize>) -> Vec<String> {
    match selected_count {
        Some(0) => Vec::new(),
        Some(count) => {
            let mut texts: Vec<String> = content.split('\n').map(str::to_string).collect();
            texts.resize(count, String::new());
            texts.truncate(count);
            texts
        }
        None if content.is_empty() => Vec::new(),
        None => content.lines().map(str::to_string).collect(),
    }
}

// =============================================================================
// Phase A read-only console helpers
// =============================================================================

/// Build the project-relative path for a single entry returned by an agent
/// `file_list` op. `rel_path` is the project-relative directory the caller
/// requested (`"."` for the project root); `name` is the bare entry name.
pub(crate) fn relative_entry_path(rel_path: &str, name: &str) -> String {
    let trimmed = rel_path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "." {
        name.to_string()
    } else {
        format!("{}/{}", trimmed, name)
    }
}

/// Parse agent `file_list` stdout (one entry per line, dirs suffixed with
/// `/`) into bounded project-relative entries with a file/dir kind. Returns
/// the entries and whether the source exceeded `max_entries`.
pub(crate) fn parse_file_list_entries(
    stdout: &str,
    rel_path: &str,
    max_entries: usize,
) -> (Vec<Value>, bool) {
    let mut all: Vec<Value> = Vec::new();
    for line in stdout.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let (name, is_dir) = if let Some(stripped) = line.strip_suffix('/') {
            (stripped.to_string(), true)
        } else {
            (line.to_string(), false)
        };
        if name.is_empty() {
            continue;
        }
        all.push(json!({
            "path": relative_entry_path(rel_path, &name),
            "kind": if is_dir { "dir" } else { "file" },
        }));
    }
    all.sort_by(|a, b| {
        a["path"]
            .as_str()
            .unwrap_or("")
            .cmp(b["path"].as_str().unwrap_or(""))
    });
    let truncated = all.len() > max_entries;
    all.truncate(max_entries);
    (all, truncated)
}

/// Build a bounded `grep -rnI` command for `search_project_text`. Excludes
/// sensitive/build directories (`.git`, `target`, `node_modules`) by default
/// and caps output with `head -n (max_matches + 1)` so the runtime can detect
/// truncation without requesting an unbounded stream.
pub(crate) fn search_project_text_command(
    pattern: &str,
    rel_path: &str,
    max_matches: usize,
) -> String {
    let escaped_pattern = shell_escape_simple(pattern);
    let escaped_target = shell_escape_simple(rel_path);
    // head -n N+1: one extra line lets the parser flag truncation.
    let head = max_matches.saturating_add(1);
    format!(
        "grep -rnI --exclude-dir=.git --exclude-dir=target --exclude-dir=node_modules -e {pattern} {target} 2>/dev/null | head -n {head}",
        pattern = escaped_pattern,
        target = escaped_target,
        head = head,
    )
}

pub(crate) const MAX_SEARCH_CONTEXT_LINES: usize = 20;

const SEARCH_PROJECT_TEXT_CONTEXT_HELPER: &str = r#"
import json
import os
import sys

pattern = sys.argv[1]
root_arg = sys.argv[2]
max_matches = int(sys.argv[3])
context_before = int(sys.argv[4])
context_after = int(sys.argv[5])
excluded_dirs = {'.git', 'target', 'node_modules'}

def emit(obj):
    sys.stdout.write(json.dumps(obj, ensure_ascii=False) + '\n')

def iter_files(root):
    if os.path.isfile(root):
        yield root
        return
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(d for d in dirnames if d not in excluded_dirs)
        for filename in sorted(filenames):
            yield os.path.join(dirpath, filename)

matches = 0
truncated = False
for path in iter_files(root_arg):
    rel = os.path.relpath(path, '.')
    try:
        with open(path, 'r', encoding='utf-8') as f:
            lines = f.read().splitlines()
    except (UnicodeDecodeError, OSError):
        continue
    for idx, text in enumerate(lines):
        if pattern not in text:
            continue
        if matches >= max_matches:
            truncated = True
            emit({'truncated': True})
            sys.exit(0)
        before_start = max(0, idx - context_before)
        after_end = min(len(lines), idx + context_after + 1)
        emit({
            'path': rel[2:] if rel.startswith('./') else rel,
            'line': idx + 1,
            'preview': text,
            'context_before': [
                {'line': n + 1, 'text': lines[n]}
                for n in range(before_start, idx)
            ],
            'context_after': [
                {'line': n + 1, 'text': lines[n]}
                for n in range(idx + 1, after_end)
            ],
        })
        matches += 1
emit({'truncated': truncated})
"#;

pub(crate) fn effective_search_context(
    context_before: Option<usize>,
    context_after: Option<usize>,
) -> (usize, usize) {
    (
        context_before.unwrap_or(0).min(MAX_SEARCH_CONTEXT_LINES),
        context_after.unwrap_or(0).min(MAX_SEARCH_CONTEXT_LINES),
    )
}

pub(crate) fn search_project_text_context_command(
    pattern: &str,
    rel_path: &str,
    max_matches: usize,
    context_before: usize,
    context_after: usize,
) -> String {
    format!(
        "python3 -c {helper} {pattern} {target} {max_matches} {before} {after}",
        helper = shell_escape_simple(SEARCH_PROJECT_TEXT_CONTEXT_HELPER),
        pattern = shell_escape_simple(pattern),
        target = shell_escape_simple(rel_path),
        max_matches = max_matches,
        before = context_before,
        after = context_after,
    )
}

/// Parse `grep -rnI` output lines (`path:lineno:content`) into bounded match
/// objects. Strips a leading `./` so paths are project-relative. Returns the
/// matches and whether the source exceeded `max_matches`.
pub(crate) fn parse_search_matches(stdout: &str, max_matches: usize) -> (Vec<Value>, bool) {
    let mut matches: Vec<Value> = Vec::new();
    let mut truncated = false;
    for line in stdout.lines() {
        if matches.len() >= max_matches {
            truncated = true;
            break;
        }
        let mut parts = line.splitn(3, ':');
        let (Some(path), Some(lineno), Some(content)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let line_no: usize = match lineno.parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        let clean_path = path.strip_prefix("./").unwrap_or(path).to_string();
        matches.push(json!({
            "path": clean_path,
            "line": line_no,
            "preview": content,
        }));
    }
    (matches, truncated)
}

pub(crate) fn parse_search_context_matches(stdout: &str, max_matches: usize) -> (Vec<Value>, bool) {
    let mut matches: Vec<Value> = Vec::new();
    let mut truncated = false;
    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value
            .get("truncated")
            .and_then(|truncated| truncated.as_bool())
            .unwrap_or(false)
        {
            truncated = true;
            break;
        }
        if matches.len() >= max_matches {
            truncated = true;
            break;
        }
        let Some(path) = value.get("path").and_then(|path| path.as_str()) else {
            continue;
        };
        let Some(line_no) = value.get("line").and_then(|line| line.as_u64()) else {
            continue;
        };
        let Some(preview) = value.get("preview").and_then(|preview| preview.as_str()) else {
            continue;
        };
        let context_before = parse_context_lines(value.get("context_before"));
        let context_after = parse_context_lines(value.get("context_after"));
        matches.push(json!({
            "path": path,
            "line": line_no,
            "preview": preview,
            "context_before": context_before,
            "context_after": context_after,
        }));
    }
    (matches, truncated)
}

fn parse_context_lines(value: Option<&Value>) -> Vec<Value> {
    value
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let line_no = item.get("line").and_then(|line| line.as_u64())?;
                    let text = item.get("text").and_then(|text| text.as_str())?;
                    Some(json!({
                        "line": line_no,
                        "text": text,
                    }))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Maximum accepted size for a single `replace_in_file` `old`/`new` field.
/// Generous for text edits while bounding memory and the agent stdin payload.
pub(crate) const MAX_REPLACE_FIELD_BYTES: usize = 256 * 1024; // 256 KiB

/// Maximum accepted size for `write_project_file` `content`. Bounded by the
/// agent run-shell stdin transport (`RUN_HELPER_STDIN_BUDGET`).
pub(crate) const MAX_WRITE_CONTENT_BYTES: usize = 256 * 1024; // 256 KiB

/// Maximum accepted size for line-edit expected prefix guards. Keep this well
/// below the helper stdin budget so oversized optimistic-concurrency guards
/// fail locally before any agent helper request is enqueued.
pub(crate) const MAX_EXPECTED_PREFIX_BYTES: usize = 64 * 1024; // 64 KiB

/// Hard cap on the serialized helper payload sent to the agent over the
/// run-shell stdin transport. Mirrors `MAX_RUN_STDIN_BYTES` in shell_client
/// without coupling this module to that private constant.
pub(crate) const RUN_HELPER_STDIN_BUDGET: usize = 15 * 1024 * 1024; // 15 MiB

fn recoverable_write_rejection(reason: impl AsRef<str>) -> String {
    format!(
        "Rejected before write: {}.\nNo files were modified.\nRetry guidance: read the file again to refresh line numbers/context, then retry with updated guards.",
        reason.as_ref()
    )
}

/// Maximum decoded size for one binary project artifact imported through GPT
/// Actions/runtime tools. Keep bounded because the current agent helper path
/// carries base64 over stdin.
pub(crate) const MAX_PROJECT_ARTIFACT_BYTES: usize = 10 * 1024 * 1024; // 10 MiB

/// Default returned segment size for `read_project_artifact`. This tool returns
/// base64 content in the JSON response, so keep chunks small for GPT Actions.
pub(crate) const DEFAULT_READ_PROJECT_ARTIFACT_LENGTH: usize = 32 * 1024; // 32 KiB

/// Maximum returned segment size for `read_project_artifact`.
pub(crate) const MAX_READ_PROJECT_ARTIFACT_LENGTH: usize = 64 * 1024; // 64 KiB

/// Hard cap for a base64-encoded artifact payload plus JSON overhead.
pub(crate) const MAX_PROJECT_ARTIFACT_BASE64_BYTES: usize = 14 * 1024 * 1024; // ~10 MiB decoded

/// Validate a project-relative file path for the Phase 4 structured edit
/// tools (`replace_in_file`, `write_project_file`). Unlike the patch preflight
/// path validator, this HARD-rejects sensitive path components (the task spec
/// for these tools says "拒绝敏感路径", not "warn"). Absolute paths, `..`
/// traversal, empty paths, NUL bytes, and sensitive components are all rejected
/// so the helper never touches secrets, version control, or build output.
pub(crate) fn validate_edit_file_path(path: &str) -> Result<(), String> {
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
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("path cannot contain parent traversal".to_string());
    }
    if is_sensitive_edit_path(path) {
        return Err(format!(
            "refusing sensitive path '{}': touches agent.toml, webcodex.env, \
             .env, projects.d, .git, target, or node_modules",
            path
        ));
    }
    Ok(())
}

/// Validate a project-relative binary artifact path. This is stricter than
/// source edit validation: in addition to build/VCS dirs it rejects secrets,
/// token paths, and private-key filenames.
pub(crate) fn validate_artifact_file_path(path: &str) -> Result<(), String> {
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
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("path cannot contain parent traversal".to_string());
    }
    if is_sensitive_artifact_path(path) {
        return Err(format!("refusing sensitive artifact path '{}'", path));
    }
    Ok(())
}

pub(crate) fn is_sensitive_artifact_path(path: &str) -> bool {
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

fn validate_artifact_mime(mime_type: Option<&str>) -> Result<Option<String>, String> {
    let Some(mime) = mime_type.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    match mime {
        "image/png"
        | "image/jpeg"
        | "image/webp"
        | "application/pdf"
        | "application/zip"
        | "text/plain"
        | "text/csv"
        | "application/json" => Ok(Some(mime.to_string())),
        "application/octet-stream" => Ok(Some(mime.to_string())),
        _ => Err(format!("unsupported mime_type '{}'; allowed first-pass artifact MIME types are image/png, image/jpeg, image/webp, application/pdf, application/zip, text/plain, text/csv, application/json", mime)),
    }
}

/// True if `path` contains a sensitive component for the structured edit
/// tools. Matching is component-wise (split on `/`) so legitimate filenames
/// that merely contain a sensitive substring (e.g. `targeting.md`) are NOT
/// rejected. A component is sensitive if it equals one of the guarded names or
/// starts with `.env` / `agent.toml` / `webcodex.env` (catching backups
/// like `.env.local` or `agent.toml.bak`).
pub(crate) fn is_sensitive_edit_path(path: &str) -> bool {
    for comp in path.to_lowercase().split('/') {
        if matches!(
            comp,
            ".git"
                | "target"
                | "node_modules"
                | "projects.d"
                | "agent.toml"
                | "webcodex.env"
                | ".env"
        ) {
            return true;
        }
        if comp.starts_with(".env")
            || comp.starts_with("agent.toml")
            || comp.starts_with("webcodex.env")
        {
            return true;
        }
    }
    false
}

/// True if `s` is a lowercase 64-character hex string (a sha256 digest).
pub(crate) fn is_hex_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

#[cfg(test)]
pub(crate) fn sha256_hex_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineEditOperation {
    Replace,
    Insert,
    Delete,
}

#[cfg(test)]
fn normalize_line_edit_text(text: &str) -> String {
    if text.is_empty() || text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{}\n", text)
    }
}

#[cfg(test)]
fn line_edit_text_line_count(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.lines().count()
    }
}

/// Apply a structured line edit to UTF-8 `content` and return the new content
/// plus the JSON payload shared by the runtime and tests. `new_sha256` is the
/// sha256 digest of the entire file after the operation. Range tools hash the
/// original replaced/deleted text; insert hashes the anchor line (or empty EOF
/// anchor).
#[cfg(test)]
pub(crate) fn apply_line_edit_content(
    content: &str,
    path: &str,
    op: LineEditOperation,
    start_line: Option<usize>,
    end_line: Option<usize>,
    line: Option<usize>,
    text: &str,
    expected_sha256: Option<&str>,
    expected_prefix: Option<&str>,
) -> Result<(String, Value), String> {
    let lines: Vec<&str> = if content.is_empty() {
        Vec::new()
    } else {
        content.split_inclusive('\n').collect()
    };
    let total_lines = lines.len();
    let (old_text, new_text, new_content, old_line_count, new_line_count, range) = match op {
        LineEditOperation::Replace | LineEditOperation::Delete => {
            let start = start_line.ok_or_else(|| "invalid line range".to_string())?;
            let end = end_line.ok_or_else(|| "invalid line range".to_string())?;
            if start == 0 || end < start || end > total_lines {
                return Err("invalid line range".to_string());
            }
            let old = lines[start - 1..end].concat();
            let replacement = if op == LineEditOperation::Delete {
                String::new()
            } else {
                normalize_line_edit_text(text)
            };
            let mut next = String::new();
            next.push_str(&lines[..start - 1].concat());
            next.push_str(&replacement);
            next.push_str(&lines[end..].concat());
            let inserted_lines = line_edit_text_line_count(&replacement);
            (
                old,
                replacement,
                next,
                end - start + 1,
                inserted_lines,
                Some((start, end)),
            )
        }
        LineEditOperation::Insert => {
            let at = line.ok_or_else(|| "line out of range".to_string())?;
            if at == 0 || at > total_lines + 1 {
                return Err("line out of range".to_string());
            }
            let anchor = if at <= total_lines {
                lines[at - 1].to_string()
            } else {
                String::new()
            };
            let insertion = normalize_line_edit_text(text);
            let mut next = String::new();
            next.push_str(&lines[..at - 1].concat());
            next.push_str(&insertion);
            next.push_str(&lines[at - 1..].concat());
            let inserted_lines = line_edit_text_line_count(&insertion);
            let anchor_count = if at <= total_lines { 1 } else { 0 };
            (anchor, insertion, next, anchor_count, inserted_lines, None)
        }
    };
    let old_sha256 = sha256_hex_bytes(old_text.as_bytes());
    if let Some(expected) = expected_sha256 {
        if old_sha256 != expected {
            let label = match op {
                LineEditOperation::Insert => "expected_anchor_sha256 mismatch",
                _ => "expected_old_sha256 mismatch",
            };
            return Err(recoverable_write_rejection(label));
        }
    }
    if let Some(prefix) = expected_prefix {
        if !old_text.starts_with(prefix) {
            let label = match op {
                LineEditOperation::Insert => "expected_anchor_prefix mismatch",
                _ => "expected_old_prefix mismatch",
            };
            return Err(recoverable_write_rejection(label));
        }
    }
    let new_sha256 = sha256_hex_bytes(new_content.as_bytes());
    let mut output = json!({
        "path": path,
        "old_sha256": old_sha256,
        "new_sha256": new_sha256,
        "old_line_count": old_line_count,
        "new_line_count": new_line_count,
        "bytes_written": new_content.len(),
        "changed": new_content != content,
    });
    if let Some((start, end)) = range {
        output["start_line"] = json!(start);
        output["end_line"] = json!(end);
    } else if let Some(at) = line {
        output["line"] = json!(at);
    }
    let _ = new_text;
    Ok((new_content, output))
}

/// Fixed python3 helper run on the owning agent for `replace_in_file`.
///
/// The script is wrapped in single quotes (`python3 -c '<script>'`), so it
/// MUST NOT contain single quotes — all Python string literals use double
/// quotes. `old`/`new`/`path` arrive over stdin as JSON (never interpolated
/// into the command). The helper counts occurrences, refuses to write on a
/// missing or ambiguous match, and writes atomically via tempfile + os.replace
/// in the file's directory. It always prints exactly one JSON object on stdout
/// and exits 0 (logical failures carry an `error` field in that object).
pub(crate) const REPLACE_IN_FILE_HELPER: &str = r#"
import sys, json, hashlib, os, tempfile
NUL = "\x00"
def emit(obj):
    sys.stdout.write(json.dumps(obj))
    sys.exit(0)
try:
    req = json.load(sys.stdin)
except Exception as e:
    emit({"changed": False, "error": "invalid json: " + str(e)})
path = req.get("path", "")
old = req.get("old", "")
new = req.get("new", "")
expected = req.get("expected_replacements", 1)
allow_multi = bool(req.get("allow_multiple", False))
if not isinstance(path, str) or not path or path.startswith("/") or NUL in path or ".." in path.split("/"):
    emit({"changed": False, "error": "invalid path"})
if not isinstance(old, str) or old == "" or NUL in old:
    emit({"changed": False, "error": "old must be a non-empty string without NUL"})
if not isinstance(new, str) or NUL in new:
    emit({"changed": False, "error": "new must be a string without NUL"})
try:
    expected = int(expected)
except Exception:
    emit({"changed": False, "error": "expected_replacements must be an integer"})
if expected < 1:
    emit({"changed": False, "error": "expected_replacements must be >= 1"})
if not allow_multi and expected != 1:
    emit({"changed": False, "error": "expected_replacements must be 1 when allow_multiple is false"})
try:
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
except FileNotFoundError:
    emit({"changed": False, "error": "file not found", "path": path})
except UnicodeDecodeError:
    emit({"changed": False, "error": "file is not valid UTF-8", "path": path})
except Exception as e:
    emit({"changed": False, "error": "read failed: " + str(e)})
before = hashlib.sha256(content.encode("utf-8")).hexdigest()
count = content.count(old)
if count == 0:
    emit({"changed": False, "path": path, "before_sha256": before, "occurrences": 0, "error": "old not found in file"})
if count > 1 and not allow_multi:
    emit({"changed": False, "path": path, "before_sha256": before, "occurrences": count, "error": "old appears multiple times and allow_multiple is false"})
if allow_multi:
    if count != expected:
        emit({"changed": False, "path": path, "before_sha256": before, "occurrences": count, "expected": expected, "error": "expected_replacements mismatch"})
    reps = expected
    replaced = content.replace(old, new, expected)
else:
    reps = 1
    replaced = content.replace(old, new, 1)
after_bytes = len(replaced.encode("utf-8"))
base_dir = os.path.dirname(path) or "."
tmp = None
try:
    os.makedirs(base_dir, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=base_dir, prefix=".pd-rep-")
    with os.fdopen(fd, "w", encoding="utf-8") as f:
        f.write(replaced)
    os.replace(tmp, path)
except Exception as e:
    if tmp is not None:
        try:
            os.remove(tmp)
        except OSError:
            pass
    emit({"changed": False, "path": path, "before_sha256": before, "error": "write failed: " + str(e)})
after = hashlib.sha256(replaced.encode("utf-8")).hexdigest()
emit({"changed": True, "path": path, "replacements": reps, "before_sha256": before, "after_sha256": after, "bytes_written": after_bytes})
"#;

/// Fixed python3 helper run on the owning agent for `write_project_file`.
///
/// Same single-quote wrapping rules as `REPLACE_IN_FILE_HELPER` (no single
/// quotes inside). Enforces create-vs-overwrite semantics with optional
/// `expected_sha256` / `expected_content_prefix` guards and writes atomically.
/// Always prints exactly one JSON object on stdout and exits 0.
pub(crate) const WRITE_PROJECT_FILE_HELPER: &str = r#"
import sys, json, hashlib, os, tempfile
NUL = "\x00"
def emit(obj):
    sys.stdout.write(json.dumps(obj))
    sys.exit(0)
try:
    req = json.load(sys.stdin)
except Exception as e:
    emit({"path": None, "created": False, "overwritten": False, "bytes_written": 0, "sha256": None, "warning": None, "error": "invalid json: " + str(e)})
path = req.get("path", "")
content = req.get("content", "")
overwrite = bool(req.get("overwrite", False))
exp_sha = req.get("expected_sha256", None)
exp_prefix = req.get("expected_content_prefix", None)
if not isinstance(path, str) or not path or path.startswith("/") or NUL in path or ".." in path.split("/"):
    emit({"path": path if isinstance(path, str) else None, "created": False, "overwritten": False, "bytes_written": 0, "sha256": None, "warning": None, "error": "invalid path"})
if not isinstance(content, str) or NUL in content:
    emit({"path": path, "created": False, "overwritten": False, "bytes_written": 0, "sha256": None, "warning": None, "error": "content must be a UTF-8 string without NUL"})
exists = os.path.lexists(path)
warning = None
if exists and not overwrite:
    emit({"path": path, "created": False, "overwritten": False, "bytes_written": 0, "sha256": None, "warning": None, "error": "file exists and overwrite is false"})
if exists and overwrite:
    try:
        with open(path, "r", encoding="utf-8") as f:
            current = f.read()
    except UnicodeDecodeError:
        emit({"path": path, "created": False, "overwritten": False, "bytes_written": 0, "sha256": None, "warning": None, "error": "existing file is not valid UTF-8"})
    except Exception as e:
        emit({"path": path, "created": False, "overwritten": False, "bytes_written": 0, "sha256": None, "warning": None, "error": "read failed: " + str(e)})
    if exp_sha is not None:
        cur_sha = hashlib.sha256(current.encode("utf-8")).hexdigest()
        if cur_sha != exp_sha:
            emit({"path": path, "created": False, "overwritten": False, "bytes_written": 0, "sha256": cur_sha, "warning": None, "error": "expected_sha256 mismatch"})
    if exp_prefix is not None:
        if not isinstance(exp_prefix, str) or not current.startswith(exp_prefix):
            emit({"path": path, "created": False, "overwritten": False, "bytes_written": 0, "sha256": None, "warning": None, "error": "expected_content_prefix mismatch"})
    if exp_sha is None and exp_prefix is None:
        warning = "overwrite without expected_sha256 or expected_content_prefix; provide expected_sha256 for safer overwrites"
base_dir = os.path.dirname(path) or "."
written_bytes = len(content.encode("utf-8"))
tmp = None
try:
    os.makedirs(base_dir, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=base_dir, prefix=".pd-write-")
    with os.fdopen(fd, "w", encoding="utf-8") as f:
        f.write(content)
    os.replace(tmp, path)
except Exception as e:
    if tmp is not None:
        try:
            os.remove(tmp)
        except OSError:
            pass
    emit({"path": path, "created": False, "overwritten": False, "bytes_written": 0, "sha256": None, "warning": None, "error": "write failed: " + str(e)})
sha = hashlib.sha256(content.encode("utf-8")).hexdigest()
emit({"path": path, "created": not exists, "overwritten": exists, "bytes_written": written_bytes, "sha256": sha, "warning": warning})
"#;

/// Fixed python3 helper for binary artifact writes. Payload carries base64 over
/// stdin; helper decodes and writes bytes atomically on the owning agent.
pub(crate) const SAVE_PROJECT_ARTIFACT_HELPER: &str = r#"
import sys, json, hashlib, os, tempfile, base64
NUL = "\x00"
def emit(obj):
    sys.stdout.write(json.dumps(obj))
    sys.exit(0)
def invalid(path, msg):
    emit({"path": path if isinstance(path, str) else None, "bytes_written": 0, "sha256": None, "mime_type": None, "error": msg})
try:
    req = json.load(sys.stdin)
except Exception as e:
    emit({"path": None, "bytes_written": 0, "sha256": None, "mime_type": None, "error": "invalid json: " + str(e)})
path = req.get("path", "")
content_base64 = req.get("content_base64", "")
mime_type = req.get("mime_type", None)
overwrite = bool(req.get("overwrite", False))
max_bytes = int(req.get("max_bytes", 10485760))
if not isinstance(path, str) or not path or path.startswith("/") or NUL in path or ".." in path.split("/"):
    invalid(path, "invalid path")
if not isinstance(content_base64, str) or NUL in content_base64:
    invalid(path, "content_base64 must be a base64 string without NUL")
try:
    data = base64.b64decode(content_base64, validate=True)
except Exception as e:
    invalid(path, "invalid base64: " + str(e))
if len(data) > max_bytes:
    invalid(path, "decoded artifact too large")
exists = os.path.lexists(path)
if exists and not overwrite:
    invalid(path, "file exists and overwrite is false")
if exists and os.path.islink(path):
    invalid(path, "refusing to overwrite symlink artifact path")
base_dir = os.path.dirname(path) or "."
tmp = None
try:
    os.makedirs(base_dir, exist_ok=True)
    root = os.path.realpath(os.getcwd())
    parent = os.path.realpath(base_dir)
    if parent != root and not parent.startswith(root + os.sep):
        invalid(path, "artifact path escapes project root")
    fd, tmp = tempfile.mkstemp(dir=base_dir, prefix=".pd-artifact-")
    with os.fdopen(fd, "wb") as f:
        f.write(data)
    if os.path.islink(path):
        try:
            os.remove(tmp)
        except OSError:
            pass
        invalid(path, "refusing to overwrite symlink artifact path")
    os.replace(tmp, path)
except Exception as e:
    if tmp is not None:
        try:
            os.remove(tmp)
        except OSError:
            pass
    invalid(path, "write failed: " + str(e))
sha = hashlib.sha256(data).hexdigest()
emit({"path": path, "bytes_written": len(data), "sha256": sha, "mime_type": mime_type})
"#;

/// Fixed python3 helper for artifact metadata. Reads bytes only to compute
/// bounded metadata; zip files are counted but never extracted.
pub(crate) const READ_PROJECT_ARTIFACT_METADATA_HELPER: &str = r#"
import sys, json, hashlib, os, mimetypes, zipfile, io, struct
def emit(obj):
    sys.stdout.write(json.dumps(obj))
    sys.exit(0)
def fail(path, msg):
    emit({"path": path if isinstance(path, str) else None, "bytes": 0, "sha256": None, "mime_type": None, "error": msg})
def png_size(data):
    if len(data) >= 24 and data[:8] == b"\x89PNG\r\n\x1a\n":
        return struct.unpack(">II", data[16:24])
    return None
def webp_size(data):
    if len(data) >= 30 and data[:4] == b"RIFF" and data[8:12] == b"WEBP" and data[12:16] == b"VP8X":
        w = 1 + int.from_bytes(data[24:27], "little")
        h = 1 + int.from_bytes(data[27:30], "little")
        return (w, h)
    return None
def jpeg_size(data):
    if len(data) < 4 or data[:2] != b"\xff\xd8":
        return None
    i = 2
    while i + 9 < len(data):
        if data[i] != 0xFF:
            i += 1
            continue
        marker = data[i+1]
        i += 2
        if marker in (0xC0,0xC1,0xC2,0xC3,0xC5,0xC6,0xC7,0xC9,0xCA,0xCB,0xCD,0xCE,0xCF):
            return (int.from_bytes(data[i+5:i+7], "big"), int.from_bytes(data[i+3:i+5], "big"))
        if i + 2 > len(data):
            break
        seg = int.from_bytes(data[i:i+2], "big")
        if seg < 2:
            break
        i += seg
    return None
try:
    req = json.load(sys.stdin)
except Exception as e:
    emit({"path": None, "bytes": 0, "sha256": None, "mime_type": None, "error": "invalid json: " + str(e)})
path = req.get("path", "")
max_bytes = int(req.get("max_bytes", 10485760))
if not isinstance(path, str) or not path or path.startswith("/") or ".." in path.split("/"):
    fail(path, "invalid path")
root = os.path.realpath(os.getcwd())
target = os.path.realpath(path)
if target != root and not target.startswith(root + os.sep):
    fail(path, "artifact path escapes project root")
try:
    with open(path, "rb") as f:
        data = f.read(max_bytes + 1)
except Exception as e:
    fail(path, "read failed: " + str(e))
if len(data) > max_bytes:
    fail(path, "artifact too large to inspect")
sha = hashlib.sha256(data).hexdigest()
mime = mimetypes.guess_type(path)[0]
if data.startswith(b"\x89PNG\r\n\x1a\n"):
    mime = "image/png"
elif data.startswith(b"\xff\xd8"):
    mime = "image/jpeg"
elif data[:4] == b"RIFF" and data[8:12] == b"WEBP":
    mime = "image/webp"
elif data.startswith(b"%PDF-"):
    mime = "application/pdf"
elif data.startswith(b"PK\x03\x04") or data.startswith(b"PK\x05\x06"):
    mime = "application/zip"
out = {"path": path, "bytes": len(data), "sha256": sha, "mime_type": mime}
size = png_size(data) or jpeg_size(data) or webp_size(data)
if size:
    out["width"], out["height"] = size
if mime == "application/zip":
    try:
        out["archive_entries_count"] = len(zipfile.ZipFile(io.BytesIO(data)).infolist())
    except Exception:
        out["archive_entries_count"] = None
emit(out)
"#;

/// Fixed python3 helper for artifact content reads. Reads a bounded binary
/// artifact and returns one base64-encoded content segment plus full-file
/// sha256/MIME metadata. This is a chunked read helper, not a large-file
/// transfer mechanism.
pub(crate) const READ_PROJECT_ARTIFACT_HELPER: &str = r#"
import sys, json, hashlib, os, mimetypes, base64
def emit(obj):
    sys.stdout.write(json.dumps(obj))
    sys.exit(0)
def fail(path, msg):
    emit({"path": path if isinstance(path, str) else None, "mime_type": None, "file_bytes": 0, "sha256": None, "offset": 0, "bytes_returned": 0, "content_base64": "", "next_offset": 0, "truncated": False, "error": msg})
try:
    req = json.load(sys.stdin)
except Exception as e:
    emit({"path": None, "mime_type": None, "file_bytes": 0, "sha256": None, "offset": 0, "bytes_returned": 0, "content_base64": "", "next_offset": 0, "truncated": False, "error": "invalid json: " + str(e)})
path = req.get("path", "")
try:
    offset = int(req.get("offset", 0))
    length = int(req.get("length", 32768))
    max_file_bytes = int(req.get("max_file_bytes", 10485760))
except Exception:
    fail(path, "offset, length, and max_file_bytes must be integers")
if offset < 0:
    fail(path, "offset must be >= 0")
if length < 1:
    fail(path, "length must be >= 1")
if max_file_bytes < 1:
    fail(path, "max_file_bytes must be >= 1")
if not isinstance(path, str) or not path or path.startswith("/") or "\x00" in path or ".." in path.split("/"):
    fail(path, "invalid path")
root = os.path.realpath(os.getcwd())
target = os.path.realpath(path)
if target != root and not target.startswith(root + os.sep):
    fail(path, "artifact path escapes project root")
try:
    file_bytes = os.path.getsize(path)
except Exception as e:
    fail(path, "stat failed: " + str(e))
if file_bytes > max_file_bytes:
    fail(path, "artifact too large to read; use metadata or a smaller artifact")
try:
    with open(path, "rb") as f:
        data = f.read()
except Exception as e:
    fail(path, "read failed: " + str(e))
sha = hashlib.sha256(data).hexdigest()
mime = mimetypes.guess_type(path)[0]
if data.startswith(b"\x89PNG\r\n\x1a\n"):
    mime = "image/png"
elif data.startswith(b"\xff\xd8"):
    mime = "image/jpeg"
elif data[:4] == b"RIFF" and data[8:12] == b"WEBP":
    mime = "image/webp"
elif data.startswith(b"%PDF-"):
    mime = "application/pdf"
elif data.startswith(b"PK\x03\x04") or data.startswith(b"PK\x05\x06"):
    mime = "application/zip"
elif data.lstrip()[:1] in (b"{", b"["):
    mime = "application/json"
if offset >= file_bytes:
    segment = b""
    next_offset = file_bytes
    truncated = False
else:
    next_offset = min(file_bytes, offset + length)
    segment = data[offset:next_offset]
    truncated = next_offset < file_bytes
emit({"path": path, "mime_type": mime, "file_bytes": file_bytes, "sha256": sha, "offset": offset, "bytes_returned": len(segment), "content_base64": base64.b64encode(segment).decode("ascii"), "next_offset": next_offset, "truncated": truncated})
"#;

impl ToolRuntime {
    pub(crate) async fn delete_project_files(
        &self,
        project: String,
        paths: Vec<String>,
    ) -> ToolResult {
        let paths = match validate_limited_cleanup_paths(&paths, true) {
            Ok(paths) => paths,
            Err(e) => return ToolResult::err(e),
        };
        let command = format!("rm -f -- {}", shell_join_paths(&paths));
        let result = self.run_shell(project, command, Some(30), None).await;
        if result.success {
            ToolResult::ok(json!({
                "deleted_paths": paths,
                "command_result": result.output,
            }))
        } else {
            result
        }
    }

    // -------------------------------------------------------------------------
    // Phase 4: structured file edit tools (replace_in_file / write_project_file)
    // -------------------------------------------------------------------------
    //
    // Both tools mutate the worktree through the owning agent only. The server
    // never reads or writes the agent project filesystem directly. Instead a
    // FIXED python3 helper is sent as the shell `command` and the tool
    // arguments (path/old/new/content/...) travel as a JSON document over the
    // process stdin. The command string is a compile-time constant — no caller
    // content is ever interpolated into it — so there is no shell-injection
    // surface. The helper performs all validation + the atomic write on the
    // agent side and prints a single JSON line on stdout.

    pub(crate) async fn replace_in_file(
        &self,
        project: String,
        path: String,
        old: String,
        new: String,
        expected_replacements: Option<i64>,
        allow_multiple: Option<bool>,
    ) -> ToolResult {
        // ---- Input validation (before project resolution) ----
        if let Err(e) = validate_edit_file_path(&path) {
            return ToolResult::err(e);
        }
        if old.is_empty() {
            return ToolResult::err("old must be non-empty");
        }
        if old.contains('\0') || new.contains('\0') {
            return ToolResult::err("old and new cannot contain NUL bytes");
        }
        if old.len() > MAX_REPLACE_FIELD_BYTES || new.len() > MAX_REPLACE_FIELD_BYTES {
            return ToolResult::err(format!(
                "old/new too large; maximum is {} bytes each",
                MAX_REPLACE_FIELD_BYTES
            ));
        }
        let expected = expected_replacements.unwrap_or(1);
        if expected < 1 {
            return ToolResult::err("expected_replacements must be >= 1");
        }
        let allow_multi = allow_multiple.unwrap_or(false);
        if !allow_multi && expected != 1 {
            return ToolResult::err("expected_replacements must be 1 when allow_multiple is false");
        }

        // ---- Project resolution (agent-registered only) ----
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err(
                "replace_in_file requires an agent-registered project; \
                 server-configured projects are not supported",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };

        let payload = json!({
            "path": path,
            "old": old,
            "new": new,
            "expected_replacements": expected,
            "allow_multiple": allow_multi,
        });
        let command = format!("python3 -c '{}'", REPLACE_IN_FILE_HELPER);
        let obj = match self
            .run_agent_helper(client_id, proj.path.clone(), command, payload)
            .await
        {
            Ok(v) => v,
            Err(e) => return ToolResult::err(recoverable_write_rejection(e)),
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(recoverable_write_rejection(err)),
            };
        }
        ToolResult::ok(obj)
    }

    pub(crate) async fn write_project_file(
        &self,
        project: String,
        path: String,
        content: String,
        overwrite: Option<bool>,
        expected_sha256: Option<String>,
        expected_content_prefix: Option<String>,
    ) -> ToolResult {
        // ---- Input validation (before project resolution) ----
        if let Err(e) = validate_edit_file_path(&path) {
            return ToolResult::err(e);
        }
        if content.contains('\0') {
            return ToolResult::err("content cannot contain NUL bytes");
        }
        if content.len() > MAX_WRITE_CONTENT_BYTES {
            return ToolResult::err(format!(
                "content too large; maximum is {} bytes",
                MAX_WRITE_CONTENT_BYTES
            ));
        }
        if let Some(hash) = expected_sha256.as_deref() {
            if !is_hex_sha256(hash) {
                return ToolResult::err(
                    "expected_sha256 must be a lowercase 64-char hex sha256 digest",
                );
            }
        }

        // ---- Project resolution (agent-registered only) ----
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err(
                "write_project_file requires an agent-registered project; \
                 server-configured projects are not supported",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };

        let payload = json!({
            "path": path,
            "content": content,
            "overwrite": overwrite.unwrap_or(false),
            "expected_sha256": expected_sha256,
            "expected_content_prefix": expected_content_prefix,
        });
        let command = format!("python3 -c '{}'", WRITE_PROJECT_FILE_HELPER);
        let obj = match self
            .run_agent_helper(client_id, proj.path.clone(), command, payload)
            .await
        {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(err),
            };
        }
        ToolResult::ok(obj)
    }

    pub(crate) async fn save_project_artifact(
        &self,
        project: String,
        path: String,
        content_base64: String,
        mime_type: Option<String>,
        overwrite: Option<bool>,
    ) -> ToolResult {
        if let Err(e) = validate_artifact_file_path(&path) {
            return ToolResult::err(e);
        }
        if content_base64.len() > MAX_PROJECT_ARTIFACT_BASE64_BYTES {
            return ToolResult::err(format!(
                "content_base64 too large; maximum encoded size is {} bytes",
                MAX_PROJECT_ARTIFACT_BASE64_BYTES
            ));
        }
        let mime_type = match validate_artifact_mime(mime_type.as_deref()) {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };
        let decoded = match general_purpose::STANDARD.decode(content_base64.as_bytes()) {
            Ok(bytes) => bytes,
            Err(e) => return ToolResult::err(format!("invalid base64: {}", e)),
        };
        if decoded.len() > MAX_PROJECT_ARTIFACT_BYTES {
            return ToolResult::err(format!(
                "decoded artifact too large; maximum is {} bytes",
                MAX_PROJECT_ARTIFACT_BYTES
            ));
        }
        if matches!(mime_type.as_deref(), Some("application/octet-stream")) {
            let lower = path.to_lowercase();
            let allowed = [
                ".png", ".jpg", ".jpeg", ".webp", ".pdf", ".zip", ".txt", ".csv", ".json",
            ];
            if !allowed.iter().any(|suffix| lower.ends_with(suffix)) {
                return ToolResult::err(
                    "application/octet-stream requires a safe artifact extension".to_string(),
                );
            }
        }

        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err("save_project_artifact requires an agent-registered project");
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };

        let payload = json!({
            "path": path,
            "content_base64": content_base64,
            "mime_type": mime_type,
            "overwrite": overwrite.unwrap_or(false),
            "max_bytes": MAX_PROJECT_ARTIFACT_BYTES,
        });
        let command = format!("python3 -c '{}'", SAVE_PROJECT_ARTIFACT_HELPER);
        let obj = match self
            .run_agent_helper(client_id, proj.path.clone(), command, payload)
            .await
        {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(err),
            };
        }
        ToolResult::ok(obj)
    }

    pub(crate) async fn read_project_artifact_metadata(
        &self,
        project: String,
        path: String,
    ) -> ToolResult {
        if let Err(e) = validate_artifact_file_path(&path) {
            return ToolResult::err(e);
        }
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err(
                "read_project_artifact_metadata requires an agent-registered project",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };
        let payload = json!({
            "path": path,
            "max_bytes": MAX_PROJECT_ARTIFACT_BYTES,
        });
        let command = format!("python3 -c '{}'", READ_PROJECT_ARTIFACT_METADATA_HELPER);
        let obj = match self
            .run_agent_helper(client_id, proj.path.clone(), command, payload)
            .await
        {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(err),
            };
        }
        ToolResult::ok(obj)
    }

    pub(crate) async fn read_project_artifact(
        &self,
        project: String,
        path: String,
        encoding: Option<String>,
        offset: Option<usize>,
        length: Option<usize>,
        max_bytes: Option<usize>,
    ) -> ToolResult {
        if let Err(e) = validate_artifact_file_path(&path) {
            return ToolResult::err(e);
        }
        let encoding = encoding.unwrap_or_else(|| "base64".to_string());
        if encoding != "base64" {
            return ToolResult::err("unsupported encoding; only 'base64' is currently supported");
        }
        let offset = offset.unwrap_or(0);
        let mut length =
            length.unwrap_or_else(|| max_bytes.unwrap_or(DEFAULT_READ_PROJECT_ARTIFACT_LENGTH));
        if let Some(max_bytes) = max_bytes {
            if max_bytes == 0 {
                return ToolResult::err("max_bytes must be at least 1");
            }
            length = length.min(max_bytes);
        }
        if length == 0 {
            return ToolResult::err("length must be at least 1");
        }
        if length > MAX_READ_PROJECT_ARTIFACT_LENGTH {
            return ToolResult::err(format!(
                "length too large; maximum is {} bytes",
                MAX_READ_PROJECT_ARTIFACT_LENGTH
            ));
        }
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err("read_project_artifact requires an agent-registered project");
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };
        let payload = json!({
            "path": path,
            "offset": offset,
            "length": length,
            "max_file_bytes": MAX_PROJECT_ARTIFACT_BYTES,
        });
        let command = format!("python3 -c '{}'", READ_PROJECT_ARTIFACT_HELPER);
        let obj = match self
            .run_agent_helper(client_id, proj.path.clone(), command, payload)
            .await
        {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(err),
            };
        }
        ToolResult::ok(obj)
    }

    fn validate_line_edit_common(
        path: &str,
        text: &str,
        expected_sha256: Option<&str>,
        expected_prefix: Option<&str>,
    ) -> Result<(), String> {
        validate_edit_file_path(path)?;
        if text.contains('\0') {
            return Err("text cannot contain NUL bytes".to_string());
        }
        if text.len() > MAX_WRITE_CONTENT_BYTES {
            return Err(format!(
                "text too large; maximum is {} bytes",
                MAX_WRITE_CONTENT_BYTES
            ));
        }
        if let Some(hash) = expected_sha256 {
            if !is_hex_sha256(hash) {
                return Err(
                    "expected sha256 must be a lowercase 64-char hex sha256 digest".to_string(),
                );
            }
        }
        if let Some(prefix) = expected_prefix {
            if prefix.contains('\0') {
                return Err("expected prefix cannot contain NUL bytes".to_string());
            }
            if prefix.len() > MAX_EXPECTED_PREFIX_BYTES {
                return Err(format!(
                    "expected prefix too large; maximum is {} bytes",
                    MAX_EXPECTED_PREFIX_BYTES
                ));
            }
        }
        Ok(())
    }

    async fn run_line_edit(
        &self,
        project: String,
        path: String,
        op: &str,
        content: Option<String>,
        start_line: Option<usize>,
        end_line: Option<usize>,
        line: Option<usize>,
        expected_sha256: Option<String>,
        expected_prefix: Option<String>,
    ) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err(
                "line edit tools require an agent-registered project; \
                 server-configured projects are not supported",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };
        let wait_timeout = 60_u64;
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_file_op(
                ShellFileOpRequest {
                    op: op.to_string(),
                    client_id,
                    path: path.clone(),
                    cwd: Some(proj.path.clone()),
                    content,
                    max_bytes: None,
                    old_text: None,
                    pattern: None,
                    expected_sha256,
                    expected_prefix,
                    start_line,
                    end_line,
                    line,
                    create_dirs: false,
                    wait_timeout_secs: wait_timeout,
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(recoverable_write_rejection(e)),
        };
        let resp = match tokio::time::timeout(Duration::from_secs(wait_timeout + 4), rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&request_id).await;
                return ToolResult::err("agent line edit request was dropped");
            }
            Err(_) => {
                self.shell_clients.cancel_request(&request_id).await;
                return ToolResult::err("timed out waiting for agent line edit");
            }
        };
        if let Some(e) = resp.error {
            return ToolResult::err(recoverable_write_rejection(e));
        }
        if resp.exit_code != Some(0) {
            return ToolResult::err(recoverable_write_rejection(resp.stderr.unwrap_or_else(
                || format!("agent line edit failed with code {:?}", resp.exit_code),
            )));
        }
        let stdout = resp.stdout.unwrap_or_default();
        let stdout = stdout.trim();
        let obj: Value = match serde_json::from_str(stdout) {
            Ok(v) => v,
            Err(e) => {
                return ToolResult::err(format!(
                    "agent line edit returned invalid JSON: {} (got: {})",
                    e,
                    &stdout[..stdout.len().min(200)]
                ))
            }
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(recoverable_write_rejection(err)),
            };
        }
        if obj.get("path").is_none() {
            let mut obj = obj;
            obj["path"] = json!(path);
            return ToolResult::ok(obj);
        }
        ToolResult::ok(obj)
    }

    fn validate_anchor_edit_common(path: &str, text: &str) -> Result<(), String> {
        validate_edit_file_path(path)?;
        if text.contains('\0') {
            return Err("text cannot contain NUL bytes".to_string());
        }
        if text.len() > MAX_WRITE_CONTENT_BYTES {
            return Err(format!(
                "text too large; maximum is {} bytes",
                MAX_WRITE_CONTENT_BYTES
            ));
        }
        Ok(())
    }

    fn parse_anchor_edit_stdout(op: &str, stdout: Option<String>) -> Result<Value, String> {
        let stdout = stdout.unwrap_or_default();
        let stdout = stdout.trim();
        if stdout.is_empty() {
            return Err(format!(
                "agent anchor edit returned empty stdout for {op}; connected agent may not support this file op or transport dispatch may have routed it incorrectly"
            ));
        }
        serde_json::from_str(stdout).map_err(|e| {
            format!(
                "agent anchor edit returned invalid JSON: {} (got: {})",
                e,
                &stdout[..stdout.len().min(200)]
            )
        })
    }

    async fn run_anchor_edit(
        &self,
        project: String,
        path: String,
        op: &str,
        old_text: Option<String>,
        pattern: Option<String>,
        content: String,
        expected_sha256: Option<String>,
    ) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err(
                "anchor edit tools require an agent-registered project; \
                 server-configured projects are not supported",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };
        let wait_timeout = 60_u64;
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_file_op(
                ShellFileOpRequest {
                    op: op.to_string(),
                    client_id,
                    path: path.clone(),
                    cwd: Some(proj.path.clone()),
                    content: Some(content),
                    max_bytes: None,
                    old_text,
                    pattern,
                    expected_sha256,
                    expected_prefix: None,
                    start_line: None,
                    end_line: None,
                    line: None,
                    create_dirs: false,
                    wait_timeout_secs: wait_timeout,
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(recoverable_write_rejection(e)),
        };
        let resp = match tokio::time::timeout(Duration::from_secs(wait_timeout + 4), rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&request_id).await;
                return ToolResult::err("agent anchor edit request was dropped");
            }
            Err(_) => {
                self.shell_clients.cancel_request(&request_id).await;
                return ToolResult::err("timed out waiting for agent anchor edit");
            }
        };
        if let Some(e) = resp.error {
            return ToolResult::err(recoverable_write_rejection(e));
        }
        if resp.exit_code != Some(0) {
            return ToolResult::err(recoverable_write_rejection(resp.stderr.unwrap_or_else(
                || format!("agent anchor edit failed with code {:?}", resp.exit_code),
            )));
        }
        let obj = match Self::parse_anchor_edit_stdout(op, resp.stdout) {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(err),
            };
        }
        ToolResult::ok(obj)
    }

    pub(crate) async fn replace_exact_block(
        &self,
        project: String,
        path: String,
        old_text: String,
        new_text: String,
        expected_old_sha256: Option<String>,
    ) -> ToolResult {
        if let Err(e) = Self::validate_anchor_edit_common(&path, &new_text) {
            return ToolResult::err(e);
        }
        if old_text.is_empty() {
            return ToolResult::err("old_text must be non-empty");
        }
        if old_text.contains('\0') {
            return ToolResult::err("old_text cannot contain NUL bytes");
        }
        if old_text.len() > MAX_REPLACE_FIELD_BYTES {
            return ToolResult::err(format!(
                "old_text too large; maximum is {} bytes",
                MAX_REPLACE_FIELD_BYTES
            ));
        }
        if let Some(hash) = expected_old_sha256.as_deref() {
            if !is_hex_sha256(hash) {
                return ToolResult::err(
                    "expected_old_sha256 must be a lowercase 64-char hex sha256 digest",
                );
            }
        }
        self.run_anchor_edit(
            project,
            path,
            "replace_exact_block",
            Some(old_text),
            None,
            new_text,
            expected_old_sha256,
        )
        .await
    }

    pub(crate) async fn insert_around_pattern(
        &self,
        project: String,
        path: String,
        pattern: String,
        text: String,
        op: &str,
    ) -> ToolResult {
        if let Err(e) = Self::validate_anchor_edit_common(&path, &text) {
            return ToolResult::err(e);
        }
        if pattern.is_empty() {
            return ToolResult::err("pattern must be non-empty literal pattern");
        }
        if text.is_empty() {
            return ToolResult::err("Rejected before write: inserted text must not be empty.\nNo files were modified.\nRetry guidance: provide the exact text to insert, including any intended newlines.");
        }
        if pattern.contains('\0') {
            return ToolResult::err("pattern cannot contain NUL bytes");
        }
        if pattern.len() > MAX_REPLACE_FIELD_BYTES {
            return ToolResult::err(format!(
                "pattern too large; maximum is {} bytes",
                MAX_REPLACE_FIELD_BYTES
            ));
        }
        self.run_anchor_edit(project, path, op, None, Some(pattern), text, None)
            .await
    }
    pub(crate) async fn replace_line_range(
        &self,
        project: String,
        path: String,
        start_line: usize,
        end_line: usize,
        new_text: String,
        expected_old_sha256: Option<String>,
        expected_old_prefix: Option<String>,
    ) -> ToolResult {
        if start_line == 0 || end_line < start_line {
            return ToolResult::err("invalid line range");
        }
        if let Err(e) = Self::validate_line_edit_common(
            &path,
            &new_text,
            expected_old_sha256.as_deref(),
            expected_old_prefix.as_deref(),
        ) {
            return ToolResult::err(e);
        }
        self.run_line_edit(
            project,
            path,
            "replace_line_range",
            Some(new_text),
            Some(start_line),
            Some(end_line),
            None,
            expected_old_sha256,
            expected_old_prefix,
        )
        .await
    }

    pub(crate) async fn insert_at_line(
        &self,
        project: String,
        path: String,
        line: usize,
        text: String,
        expected_anchor_sha256: Option<String>,
        expected_anchor_prefix: Option<String>,
    ) -> ToolResult {
        if line == 0 {
            return ToolResult::err("line out of range");
        }
        if let Err(e) = Self::validate_line_edit_common(
            &path,
            &text,
            expected_anchor_sha256.as_deref(),
            expected_anchor_prefix.as_deref(),
        ) {
            return ToolResult::err(e);
        }
        self.run_line_edit(
            project,
            path,
            "insert_at_line",
            Some(text),
            None,
            None,
            Some(line),
            expected_anchor_sha256,
            expected_anchor_prefix,
        )
        .await
    }

    pub(crate) async fn delete_line_range(
        &self,
        project: String,
        path: String,
        start_line: usize,
        end_line: usize,
        expected_old_sha256: Option<String>,
        expected_old_prefix: Option<String>,
    ) -> ToolResult {
        if start_line == 0 || end_line < start_line {
            return ToolResult::err("invalid line range");
        }
        if let Err(e) = Self::validate_line_edit_common(
            &path,
            "",
            expected_old_sha256.as_deref(),
            expected_old_prefix.as_deref(),
        ) {
            return ToolResult::err(e);
        }
        self.run_line_edit(
            project,
            path,
            "delete_line_range",
            None,
            Some(start_line),
            Some(end_line),
            None,
            expected_old_sha256,
            expected_old_prefix,
        )
        .await
    }

    /// Run a fixed agent-side helper `command` with a JSON `payload` on stdin
    /// and return the parsed JSON object the helper prints on stdout. Shared by
    /// `replace_in_file` and `write_project_file` so the enqueue/timeout/error
    /// handling stays in one place. The command is always a compile-time
    /// constant supplied by the caller; only the JSON payload varies.
    pub(crate) async fn run_agent_helper(
        &self,
        client_id: String,
        cwd: String,
        command: String,
        payload: Value,
    ) -> Result<Value, String> {
        let stdin = serde_json::to_string(&payload)
            .map_err(|e| format!("failed to serialize helper payload: {}", e))?;
        if stdin.len() > RUN_HELPER_STDIN_BUDGET {
            return Err(format!(
                "helper payload too large for the agent stdin transport ({} bytes; max {}). \
                 Reduce the old/new/content size.",
                stdin.len(),
                RUN_HELPER_STDIN_BUDGET
            ));
        }
        let wait_timeout = 60_u64;
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_run(
                ShellRunRequest {
                    client_id,
                    cwd: Some(cwd),
                    command,
                    stdin: Some(stdin),
                    timeout_secs: wait_timeout,
                    wait_timeout_secs: wait_timeout + 2,
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => return Err(e),
        };
        let resp = match tokio::time::timeout(Duration::from_secs(wait_timeout + 4), rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&request_id).await;
                return Err("agent helper request was dropped".to_string());
            }
            Err(_) => {
                self.shell_clients.cancel_request(&request_id).await;
                return Err("timed out waiting for agent helper".to_string());
            }
        };
        if let Some(e) = resp.error {
            return Err(e);
        }
        if resp.exit_code != Some(0) {
            return Err(resp
                .stderr
                .unwrap_or_else(|| format!("agent helper exited with code {:?}", resp.exit_code)));
        }
        let stdout = resp.stdout.unwrap_or_default();
        let stdout = stdout.trim();
        serde_json::from_str(stdout).map_err(|e| {
            format!(
                "agent helper returned invalid JSON: {} (got: {})",
                e,
                &stdout[..stdout.len().min(200)]
            )
        })
    }

    pub(crate) async fn read_file(
        &self,
        project: String,
        path: String,
        start_line: Option<usize>,
        limit: Option<usize>,
        with_line_numbers: Option<bool>,
    ) -> ToolResult {
        let with_line_numbers = with_line_numbers.unwrap_or(false);
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            let wait_timeout = 30;
            let (eff_start, _eff_limit, eff_end) = effective_read_file_range(start_line, limit);
            let (request_id, rx) = match self
                .shell_clients
                .enqueue_file_op(
                    ShellFileOpRequest {
                        op: "read".to_string(),
                        client_id,
                        path: path.clone(),
                        cwd: Some(proj.path.clone()),
                        content: None,
                        max_bytes: Some(512 * 1024),
                        old_text: None,
                        pattern: None,
                        expected_sha256: None,
                        expected_prefix: None,
                        start_line: Some(eff_start),
                        end_line: Some(eff_end),
                        line: None,
                        create_dirs: false,
                        wait_timeout_secs: wait_timeout,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::err(e),
            };
            return match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
                Ok(Ok(resp)) if resp.exit_code == Some(0) && resp.error.is_none() => {
                    read_file_agent_stdout_result_with_options(
                        resp.stdout.unwrap_or_default(),
                        start_line,
                        limit,
                        with_line_numbers,
                    )
                }
                Ok(Ok(resp)) => ToolResult::err(
                    resp.error
                        .or(resp.stderr)
                        .unwrap_or_else(|| "agent read_file failed".to_string()),
                ),
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err("agent read_file waiter was dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err("timed out waiting for agent read_file")
                }
            };
        }
        let file_path = proj.root().join(&path);
        let canonical_root = match proj.root().canonicalize() {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("Project root does not exist: {}", e)),
        };
        let canonical = match file_path.canonicalize() {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("Path does not exist: {}", e)),
        };
        if !canonical.starts_with(&canonical_root) {
            return ToolResult::err("Path is outside project directory");
        }
        if !canonical.is_file() {
            return ToolResult::err("Path is not a file");
        }
        let content = match std::fs::read_to_string(&canonical) {
            Ok(c) => c,
            Err(e) => return ToolResult::err(format!("Failed to read file: {}", e)),
        };
        read_file_content_result_with_options(content, start_line, limit, with_line_numbers)
    }

    // -------------------------------------------------------------------------
    // Project instructions auto-load (best-effort, session-start guidance)
    // -------------------------------------------------------------------------

    /// Best-effort load of project-local instruction files
    /// (`project_instructions::INSTRUCTION_CANDIDATE_PATHS`) for a resolved
    /// project. Candidates are tried in fixed order; the first candidate that
    /// reads successfully wins, bounding agent round-trips. Any read failure
    /// (agent not connected, file missing, timeout, decode error) is swallowed
    /// and the next candidate is tried. Returns an empty (`loaded=false`)
    /// snapshot when no candidate could be read.
    ///
    /// This never records session events (the session does not exist yet) and
    /// never fails `start_session`.
    pub(crate) async fn load_project_instructions(
        &self,
        config: &ProjectConfig,
    ) -> super::project_instructions::ProjectInstructionsSnapshot {
        use super::project_instructions::{
            ProjectInstructionsSnapshot, INSTRUCTION_CANDIDATE_PATHS,
        };
        for candidate in INSTRUCTION_CANDIDATE_PATHS {
            if let Some((content, total_lines)) =
                self.read_instruction_candidate(config, candidate).await
            {
                return ProjectInstructionsSnapshot::from_single_file(
                    candidate,
                    content,
                    total_lines,
                );
            }
        }
        ProjectInstructionsSnapshot::empty()
    }

    /// Read a single instruction candidate from a resolved project. Returns
    /// `(content, total_lines)` on success or `None` on any failure.
    ///
    /// For agent projects the read is routed to the owning agent via the
    /// `file_read` op with a short best-effort timeout. For server-configured
    /// (local) projects the file is read directly from the resolved root.
    async fn read_instruction_candidate(
        &self,
        config: &ProjectConfig,
        path: &str,
    ) -> Option<(String, usize)> {
        use super::project_instructions::MAX_LINES_PER_FILE;
        // Request one extra line so a returned line count strictly greater
        // than the per-file cap reliably signals line truncation regardless of
        // the agent response format (JSON sentinel vs plain-text fallback).
        let read_limit = MAX_LINES_PER_FILE + 1;
        const WAIT_TIMEOUT: u64 = 6;

        if config.is_agent() {
            let client_id = config.agent_client_id().ok()?;
            let (request_id, rx) = self
                .shell_clients
                .enqueue_file_op(
                    ShellFileOpRequest {
                        op: "read".to_string(),
                        client_id: client_id.to_string(),
                        path: path.to_string(),
                        cwd: Some(config.path.clone()),
                        content: None,
                        max_bytes: Some(512 * 1024),
                        old_text: None,
                        pattern: None,
                        expected_sha256: None,
                        expected_prefix: None,
                        start_line: Some(1),
                        end_line: Some(read_limit),
                        line: None,
                        create_dirs: false,
                        wait_timeout_secs: WAIT_TIMEOUT,
                    },
                    "project_instructions".to_string(),
                )
                .await
                .ok()?;
            match tokio::time::timeout(Duration::from_secs(WAIT_TIMEOUT + 2), rx).await {
                Ok(Ok(resp)) if resp.exit_code == Some(0) && resp.error.is_none() => {
                    parse_instruction_agent_stdout(resp.stdout.unwrap_or_default())
                }
                _ => {
                    self.shell_clients.cancel_request(&request_id).await;
                    None
                }
            }
        } else {
            // Server-configured (local) project: read directly. The root lives
            // on the server host, so the true total line count is exact.
            let root = config.root();
            let file_path = root.join(path);
            let canonical_root = root.canonicalize().ok()?;
            let canonical = file_path.canonicalize().ok()?;
            if !canonical.starts_with(&canonical_root) {
                return None;
            }
            if !canonical.is_file() {
                return None;
            }
            let content = std::fs::read_to_string(&canonical).ok()?;
            let total_lines = content.lines().count();
            if content_is_empty_instruction(&content) {
                return None;
            }
            Some((content, total_lines))
        }
    }

    /// `list_project_files`: bounded, project-relative file listing routed to
    /// the owning registered agent via the `file_list` op. The server never
    /// reads the agent project path directly. Returns `path` + `kind`
    /// (file/dir); size/mtime are not exposed by the current file op protocol.
    pub(crate) async fn list_project_files(
        &self,
        project: String,
        path: Option<String>,
        limit: Option<usize>,
    ) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let rel_path = path
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| ".".to_string());
        if let Err(e) = validate_project_relative_path(&rel_path) {
            return ToolResult::err(e);
        }
        let max_entries = limit.unwrap_or(200).clamp(1, 500);
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            let wait_timeout = 30;
            let (request_id, rx) = match self
                .shell_clients
                .enqueue_file_op(
                    ShellFileOpRequest {
                        op: "list".to_string(),
                        client_id,
                        path: rel_path.clone(),
                        cwd: Some(proj.path.clone()),
                        content: None,
                        max_bytes: None,
                        old_text: None,
                        pattern: None,
                        expected_sha256: None,
                        expected_prefix: None,
                        start_line: None,
                        end_line: None,
                        line: None,
                        create_dirs: false,
                        wait_timeout_secs: wait_timeout,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::err(e),
            };
            return match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
                Ok(Ok(resp)) if resp.exit_code == Some(0) && resp.error.is_none() => {
                    let stdout = resp.stdout.unwrap_or_default();
                    let (entries, truncated) =
                        parse_file_list_entries(&stdout, &rel_path, max_entries);
                    ToolResult::ok(json!({
                        "project": project,
                        "path": rel_path,
                        "entries": entries,
                        "truncated": truncated,
                    }))
                }
                Ok(Ok(resp)) => ToolResult::err(
                    resp.error
                        .or(resp.stderr)
                        .unwrap_or_else(|| "agent list_project_files failed".to_string()),
                ),
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err("agent list_project_files waiter was dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err("timed out waiting for agent list_project_files")
                }
            };
        }
        // Local-executor parity path (the runtime surface is agent-first; this
        // branch mirrors read_file/git_status for structural consistency).
        let root = proj.root();
        let dir = if rel_path == "." {
            root.to_path_buf()
        } else {
            root.join(&rel_path)
        };
        let canonical_root = match root.canonicalize() {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("Project root does not exist: {}", e)),
        };
        let canonical_dir = match dir.canonicalize() {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("Path does not exist: {}", e)),
        };
        if !canonical_dir.starts_with(&canonical_root) {
            return ToolResult::err("Path is outside project directory");
        }
        let (entries, truncated) = match std::fs::read_dir(&canonical_dir) {
            Ok(rd) => {
                let mut all = Vec::new();
                for entry in rd.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    all.push(json!({
                        "path": relative_entry_path(&rel_path, &name),
                        "kind": if is_dir { "dir" } else { "file" },
                    }));
                }
                all.sort_by(|a, b| {
                    a["path"]
                        .as_str()
                        .unwrap_or("")
                        .cmp(b["path"].as_str().unwrap_or(""))
                });
                let truncated = all.len() > max_entries;
                all.truncate(max_entries);
                (all, truncated)
            }
            Err(e) => return ToolResult::err(format!("Failed to list directory: {}", e)),
        };
        ToolResult::ok(json!({
            "project": project,
            "path": rel_path,
            "entries": entries,
            "truncated": truncated,
        }))
    }

    /// `search_project_text`: bounded text search routed to the owning agent
    /// via a bounded `grep -rnI` shell call. Excludes `.git`, `target`, and
    /// `node_modules` by default. Each match carries a project-relative path,
    /// 1-based line number, and a preview line.
    pub(crate) async fn search_project_text(
        &self,
        project: String,
        pattern: String,
        path: Option<String>,
        limit: Option<usize>,
        context_before: Option<usize>,
        context_after: Option<usize>,
    ) -> ToolResult {
        if pattern.trim().is_empty() {
            return ToolResult::err("pattern cannot be empty");
        }
        if pattern.contains('\0') {
            return ToolResult::err("pattern cannot contain NUL bytes");
        }
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let rel_path = path
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| ".".to_string());
        if let Err(e) = validate_project_relative_path(&rel_path) {
            return ToolResult::err(e);
        }
        let max_matches = limit.unwrap_or(50).clamp(1, 200);
        let (context_before, context_after) =
            effective_search_context(context_before, context_after);
        let include_context = context_before > 0 || context_after > 0;
        let cmd = if include_context {
            search_project_text_context_command(
                &pattern,
                &rel_path,
                max_matches,
                context_before,
                context_after,
            )
        } else {
            search_project_text_command(&pattern, &rel_path, max_matches)
        };
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            let (req_id, rx) = match self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id,
                        cwd: Some(proj.path.clone()),
                        command: cmd,
                        stdin: None,
                        timeout_secs: 30,
                        wait_timeout_secs: 32,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::err(e),
            };
            return match tokio::time::timeout(Duration::from_secs(34), rx).await {
                Ok(Ok(resp)) => {
                    let stdout = resp.stdout.unwrap_or_default();
                    let (matches, truncated) = if include_context {
                        parse_search_context_matches(&stdout, max_matches)
                    } else {
                        parse_search_matches(&stdout, max_matches)
                    };
                    let mut output = json!({
                        "project": project,
                        "pattern": pattern,
                        "path": rel_path,
                        "matches": matches,
                        "count": matches.len(),
                        "truncated": truncated,
                        "exit_code": resp.exit_code,
                    });
                    if include_context {
                        output["context_before"] = json!(context_before);
                        output["context_after"] = json!(context_after);
                    }
                    ToolResult::ok(output)
                }
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    ToolResult::err("request dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    ToolResult::err("timed out")
                }
            };
        }
        let root = proj.root();
        let result = tokio::task::spawn_blocking(move || run_command_sync(&cmd, &root, 30)).await;
        match result {
            Ok((exit_code, stdout, _stderr, _)) => {
                let (matches, truncated) = if include_context {
                    parse_search_context_matches(&stdout, max_matches)
                } else {
                    parse_search_matches(&stdout, max_matches)
                };
                let mut output = json!({
                    "project": project,
                    "pattern": pattern,
                    "path": rel_path,
                    "matches": matches,
                    "count": matches.len(),
                    "truncated": truncated,
                    "exit_code": exit_code,
                });
                if include_context {
                    output["context_before"] = json!(context_before);
                    output["context_after"] = json!(context_after);
                }
                ToolResult::ok(output)
            }
            Err(e) => ToolResult::err(format!("task join error: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "webcodex-{}-{}-{}",
            name,
            std::process::id(),
            stamp
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn run_python_helper(helper: &str, cwd: &std::path::Path, payload: Value) -> Value {
        let mut child = Command::new("python3")
            .arg("-c")
            .arg(helper)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn python3 helper");
        child
            .stdin
            .as_mut()
            .expect("helper stdin")
            .write_all(payload.to_string().as_bytes())
            .expect("write helper payload");
        let output = child.wait_with_output().expect("wait for helper");
        assert!(
            output.status.success(),
            "helper process failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("helper json stdout")
    }

    #[test]
    fn effective_read_file_range_defaults_and_clamps() {
        assert_eq!(effective_read_file_range(None, None), (1, 2000, 2000));
        assert_eq!(effective_read_file_range(Some(0), Some(0)), (1, 1, 1));
        assert_eq!(
            effective_read_file_range(Some(7), Some(5000)),
            (7, 2000, 2006)
        );
    }

    #[test]
    fn read_file_default_behavior_has_no_line_number_fields() {
        let result = read_file_content_result("one\ntwo\nthree".to_string(), Some(2), Some(1));

        assert!(result.success);
        assert_eq!(result.output["content"], "two");
        assert_eq!(result.output["total_lines"], 3);
        assert_eq!(result.output["start_line"], 2);
        assert_eq!(result.output["limit"], 1);
        assert!(result.output.get("numbered_text").is_none());
        assert!(result.output.get("lines").is_none());
    }

    #[test]
    fn read_file_agent_stdout_json_is_returned_without_reslicing() {
        let result = read_file_agent_stdout_result(
            serde_json::json!({
                "format": "webcodex.file_read_range.v1",
                "content": "line-560\nline-561",
                "total_lines": 7348,
                "start_line": 560,
                "limit": 2,
            })
            .to_string(),
            Some(1),
            Some(1),
        );

        assert!(result.success);
        assert_eq!(result.output["content"], "line-560\nline-561");
        assert_eq!(result.output["total_lines"], 7348);
        assert_eq!(result.output["start_line"], 560);
        assert_eq!(result.output["limit"], 2);
    }

    #[test]
    fn read_file_agent_stdout_json_without_sentinel_uses_legacy_fallback() {
        let result = read_file_agent_stdout_result(
            serde_json::json!({
                "content": "file-json-content",
                "total_lines": 7348,
                "start_line": 560,
                "limit": 2,
            })
            .to_string(),
            Some(1),
            Some(1),
        );

        assert!(result.success);
        assert_eq!(result.output["content"], "{\"content\":\"file-json-content\",\"limit\":2,\"start_line\":560,\"total_lines\":7348}");
        assert_eq!(result.output["total_lines"], 1);
        assert_eq!(result.output["start_line"], 1);
        assert_eq!(result.output["limit"], 1);
    }

    #[test]
    fn read_file_agent_stdout_plain_text_keeps_legacy_fallback() {
        let result =
            read_file_agent_stdout_result("one\ntwo\nthree\n".to_string(), Some(2), Some(1));

        assert!(result.success);
        assert_eq!(result.output["content"], "two");
        assert_eq!(result.output["total_lines"], 3);
        assert_eq!(result.output["start_line"], 2);
        assert_eq!(result.output["limit"], 1);
        assert!(result.output.get("numbered_text").is_none());
        assert!(result.output.get("lines").is_none());
    }

    #[test]
    fn read_file_with_line_numbers_returns_numbered_text_and_lines() {
        let result = read_file_content_result_with_options(
            "alpha\nbeta\ngamma".to_string(),
            None,
            None,
            true,
        );

        assert!(result.success);
        assert_eq!(result.output["content"], "alpha\nbeta\ngamma");
        assert_eq!(
            result.output["numbered_text"],
            "1 | alpha\n2 | beta\n3 | gamma"
        );
        assert_eq!(
            result.output["lines"],
            json!([
                {"line": 1, "text": "alpha"},
                {"line": 2, "text": "beta"},
                {"line": 3, "text": "gamma"},
            ])
        );
    }

    #[test]
    fn read_file_start_line_limit_with_line_numbers_uses_effective_range() {
        let result = read_file_content_result_with_options(
            "one\ntwo\nthree\nfour".to_string(),
            Some(2),
            Some(2),
            true,
        );

        assert!(result.success);
        assert_eq!(result.output["content"], "two\nthree");
        assert_eq!(result.output["start_line"], 2);
        assert_eq!(result.output["limit"], 2);
        assert_eq!(result.output["numbered_text"], "2 | two\n3 | three");
        assert_eq!(
            result.output["lines"],
            json!([
                {"line": 2, "text": "two"},
                {"line": 3, "text": "three"},
            ])
        );
    }

    #[test]
    fn read_file_with_line_numbers_handles_eof_and_short_files() {
        let result =
            read_file_content_result_with_options("one\ntwo".to_string(), Some(5), Some(3), true);

        assert!(result.success);
        assert_eq!(result.output["content"], "");
        assert_eq!(result.output["total_lines"], 2);
        assert_eq!(result.output["start_line"], 5);
        assert_eq!(result.output["limit"], 3);
        assert_eq!(result.output["numbered_text"], "");
        assert_eq!(result.output["lines"], json!([]));
    }

    #[test]
    fn read_file_agent_stdout_json_with_line_numbers_preserves_empty_lines() {
        let result = read_file_agent_stdout_result_with_options(
            serde_json::json!({
                "format": "webcodex.file_read_range.v1",
                "content": "\nsecond",
                "total_lines": 3,
                "start_line": 1,
                "limit": 2,
            })
            .to_string(),
            Some(1),
            Some(2),
            true,
        );

        assert!(result.success);
        assert_eq!(result.output["content"], "\nsecond");
        assert_eq!(result.output["numbered_text"], "1 | \n2 | second");
        assert_eq!(
            result.output["lines"],
            json!([
                {"line": 1, "text": ""},
                {"line": 2, "text": "second"},
            ])
        );
    }

    #[test]
    fn parse_search_matches_default_output_has_no_context_fields() {
        let (matches, truncated) = parse_search_matches("src/main.rs:42:fn main() {}\n", 10);

        assert!(!truncated);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "src/main.rs");
        assert_eq!(matches[0]["line"], 42);
        assert_eq!(matches[0]["preview"], "fn main() {}");
        assert!(matches[0].get("context_before").is_none());
        assert!(matches[0].get("context_after").is_none());
    }

    #[test]
    fn parse_search_context_matches_returns_context_line_numbers() {
        let stdout = serde_json::json!({
            "path": "src/lib.rs",
            "line": 3,
            "preview": "needle",
            "context_before": [
                {"line": 1, "text": "one"},
                {"line": 2, "text": "two"}
            ],
            "context_after": [
                {"line": 4, "text": "four"},
                {"line": 5, "text": "five"}
            ]
        })
        .to_string()
            + "\n"
            + &serde_json::json!({"truncated": false}).to_string()
            + "\n";

        let (matches, truncated) = parse_search_context_matches(&stdout, 10);

        assert!(!truncated);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "src/lib.rs");
        assert_eq!(matches[0]["line"], 3);
        assert_eq!(matches[0]["preview"], "needle");
        assert_eq!(
            matches[0]["context_before"],
            json!([
                {"line": 1, "text": "one"},
                {"line": 2, "text": "two"},
            ])
        );
        assert_eq!(
            matches[0]["context_after"],
            json!([
                {"line": 4, "text": "four"},
                {"line": 5, "text": "five"},
            ])
        );
    }

    #[test]
    fn search_context_helper_bounds_file_start_and_end() {
        let root = unique_temp_dir("search-context");
        std::fs::write(
            root.join("sample.txt"),
            "needle-start\nmiddle\nneedle-end\n",
        )
        .expect("write sample");
        let cmd = search_project_text_context_command("needle", ".", 10, 3, 3);
        let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);

        assert_eq!(exit_code, 0, "stderr: {stderr}");
        let (matches, truncated) = parse_search_context_matches(&stdout, 10);
        assert!(!truncated);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0]["line"], 1);
        assert_eq!(matches[0]["context_before"], json!([]));
        assert_eq!(
            matches[0]["context_after"],
            json!([
                {"line": 2, "text": "middle"},
                {"line": 3, "text": "needle-end"},
            ])
        );
        assert_eq!(matches[1]["line"], 3);
        assert_eq!(
            matches[1]["context_before"],
            json!([
                {"line": 1, "text": "needle-start"},
                {"line": 2, "text": "middle"},
            ])
        );
        assert_eq!(matches[1]["context_after"], json!([]));
    }

    #[test]
    fn effective_search_context_clamps_values() {
        assert_eq!(effective_search_context(None, None), (0, 0));
        assert_eq!(effective_search_context(Some(3), Some(8)), (3, 8));
        assert_eq!(effective_search_context(Some(21), Some(99)), (20, 20));
    }

    #[cfg(unix)]
    #[test]
    fn helper_read_project_artifact_rejects_symlink_escape() {
        let root = unique_temp_dir("artifact-read-root");
        let outside = unique_temp_dir("artifact-read-outside").join("outside.bin");
        let outside_content = b"outside-secret-content";
        std::fs::write(&outside, outside_content).expect("write outside file");
        std::os::unix::fs::symlink(&outside, root.join("leak.bin")).expect("create symlink");

        let out = run_python_helper(
            READ_PROJECT_ARTIFACT_HELPER,
            &root,
            json!({"path":"leak.bin","offset":0,"length":8,"max_file_bytes":MAX_PROJECT_ARTIFACT_BYTES}),
        );
        assert_eq!(out["error"], "artifact path escapes project root");
        assert!(!out.to_string().contains("outside-secret-content"));
    }

    #[cfg(unix)]
    #[test]
    fn helper_read_project_artifact_metadata_rejects_symlink_escape() {
        let root = unique_temp_dir("artifact-meta-root");
        let outside = unique_temp_dir("artifact-meta-outside").join("outside.bin");
        std::fs::write(&outside, b"outside-secret-content").expect("write outside file");
        std::os::unix::fs::symlink(&outside, root.join("leak.bin")).expect("create symlink");

        let out = run_python_helper(
            READ_PROJECT_ARTIFACT_METADATA_HELPER,
            &root,
            json!({"path":"leak.bin","max_bytes":MAX_PROJECT_ARTIFACT_BYTES}),
        );
        assert_eq!(out["error"], "artifact path escapes project root");
        assert!(!out.to_string().contains("outside-secret-content"));
    }

    #[cfg(unix)]
    #[test]
    fn helper_save_project_artifact_rejects_existing_symlink_target_escape() {
        let root = unique_temp_dir("artifact-save-root");
        let outside = unique_temp_dir("artifact-save-outside").join("outside.bin");
        std::fs::write(&outside, b"outside-secret-content").expect("write outside file");
        std::os::unix::fs::symlink(&outside, root.join("leak.bin")).expect("create symlink");

        let out = run_python_helper(
            SAVE_PROJECT_ARTIFACT_HELPER,
            &root,
            json!({"path":"leak.bin","content_base64":"bmV3","mime_type":"text/plain","overwrite":true,"max_bytes":MAX_PROJECT_ARTIFACT_BYTES}),
        );
        assert_eq!(out["error"], "refusing to overwrite symlink artifact path");
        assert_eq!(
            std::fs::read(&outside).expect("outside file remains readable"),
            b"outside-secret-content"
        );
        assert!(!out.to_string().contains("outside-secret-content"));
    }

    #[test]
    fn helper_save_project_artifact_allows_normal_nested_write() {
        let root = unique_temp_dir("artifact-save-normal");
        let out = run_python_helper(
            SAVE_PROJECT_ARTIFACT_HELPER,
            &root,
            json!({"path":"nested/out.txt","content_base64":"aGVsbG8=","mime_type":"text/plain","overwrite":false,"max_bytes":MAX_PROJECT_ARTIFACT_BYTES}),
        );
        assert!(out.get("error").is_none(), "unexpected helper error: {out}");
        assert_eq!(out["path"], "nested/out.txt");
        assert_eq!(out["bytes_written"], 5);
        assert_eq!(
            std::fs::read(root.join("nested/out.txt")).expect("read written artifact"),
            b"hello"
        );
    }

    #[test]
    fn parse_anchor_edit_stdout_rejects_empty_stdout_with_dispatch_hint() {
        let err = ToolRuntime::parse_anchor_edit_stdout("replace_exact_block", Some(String::new()))
            .expect_err("empty stdout should be rejected before JSON parsing");
        assert!(err.contains("empty stdout"), "{err}");
        assert!(err.contains("replace_exact_block"), "{err}");
        assert!(err.contains("transport dispatch"), "{err}");
    }
}
