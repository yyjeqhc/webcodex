use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

use super::helpers::{
    run_command_sync, shell_escape_simple, shell_join_paths, validate_limited_cleanup_paths,
    validate_project_relative_path,
};
use super::types::ToolResult;
use super::ToolRuntime;
use crate::shell_protocol::ShellRunRequest;
use crate::tool_runtime::sessions::{SessionEvent, SessionSummary};

/// Sentinel separating `git status --porcelain` from `git diff --stat` in the
/// combined `git_diff_summary` command output. Chosen to be extremely unlikely
/// to appear in real git output.
pub(crate) const DIFF_SUMMARY_SENTINEL: &str = "@@WEBCODEX_DIFF_SUMMARY_SEP@@";
pub(crate) const SHOW_CHANGES_SENTINEL: &str = "@@WEBCODEX_SHOW_CHANGES_SEP@@";
const DEFAULT_MAX_HUNKS: usize = 30;
const MAX_MAX_HUNKS: usize = 100;
const DEFAULT_MAX_HUNK_LINES: usize = 160;
const MAX_MAX_HUNK_LINES: usize = 400;
const SHOW_CHANGES_DEFAULT_MAX_HUNKS: usize = 20;
const SHOW_CHANGES_MAX_HUNKS: usize = 100;
const SHOW_CHANGES_DEFAULT_MAX_HUNK_LINES: usize = 80;
const SHOW_CHANGES_MAX_HUNK_LINES: usize = 240;
const SHOW_CHANGES_DEFAULT_SESSION_EVENT_LIMIT: usize = 30;
const SHOW_CHANGES_MAX_SESSION_EVENT_LIMIT: usize = 200;
const DEFAULT_GIT_LOG_LIMIT: usize = 20;
const MAX_GIT_LOG_LIMIT: usize = 100;
const MAX_GIT_LOG_SKIP: usize = 10_000;
const GIT_LOG_RECORD_SEP: char = '\u{1e}';
const GIT_LOG_UNIT_SEP: char = '\u{1f}';
const SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_FILES: usize = 5;
const SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_BYTES: u64 = 8192;
const SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_LINES: usize = 40;

/// Build the read-only `git_diff_summary` command. Runs `git status
/// --porcelain` and `git diff --stat` separated by a unique sentinel. No
/// mutating git subcommand is emitted.
pub(crate) fn git_diff_summary_command() -> String {
    format!(
        "git status --porcelain; printf '\\n{sentinel}\\n'; git diff --stat",
        sentinel = DIFF_SUMMARY_SENTINEL,
    )
}

pub(crate) fn normalize_git_log_limit(limit: Option<usize>) -> usize {
    limit
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_GIT_LOG_LIMIT)
        .min(MAX_GIT_LOG_LIMIT)
}

pub(crate) fn normalize_git_log_skip(skip: Option<usize>) -> usize {
    skip.unwrap_or(0).min(MAX_GIT_LOG_SKIP)
}

pub(crate) fn git_log_command(limit: usize, skip: usize) -> String {
    let limit_plus_one = limit.saturating_add(1);
    format!(
        "git log --decorate=short --date=iso-strict --pretty=format:'%H%x1f%h%x1f%D%x1f%an%x1f%ae%x1f%aI%x1f%s%x1e' -n {limit_plus_one} --skip {skip}",
    )
}

fn parse_git_log_refs(decorations: &str) -> Vec<String> {
    decorations
        .split(',')
        .flat_map(|part| {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else if let Some((head, branch)) = trimmed.split_once(" -> ") {
                vec![head.trim().to_string(), branch.trim().to_string()]
            } else if let Some(tag) = trimmed.strip_prefix("tag: ") {
                vec![tag.trim().to_string()]
            } else {
                vec![trimmed.to_string()]
            }
        })
        .collect()
}

pub(crate) fn parse_git_log_commits(stdout: &str, limit: usize) -> (Vec<Value>, bool) {
    let mut commits = Vec::new();
    let mut truncated = false;
    for record in stdout.split(GIT_LOG_RECORD_SEP) {
        let record = record.trim_matches(['\n', '\r']);
        if record.is_empty() {
            continue;
        }
        let fields: Vec<&str> = record.splitn(7, GIT_LOG_UNIT_SEP).collect();
        if fields.len() != 7 {
            continue;
        }
        if commits.len() >= limit {
            truncated = true;
            break;
        }
        commits.push(json!({
            "hash": fields[0],
            "short_hash": fields[1],
            "subject": fields[6],
            "author_name": fields[3],
            "author_email": fields[4],
            "author_date": fields[5],
            "refs": parse_git_log_refs(fields[2]),
        }));
    }
    (commits, truncated)
}

fn git_log_empty_repo(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("does not have any commits") || lower.contains("no commits yet")
}

/// Classify whether a failed `show_changes` git inspection is due to the
/// project directory not being inside a git repository.
///
/// Robust across git locales: the English `fatal: not a git repository`
/// message is matched directly, and a locale-independent structural signal
/// (non-zero exit with no porcelain branch header) covers localized git
/// builds where the fatal message is translated (e.g. `不是 git 仓库`). In a
/// real repository `git status --porcelain=v1 -b` always emits a `## `
/// branch header — even for a repo with no commits — so its absence combined
/// with a non-zero exit means git could not inspect the worktree, the common
/// case being a non-git project directory.
pub(crate) fn is_non_git_project_inspection(
    exit_code: Option<i32>,
    stderr: &str,
    status_stdout: &str,
) -> bool {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("not a git repository") {
        return true;
    }
    if exit_code != Some(0) && !status_stdout.lines().any(|line| line.starts_with("## ")) {
        return true;
    }
    false
}

/// Build the graceful-degradation payload returned by `show_changes` when the
/// project is not a git repository. Git-backed status/diff is reported as
/// unavailable without dumping git's noisy stderr/usage; the session
/// sub-summary is still layered on by the caller via
/// `apply_show_changes_session`.
pub(crate) fn non_git_show_changes_payload(
    project: &str,
    exit_code: Option<i32>,
    include_diff: bool,
) -> serde_json::Value {
    let mut payload = json!({
        "project": project,
        "git_available": false,
        "non_git_project": true,
        "git_error": "not a git repository; git-backed diff unavailable",
        "branch": null,
        "head": null,
        "clean": true,
        "counts": {
            "modified": 0,
            "added": 0,
            "deleted": 0,
            "renamed": 0,
            "copied": 0,
            "untracked": 0,
            "staged": 0,
            "unstaged": 0,
        },
        "files": [],
        "diff_stat": "",
        "warnings": [],
        "suggested_next_actions": [
            "git-backed status/diff unavailable; project is not a git repository",
        ],
        "session": null,
        "exit_code": exit_code,
        "stderr": "",
    });
    if include_diff {
        payload["untracked_previews"] = json!([]);
        payload["untracked_previews_truncated"] = json!(false);
    }
    payload
}

/// Build the read-only `show_changes` command. It combines the minimal git
/// inspections needed for a model-facing worktree summary. The optional full
/// diff is only emitted when the caller asks for bounded hunks.
pub(crate) fn show_changes_command(include_diff: bool) -> String {
    let diff_part = if include_diff {
        format!(
            "; printf '\\n{sentinel}\\n'; \
             git diff --unified=80",
            sentinel = SHOW_CHANGES_SENTINEL,
        )
    } else {
        String::new()
    };
    format!(
        "git status --porcelain=v1 -b; \
         printf '\\n{sentinel}\\n'; \
         {{ git log -1 --format='%H%x00%h%x00%s' || true; }}; \
         printf '\\n{sentinel}\\n'; \
         git diff --stat{diff_part}",
        sentinel = SHOW_CHANGES_SENTINEL,
        diff_part = diff_part,
    )
}

pub(crate) fn split_show_changes_stdout(
    stdout: &str,
    include_diff: bool,
) -> (String, String, String, String, String) {
    let mut parts = stdout.split(SHOW_CHANGES_SENTINEL);
    let status = parts
        .next()
        .unwrap_or_default()
        .trim_end_matches(['\n', '\r'])
        .to_string();
    let head = parts
        .next()
        .unwrap_or_default()
        .trim_matches(['\n', '\r'])
        .to_string();
    let stat = parts
        .next()
        .unwrap_or_default()
        .trim_matches(['\n', '\r'])
        .to_string();
    let diff = if include_diff {
        parts
            .next()
            .unwrap_or_default()
            .trim_start_matches(['\n', '\r'])
            .to_string()
    } else {
        String::new()
    };
    let untracked_preview = if include_diff {
        parts
            .next()
            .unwrap_or_default()
            .trim_matches(['\n', '\r'])
            .to_string()
    } else {
        String::new()
    };
    (status, head, stat, diff, untracked_preview)
}

fn parse_status_branch(line: &str) -> Option<String> {
    let rest = line.strip_prefix("## ")?;
    let branch = rest
        .split("...")
        .next()
        .unwrap_or(rest)
        .trim()
        .trim_matches('"');
    if branch.is_empty() {
        None
    } else {
        Some(branch.to_string())
    }
}

fn parse_show_changes_head(head: &str) -> serde_json::Value {
    let mut parts = head.splitn(3, '\0');
    let commit = parts.next().unwrap_or_default().trim();
    let short = parts.next().unwrap_or_default().trim();
    let summary = parts.next().unwrap_or_default().trim();
    if commit.is_empty() {
        json!({
            "commit": null,
            "short": null,
            "summary": null,
        })
    } else {
        json!({
            "commit": commit,
            "short": if short.is_empty() { commit.chars().take(7).collect::<String>() } else { short.to_string() },
            "summary": summary,
        })
    }
}

fn porcelain_path(path_part: &str) -> (String, Option<String>) {
    let path_part = path_part.trim().trim_matches('"');
    if let Some((old, new)) = path_part.split_once(" -> ") {
        (
            new.trim().trim_matches('"').to_string(),
            Some(old.trim().trim_matches('"').to_string()),
        )
    } else {
        (path_part.to_string(), None)
    }
}

fn status_label(x: char, y: char) -> &'static str {
    if x == '?' && y == '?' {
        return "untracked";
    }
    if x == 'R' || y == 'R' {
        "renamed"
    } else if x == 'C' || y == 'C' {
        "copied"
    } else if x == 'D' || y == 'D' {
        "deleted"
    } else if x == 'A' || y == 'A' {
        "added"
    } else {
        "modified"
    }
}

fn looks_like_smoke_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains("smoke")
        || lower.contains("tmp")
        || lower.contains("test")
        || lower.contains("anchor")
}

pub(crate) fn parse_show_changes_output(
    project: &str,
    status_stdout: &str,
    head_stdout: &str,
    diff_stat: &str,
    diff_stdout: Option<&str>,
    max_hunks: usize,
    max_hunk_lines: usize,
    exit_code: Option<i32>,
    stderr: &str,
) -> serde_json::Value {
    let mut branch = None;
    let mut files = Vec::new();
    let mut modified = 0usize;
    let mut added = 0usize;
    let mut deleted = 0usize;
    let mut renamed = 0usize;
    let mut copied = 0usize;
    let mut untracked = 0usize;
    let mut staged_count = 0usize;
    let mut unstaged_count = 0usize;

    for line in status_stdout.lines() {
        if let Some(parsed) = parse_status_branch(line) {
            branch = Some(parsed);
            continue;
        }
        if line.len() < 3 {
            continue;
        }
        let mut chars = line.chars();
        let x = chars.next().unwrap_or(' ');
        let y = chars.next().unwrap_or(' ');
        if x == '!' && y == '!' {
            continue;
        }
        let path_part = line.get(3..).unwrap_or_default();
        let (path, old_path) = porcelain_path(path_part);
        if path.is_empty() {
            continue;
        }
        let status = status_label(x, y);
        let is_untracked = status == "untracked";
        let staged = !is_untracked && x != ' ' && x != '?';
        let unstaged = !is_untracked && y != ' ' && y != '?';
        match status {
            "modified" => modified += 1,
            "added" => added += 1,
            "deleted" => deleted += 1,
            "renamed" => renamed += 1,
            "copied" => copied += 1,
            "untracked" => untracked += 1,
            _ => {}
        }
        if staged {
            staged_count += 1;
        }
        if unstaged {
            unstaged_count += 1;
        }
        let mut file = serde_json::Map::new();
        file.insert("path".to_string(), json!(path));
        file.insert("status".to_string(), json!(status));
        file.insert("staged".to_string(), json!(staged));
        file.insert("unstaged".to_string(), json!(unstaged));
        file.insert(
            "kind".to_string(),
            json!(if is_untracked { "untracked" } else { "tracked" }),
        );
        if let Some(old_path) = old_path {
            file.insert("old_path".to_string(), json!(old_path));
        }
        files.push(json!(file));
    }

    let clean = files.is_empty();
    let mut warnings = Vec::new();
    for file in &files {
        if file["kind"] != "untracked" {
            continue;
        }
        let path = file["path"].as_str().unwrap_or_default();
        if looks_like_smoke_path(path) {
            warnings.push(json!({
                "kind": "untracked_smoke_file",
                "path": path,
                "message": "untracked smoke/tmp/test/anchor file should be reviewed before commit",
            }));
        } else {
            warnings.push(json!({
                "kind": "untracked_file",
                "path": path,
                "message": "untracked file should be reviewed before commit",
            }));
        }
    }

    let suggested_next_actions =
        suggested_next_actions_for(clean, untracked > 0, has_smoke_warning(&warnings), None);

    let mut output = json!({
        "project": project,
        "git_available": true,
        "non_git_project": false,
        "git_error": null,
        "branch": branch,
        "head": parse_show_changes_head(head_stdout),
        "clean": clean,
        "counts": {
            "modified": modified,
            "added": added,
            "deleted": deleted,
            "renamed": renamed,
            "copied": copied,
            "untracked": untracked,
            "staged": staged_count,
            "unstaged": unstaged_count,
        },
        "files": files,
        "diff_stat": diff_stat,
        "warnings": warnings,
        "suggested_next_actions": suggested_next_actions,
        "session": null,
        "exit_code": exit_code,
        "stderr": stderr,
    });

    if let Some(diff) = diff_stdout {
        let (hunks, hunk_count, truncated) = parse_git_diff_hunks(diff, max_hunks, max_hunk_lines);
        output["hunks"] = json!(hunks);
        output["hunk_count"] = json!(hunk_count);
        output["hunks_truncated"] = json!(truncated);
    }

    output
}

fn parse_untracked_previews_stdout(preview_stdout: &str) -> Result<(Vec<Value>, bool), String> {
    let preview_stdout = preview_stdout.trim();
    if preview_stdout.is_empty() {
        return Ok((Vec::new(), false));
    }
    let value: Value = serde_json::from_str(preview_stdout)
        .map_err(|e| format!("failed to parse untracked preview JSON: {}", e))?;
    match value {
        Value::Array(entries) => Ok((entries, false)),
        Value::Object(mut object) => {
            let previews = object
                .remove("previews")
                .and_then(|previews| match previews {
                    Value::Array(entries) => Some(entries),
                    _ => None,
                })
                .unwrap_or_default();
            let truncated = object
                .remove("truncated")
                .and_then(|truncated| truncated.as_bool())
                .unwrap_or(false);
            Ok((previews, truncated))
        }
        _ => Err("untracked preview JSON must be an array or object".to_string()),
    }
}

pub(crate) fn apply_show_changes_untracked_previews(output: &mut Value, preview_stdout: &str) {
    match parse_untracked_previews_stdout(preview_stdout) {
        Ok((previews, truncated)) => {
            output["untracked_previews"] = json!(previews);
            output["untracked_previews_truncated"] = json!(truncated);
        }
        Err(error) => {
            output["untracked_previews"] = json!([]);
            output["untracked_previews_truncated"] = json!(false);
            if let Some(warnings) = output["warnings"].as_array_mut() {
                warnings.push(json!({
                    "kind": "untracked_preview_parse_failed",
                    "message": error,
                }));
            }
        }
    }
}

fn skipped_untracked_preview(path: &str, reason: &str, byte_count: Option<u64>) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("path".to_string(), json!(path));
    object.insert("kind".to_string(), json!("skipped"));
    object.insert("reason".to_string(), json!(reason));
    if let Some(byte_count) = byte_count {
        object.insert("byte_count".to_string(), json!(byte_count));
    }
    Value::Object(object)
}

fn untracked_preview_path_is_invalid(path: &str) -> bool {
    let trimmed = path.trim();
    trimmed.is_empty()
        || trimmed == "."
        || validate_project_relative_path(trimmed).is_err()
        || trimmed.split('/').any(|part| part.is_empty())
}

fn untracked_preview_path_is_sensitive(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    normalized
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .any(|part| {
            matches!(
                part,
                ".git"
                    | "target"
                    | "node_modules"
                    | "projects.d"
                    | "agent.toml"
                    | "webcodex.env"
                    | ".env"
                    | "secrets"
                    | "tokens"
                    | "id_rsa"
                    | "id_ed25519"
            ) || part.starts_with(".env")
                || part.starts_with("agent.toml")
                || part.starts_with("webcodex.env")
                || part.ends_with(".pem")
                || part.ends_with(".key")
        })
}

fn untracked_preview_from_bytes(
    path: &str,
    data: &[u8],
    declared_byte_count: Option<u64>,
) -> Value {
    let byte_count = declared_byte_count.unwrap_or(data.len() as u64);
    if data.len() as u64 > SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_BYTES {
        return skipped_untracked_preview(
            path,
            "too_large",
            Some(byte_count.max(data.len() as u64)),
        );
    }
    if data
        .iter()
        .any(|byte| *byte == 0 || (*byte < 32 && !matches!(*byte, b'\t' | b'\n' | b'\r')))
    {
        return skipped_untracked_preview(path, "binary_or_non_utf8", Some(data.len() as u64));
    }
    let text = match std::str::from_utf8(data) {
        Ok(text) => text,
        Err(_) => {
            return skipped_untracked_preview(path, "binary_or_non_utf8", Some(data.len() as u64))
        }
    };
    let all_lines: Vec<&str> = text.lines().collect();
    let shown_lines: Vec<Value> = all_lines
        .iter()
        .take(SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_LINES)
        .enumerate()
        .map(|(index, line)| {
            json!({
                "line": index + 1,
                "text": line,
            })
        })
        .collect();
    json!({
        "path": path,
        "kind": "text",
        "line_count": all_lines.len(),
        "byte_count": byte_count,
        "truncated": all_lines.len() > SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_LINES,
        "lines": shown_lines,
    })
}

pub(crate) fn collect_show_changes_untracked_previews_for_root(
    root: &Path,
    untracked_paths: &[String],
) -> (Vec<Value>, bool) {
    let truncated = untracked_paths.len() > SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_FILES;
    let canonical_root = match root.canonicalize() {
        Ok(root) => root,
        Err(_) => return (Vec::new(), truncated),
    };
    let mut previews = Vec::new();
    for path in untracked_paths
        .iter()
        .take(SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_FILES)
    {
        if untracked_preview_path_is_invalid(path) || untracked_preview_path_is_sensitive(path) {
            previews.push(skipped_untracked_preview(
                path,
                "sensitive_or_excluded_path",
                None,
            ));
            continue;
        }
        let full_path = root.join(path);
        let metadata = match std::fs::symlink_metadata(&full_path) {
            Ok(metadata) => metadata,
            Err(_) => {
                previews.push(skipped_untracked_preview(path, "not_found", None));
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            previews.push(skipped_untracked_preview(
                path,
                "sensitive_or_excluded_path",
                None,
            ));
            continue;
        }
        if !metadata.is_file() {
            previews.push(skipped_untracked_preview(path, "not_regular_file", None));
            continue;
        }
        let canonical = match full_path.canonicalize() {
            Ok(canonical) => canonical,
            Err(_) => {
                previews.push(skipped_untracked_preview(
                    path,
                    "sensitive_or_excluded_path",
                    None,
                ));
                continue;
            }
        };
        if !canonical.starts_with(&canonical_root) {
            previews.push(skipped_untracked_preview(
                path,
                "sensitive_or_excluded_path",
                None,
            ));
            continue;
        }
        let byte_count = metadata.len();
        if byte_count > SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_BYTES {
            previews.push(skipped_untracked_preview(
                path,
                "too_large",
                Some(byte_count),
            ));
            continue;
        }
        match std::fs::read(&full_path) {
            Ok(data) => previews.push(untracked_preview_from_bytes(path, &data, Some(byte_count))),
            Err(_) => previews.push(skipped_untracked_preview(path, "read_error", None)),
        }
    }
    (previews, truncated)
}

fn show_changes_untracked_paths(output: &Value) -> Vec<String> {
    output["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|file| file["kind"] == "untracked")
        .filter_map(|file| file["path"].as_str().map(str::to_string))
        .collect()
}

fn show_changes_untracked_preview_probe_command(path: &str) -> String {
    format!(
        "p={path}; \
         if [ -L \"$p\" ]; then printf 'SKIP\\tsensitive_or_excluded_path\\n'; \
         elif [ ! -e \"$p\" ]; then printf 'SKIP\\tnot_found\\n'; \
         elif [ ! -f \"$p\" ]; then printf 'SKIP\\tnot_regular_file\\n'; \
         else bytes=$(wc -c < \"$p\" 2>/dev/null | tr -d '[:space:]'); \
           case \"$bytes\" in \
             ''|*[!0-9]*) printf 'SKIP\\tread_error\\n' ;; \
             *) if [ \"$bytes\" -gt {max_bytes} ]; then printf 'SKIP\\ttoo_large\\t%s\\n' \"$bytes\"; \
                else printf 'DATA\\t%s\\n' \"$bytes\"; base64 < \"$p\" 2>/dev/null; fi ;; \
           esac; \
         fi",
        path = shell_escape_simple(path),
        max_bytes = SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_BYTES,
    )
}

fn parse_show_changes_agent_preview_probe(
    path: &str,
    stdout: &str,
    exit_code: Option<i32>,
) -> Value {
    let mut lines = stdout.lines();
    let Some(header) = lines.next() else {
        return skipped_untracked_preview(path, "read_error", None);
    };
    let parts: Vec<&str> = header.split('\t').collect();
    match parts.as_slice() {
        ["SKIP", reason] => skipped_untracked_preview(path, reason, None),
        ["SKIP", reason, byte_count] => {
            skipped_untracked_preview(path, reason, byte_count.parse::<u64>().ok())
        }
        ["DATA", byte_count] => {
            if exit_code != Some(0) {
                return skipped_untracked_preview(
                    path,
                    "read_error",
                    byte_count.parse::<u64>().ok(),
                );
            }
            let encoded = lines.collect::<Vec<_>>().join("");
            let data = match general_purpose::STANDARD.decode(encoded.as_bytes()) {
                Ok(data) => data,
                Err(_) => return skipped_untracked_preview(path, "read_error", None),
            };
            untracked_preview_from_bytes(path, &data, byte_count.parse::<u64>().ok())
        }
        _ => skipped_untracked_preview(path, "read_error", None),
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct SessionActionSignals {
    failed: bool,
    write_like: bool,
    shell_like: bool,
}

fn has_smoke_warning(warnings: &[Value]) -> bool {
    warnings
        .iter()
        .any(|warning| warning["kind"] == "untracked_smoke_file")
}

fn suggested_next_actions_for(
    clean: bool,
    has_untracked: bool,
    has_smoke_warning: bool,
    session: Option<SessionActionSignals>,
) -> Vec<String> {
    let mut actions = Vec::new();
    let session = session.unwrap_or_default();
    if clean && !session.failed {
        push_unique_action(&mut actions, "no changes detected");
    }
    if !clean {
        push_unique_action(&mut actions, "review diff");
        push_unique_action(&mut actions, "run focused tests");
        if has_untracked {
            push_unique_action(&mut actions, "review untracked files before commit");
        }
        if has_smoke_warning {
            push_unique_action(
                &mut actions,
                "clean untracked smoke/tmp files or intentionally commit them",
            );
        }
        push_unique_action(&mut actions, "commit or revert changes after review");
    }
    if session.failed {
        push_unique_action(&mut actions, "review failed tool calls in session_summary");
    }
    if session.write_like {
        push_unique_action(&mut actions, "review changed paths from this session");
    }
    if session.shell_like {
        push_unique_action(&mut actions, "check command/test results before commit");
    }
    actions
}

fn push_unique_action(actions: &mut Vec<String>, action: &str) {
    if !actions.iter().any(|existing| existing == action) {
        actions.push(action.to_string());
    }
}

pub(crate) fn apply_show_changes_session(
    output: &mut Value,
    session_id: Option<&str>,
    summary: Option<SessionSummary>,
) {
    let Some(session_id) = session_id else {
        output["session"] = Value::Null;
        return;
    };
    let session_signals = match summary {
        Some(summary) => {
            let changed_paths = session_changed_paths(&summary.events);
            let recent_events: Vec<Value> = summary
                .events
                .iter()
                .map(show_changes_session_event)
                .collect();
            let signals = SessionActionSignals {
                failed: summary.counts.failed > 0,
                write_like: summary.counts.write_like > 0,
                shell_like: summary.counts.shell_like > 0,
            };
            output["session"] = json!({
                "found": true,
                "session_id": summary.session_id,
                "project": summary.project,
                "title": summary.title,
                "created_at": summary.created_at,
                "updated_at": summary.updated_at,
                "counts": summary.counts,
                "changed_paths": changed_paths,
                "recent_events": recent_events,
            });
            Some(signals)
        }
        None => {
            output["session"] = json!({
                "found": false,
                "session_id": session_id,
                "message": "session not found",
            });
            if let Some(warnings) = output["warnings"].as_array_mut() {
                warnings.push(json!({
                    "kind": "session_not_found",
                    "session_id": session_id,
                    "message": "session not found",
                }));
            }
            None
        }
    };
    refresh_show_changes_suggestions(output, session_signals);
}

fn refresh_show_changes_suggestions(output: &mut Value, session: Option<SessionActionSignals>) {
    // Non-git projects: keep the git-unavailable message as the primary
    // suggestion and only append session-signal suggestions. The normal
    // clean/dirty review suggestions do not apply when git inspection is
    // unavailable.
    if output["non_git_project"].as_bool().unwrap_or(false) {
        let session = session.unwrap_or_default();
        let mut actions =
            vec!["git-backed status/diff unavailable; project is not a git repository".to_string()];
        if session.failed {
            push_unique_action(&mut actions, "review failed tool calls in session_summary");
        }
        if session.write_like {
            push_unique_action(&mut actions, "review changed paths from this session");
        }
        if session.shell_like {
            push_unique_action(&mut actions, "check command/test results before commit");
        }
        output["suggested_next_actions"] = json!(actions);
        return;
    }
    let clean = output["clean"].as_bool().unwrap_or(false);
    let has_untracked = output["counts"]["untracked"].as_u64().unwrap_or(0) > 0;
    let has_smoke_warning = output["warnings"]
        .as_array()
        .is_some_and(|warnings| has_smoke_warning(warnings));
    output["suggested_next_actions"] = json!(suggested_next_actions_for(
        clean,
        has_untracked,
        has_smoke_warning,
        session,
    ));
}

fn session_changed_paths(events: &[SessionEvent]) -> Vec<String> {
    let mut paths = Vec::new();
    for event in events {
        for path in &event.changed_paths {
            let path = path.trim();
            if !path.is_empty() && !paths.iter().any(|existing| existing == path) {
                paths.push(path.to_string());
            }
        }
    }
    paths
}

fn show_changes_session_event(event: &SessionEvent) -> Value {
    json!({
        "event_id": event.event_id,
        "kind": event.kind,
        "timestamp": event.timestamp,
        "transport": event.transport,
        "tool_name": event.tool_name,
        "project": event.project,
        "resolved_project": event.resolved_project,
        "risk_class": event.risk_class,
        "read_like": event.read_like,
        "write_like": event.write_like,
        "shell_like": event.shell_like,
        "git_like": event.git_like,
        "change_summary_like": event.change_summary_like,
        "started_at": event.started_at,
        "finished_at": event.finished_at,
        "status": event.status,
        "exit_code": event.exit_code,
        "failure_kind": event.failure_kind,
        "duration_ms": event.duration_ms,
        "error_kind": event.error_kind,
        "error_message_summary": event.error_message_summary,
        "changed_paths": event.changed_paths,
        "job_id": event.job_id,
    })
}

/// Split the combined `git_diff_summary` stdout into the porcelain section and
/// the `diff --stat` section. If the sentinel is absent, everything is treated
/// as porcelain (defensive; should not happen in practice).
pub(crate) fn split_diff_summary(stdout: &str) -> (String, String) {
    if let Some((before, after)) = stdout.split_once(DIFF_SUMMARY_SENTINEL) {
        (
            before.trim_end_matches(['\n', '\r']).to_string(),
            after
                .trim_start_matches(['\n', '\r'])
                .trim_end()
                .to_string(),
        )
    } else {
        (stdout.trim_end().to_string(), String::new())
    }
}

fn clean_optional_paths(paths: Option<Vec<String>>) -> Result<Vec<String>, String> {
    let mut clean = Vec::new();
    for raw in paths.unwrap_or_default() {
        validate_project_relative_path(&raw)?;
        let path = raw.trim().trim_start_matches("./").trim_end_matches('/');
        if path.is_empty() || path == "." {
            return Err(
                "diff path must name a file or directory, not the project root".to_string(),
            );
        }
        if !clean.iter().any(|p: &String| p == path) {
            clean.push(path.to_string());
        }
    }
    Ok(clean)
}

pub(crate) fn git_diff_hunks_command(paths: &[String], cached: bool) -> Result<String, String> {
    let mut parts = vec!["git".to_string(), "diff".to_string()];
    if cached {
        parts.push("--cached".to_string());
    }
    parts.push("--unified=80".to_string());
    if !paths.is_empty() {
        parts.push("--".to_string());
        parts.extend(paths.iter().map(|path| shell_escape_simple(path)));
    }
    Ok(parts.join(" "))
}

fn strip_diff_prefix(path: &str) -> String {
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
        .to_string()
}

fn parse_hunk_header(header: &str) -> (i64, i64, i64, i64) {
    fn parse_range(raw: &str) -> (i64, i64) {
        let raw = raw.trim_start_matches(['-', '+']);
        let mut parts = raw.splitn(2, ',');
        let start = parts.next().unwrap_or("0").parse::<i64>().unwrap_or(0);
        let lines = parts.next().unwrap_or("1").parse::<i64>().unwrap_or(1);
        (start, lines)
    }
    let mut parts = header.split_whitespace();
    let _at = parts.next();
    let old = parts.next().unwrap_or("-0,0");
    let new = parts.next().unwrap_or("+0,0");
    let (old_start, old_lines) = parse_range(old);
    let (new_start, new_lines) = parse_range(new);
    (old_start, old_lines, new_start, new_lines)
}

fn finish_hunk(
    file: &mut serde_json::Map<String, serde_json::Value>,
    current_hunk: &mut Option<serde_json::Map<String, serde_json::Value>>,
    hunk_lines: &mut Vec<String>,
) {
    let Some(mut hunk) = current_hunk.take() else {
        return;
    };
    hunk.insert("diff".to_string(), json!(hunk_lines.join("\n")));
    hunk.insert("line_count".to_string(), json!(hunk_lines.len()));
    file.entry("hunks".to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("hunks array")
        .push(json!(hunk));
    hunk_lines.clear();
}

fn finish_file(
    files: &mut Vec<serde_json::Value>,
    current_file: &mut Option<serde_json::Map<String, serde_json::Value>>,
    current_hunk: &mut Option<serde_json::Map<String, serde_json::Value>>,
    hunk_lines: &mut Vec<String>,
) {
    let Some(mut file) = current_file.take() else {
        return;
    };
    finish_hunk(&mut file, current_hunk, hunk_lines);
    if file.get("hunks").is_none() {
        file.insert("hunks".to_string(), json!([]));
    }
    files.push(json!(file));
}

pub(crate) fn parse_git_diff_hunks(
    diff: &str,
    max_hunks: usize,
    max_hunk_lines: usize,
) -> (Vec<serde_json::Value>, usize, bool) {
    let mut files = Vec::new();
    let mut current_file: Option<serde_json::Map<String, serde_json::Value>> = None;
    let mut current_hunk: Option<serde_json::Map<String, serde_json::Value>> = None;
    let mut hunk_lines = Vec::new();
    let mut hunk_count = 0usize;
    let mut truncated = false;
    let mut skip_current_hunk = false;

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            finish_file(
                &mut files,
                &mut current_file,
                &mut current_hunk,
                &mut hunk_lines,
            );
            let mut parts = rest.split_whitespace();
            let old_path = parts.next().map(strip_diff_prefix).unwrap_or_default();
            let path = parts.next().map(strip_diff_prefix).unwrap_or_default();
            let mut file = serde_json::Map::new();
            file.insert("path".to_string(), json!(path));
            file.insert("old_path".to_string(), json!(old_path));
            file.insert("status".to_string(), json!("modified"));
            file.insert("hunks".to_string(), json!([]));
            current_file = Some(file);
            skip_current_hunk = false;
            continue;
        }

        let Some(file) = current_file.as_mut() else {
            continue;
        };

        if line.starts_with("new file mode ") {
            file.insert("status".to_string(), json!("added"));
        } else if line.starts_with("deleted file mode ") {
            file.insert("status".to_string(), json!("deleted"));
        } else if let Some(path) = line.strip_prefix("rename from ") {
            file.insert("old_path".to_string(), json!(path));
            file.insert("status".to_string(), json!("renamed"));
        } else if let Some(path) = line.strip_prefix("rename to ") {
            file.insert("path".to_string(), json!(path));
            file.insert("status".to_string(), json!("renamed"));
        } else if line.starts_with("Binary files ") {
            file.insert("binary".to_string(), json!(true));
        } else if let Some(path) = line.strip_prefix("--- ") {
            if path == "/dev/null" {
                file.insert("old_path".to_string(), json!(null));
                file.insert("status".to_string(), json!("added"));
            } else {
                file.insert("old_path".to_string(), json!(strip_diff_prefix(path)));
            }
        } else if let Some(path) = line.strip_prefix("+++ ") {
            if path == "/dev/null" {
                file.insert("path".to_string(), json!(null));
                file.insert("status".to_string(), json!("deleted"));
            } else {
                file.insert("path".to_string(), json!(strip_diff_prefix(path)));
            }
        }

        if line.starts_with("@@ ") {
            finish_hunk(file, &mut current_hunk, &mut hunk_lines);
            if hunk_count >= max_hunks {
                truncated = true;
                skip_current_hunk = true;
                continue;
            }
            let (old_start, old_lines, new_start, new_lines) = parse_hunk_header(line);
            let mut hunk = serde_json::Map::new();
            hunk.insert("old_start".to_string(), json!(old_start));
            hunk.insert("old_lines".to_string(), json!(old_lines));
            hunk.insert("new_start".to_string(), json!(new_start));
            hunk.insert("new_lines".to_string(), json!(new_lines));
            hunk.insert("header".to_string(), json!(line));
            hunk.insert("truncated".to_string(), json!(false));
            current_hunk = Some(hunk);
            hunk_lines.push(line.to_string());
            hunk_count += 1;
            skip_current_hunk = false;
            continue;
        }

        if current_hunk.is_some() && !skip_current_hunk {
            if hunk_lines.len() < max_hunk_lines {
                hunk_lines.push(line.to_string());
            } else {
                truncated = true;
                if let Some(hunk) = current_hunk.as_mut() {
                    hunk.insert("truncated".to_string(), json!(true));
                }
            }
        }
    }
    finish_file(
        &mut files,
        &mut current_file,
        &mut current_hunk,
        &mut hunk_lines,
    );
    (files, hunk_count, truncated)
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PorcelainSummary {
    pub(crate) changed_files: Vec<String>,
    pub(crate) tracked_changed_files: Vec<String>,
    pub(crate) untracked_files: Vec<String>,
    pub(crate) ignored_files: Vec<String>,
    pub(crate) changed_files_count: usize,
}

/// Parse `git status --porcelain` output into tracked/untracked buckets.
/// Handles renames (`R  old -> new` -> `new`) and quoted paths.
pub(crate) fn parse_porcelain_summary(porcelain: &str) -> PorcelainSummary {
    let mut summary = PorcelainSummary::default();
    for line in porcelain.lines() {
        if line.len() < 4 {
            continue;
        }
        let status = &line[..2];
        let path_part = &line[3..];
        let path = if let Some((_, dst)) = path_part.split_once(" -> ") {
            dst
        } else {
            path_part
        };
        let path = path.trim().trim_matches('"');
        if path.is_empty() {
            continue;
        }
        match status {
            "??" => summary.untracked_files.push(path.to_string()),
            "!!" => summary.ignored_files.push(path.to_string()),
            _ => summary.tracked_changed_files.push(path.to_string()),
        }
        summary.changed_files.push(path.to_string());
    }
    summary.changed_files_count = summary.changed_files.len();
    summary
}

/// Backward-compatible helper for older tests/callers that only need all paths.
#[allow(dead_code)]
pub(crate) fn parse_porcelain_files(porcelain: &str) -> Vec<String> {
    parse_porcelain_summary(porcelain).changed_files
}

impl ToolRuntime {
    async fn collect_show_changes_untracked_previews(
        &self,
        project: &str,
        untracked_paths: &[String],
    ) -> (Vec<Value>, bool) {
        let truncated = untracked_paths.len() > SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_FILES;
        let proj = match self.resolve_project(project).await {
            Ok(proj) => proj,
            Err(_) => return (Vec::new(), truncated),
        };
        if !proj.is_agent() {
            return collect_show_changes_untracked_previews_for_root(&proj.root(), untracked_paths);
        }
        let mut previews = Vec::new();
        for path in untracked_paths
            .iter()
            .take(SHOW_CHANGES_UNTRACKED_PREVIEW_MAX_FILES)
        {
            if untracked_preview_path_is_invalid(path) || untracked_preview_path_is_sensitive(path)
            {
                previews.push(skipped_untracked_preview(
                    path,
                    "sensitive_or_excluded_path",
                    None,
                ));
                continue;
            }
            let command = show_changes_untracked_preview_probe_command(path);
            let preview = match self
                .run_project_command_capture(project, command, 10, None)
                .await
            {
                Ok(output) => {
                    parse_show_changes_agent_preview_probe(path, &output.stdout, output.exit_code)
                }
                Err(_) => skipped_untracked_preview(path, "read_error", None),
            };
            previews.push(preview);
        }
        (previews, truncated)
    }

    pub(crate) async fn git_restore_paths(
        &self,
        project: String,
        paths: Vec<String>,
    ) -> ToolResult {
        let paths = match validate_limited_cleanup_paths(&paths, true) {
            Ok(paths) => paths,
            Err(e) => return ToolResult::err(e),
        };
        let command = format!("git restore -- {}", shell_join_paths(&paths));
        let result = self.run_shell(project, command, Some(30), None).await;
        if result.success {
            ToolResult::ok(json!({
                "restored_paths": paths,
                "command_result": result.output,
            }))
        } else {
            result
        }
    }

    pub(crate) async fn discard_untracked(
        &self,
        project: String,
        paths: Vec<String>,
    ) -> ToolResult {
        let paths = match validate_limited_cleanup_paths(&paths, true) {
            Ok(paths) => paths,
            Err(e) => return ToolResult::err(e),
        };
        let command = format!("git clean -f -- {}", shell_join_paths(&paths));
        let result = self.run_shell(project, command, Some(30), None).await;
        if result.success {
            ToolResult::ok(json!({
                "discarded_untracked_paths": paths,
                "command_result": result.output,
            }))
        } else {
            result
        }
    }

    pub(crate) async fn git_status(&self, project: String) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
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
                        command: "git status --porcelain".to_string(),
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
            match tokio::time::timeout(Duration::from_secs(34), rx).await {
                Ok(Ok(resp)) => ToolResult::ok(json!({
                    "stdout": resp.stdout,
                    "stderr": resp.stderr,
                    "exit_code": resp.exit_code,
                })),
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    ToolResult::err("request dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    ToolResult::err("timed out")
                }
            }
        } else {
            let root = proj.root();
            let result = tokio::task::spawn_blocking(move || {
                run_command_sync("git status --porcelain", &root, 30)
            })
            .await;
            match result {
                Ok((exit_code, stdout, stderr, _)) => ToolResult::ok(json!({
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": exit_code,
                })),
                Err(e) => ToolResult::err(format!("task join error: {}", e)),
            }
        }
    }

    pub(crate) async fn git_diff(&self, project: String, args: Option<Vec<String>>) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let diff_args = args.unwrap_or_default();
        let cmd = if diff_args.is_empty() {
            "git diff".to_string()
        } else {
            let escaped: Vec<String> = diff_args.iter().map(|a| shell_escape_simple(a)).collect();
            format!("git diff -- {}", escaped.join(" "))
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
            match tokio::time::timeout(Duration::from_secs(34), rx).await {
                Ok(Ok(resp)) => ToolResult::ok(json!({
                    "stdout": resp.stdout,
                    "stderr": resp.stderr,
                    "exit_code": resp.exit_code,
                })),
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    ToolResult::err("request dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    ToolResult::err("timed out")
                }
            }
        } else {
            let root = proj.root();
            let result =
                tokio::task::spawn_blocking(move || run_command_sync(&cmd, &root, 30)).await;
            match result {
                Ok((exit_code, stdout, stderr, _)) => ToolResult::ok(json!({
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": exit_code,
                })),
                Err(e) => ToolResult::err(format!("task join error: {}", e)),
            }
        }
    }

    pub(crate) async fn git_diff_hunks(
        &self,
        project: String,
        paths: Option<Vec<String>>,
        max_hunks: Option<usize>,
        max_hunk_lines: Option<usize>,
        cached: Option<bool>,
    ) -> ToolResult {
        let paths = match clean_optional_paths(paths) {
            Ok(paths) => paths,
            Err(e) => return ToolResult::err(e),
        };
        let max_hunks = max_hunks
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_MAX_HUNKS)
            .min(MAX_MAX_HUNKS);
        let max_hunk_lines = max_hunk_lines
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_MAX_HUNK_LINES)
            .min(MAX_MAX_HUNK_LINES);
        let cached = cached.unwrap_or(false);
        let command = match git_diff_hunks_command(&paths, cached) {
            Ok(command) => command,
            Err(e) => return ToolResult::err(e),
        };
        let output = match self
            .run_project_command_capture(&project, command, 30, None)
            .await
        {
            Ok(output) => output,
            Err(e) => return ToolResult::err(e),
        };
        let (files, hunk_count, truncated) =
            parse_git_diff_hunks(&output.stdout, max_hunks, max_hunk_lines);
        let success = output.exit_code == Some(0);
        let payload = json!({
            "project": project,
            "paths": paths,
            "cached": cached,
            "files": files,
            "hunk_count": hunk_count,
            "truncated": truncated,
            "exit_code": output.exit_code,
            "stderr": output.stderr,
        });
        if success {
            ToolResult::ok(payload)
        } else {
            ToolResult {
                success: false,
                output: payload,
                error: Some("git diff failed".to_string()),
            }
        }
    }

    pub(crate) async fn git_log(
        &self,
        project: String,
        limit: Option<usize>,
        skip: Option<usize>,
    ) -> ToolResult {
        let limit = normalize_git_log_limit(limit);
        let skip = normalize_git_log_skip(skip);
        let command = git_log_command(limit, skip);
        let output = match self
            .run_project_command_capture(&project, command, 30, None)
            .await
        {
            Ok(output) => output,
            Err(e) => return ToolResult::err(e),
        };
        let (commits, truncated) = parse_git_log_commits(&output.stdout, limit);
        let payload = json!({
            "project": project,
            "limit": limit,
            "skip": skip,
            "count": commits.len(),
            "truncated": truncated,
            "commits": commits,
        });
        if output.exit_code == Some(0) || git_log_empty_repo(&output.stderr) {
            ToolResult::ok(payload)
        } else {
            ToolResult {
                success: false,
                output: json!({
                    "project": payload["project"],
                    "limit": payload["limit"],
                    "skip": payload["skip"],
                    "count": payload["count"],
                    "truncated": payload["truncated"],
                    "commits": payload["commits"],
                    "exit_code": output.exit_code,
                    "stderr": output.stderr,
                }),
                error: Some("git log failed".to_string()),
            }
        }
    }

    pub(crate) async fn git_diff_summary(&self, project: String) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let cmd = git_diff_summary_command();
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
                    let (porcelain, diff_stat) = split_diff_summary(&stdout);
                    let porcelain_summary = parse_porcelain_summary(&porcelain);
                    ToolResult::ok(json!({
                        "porcelain": porcelain,
                        "diff_stat": diff_stat,
                        "changed_files": porcelain_summary.changed_files,
                        "changed_files_count": porcelain_summary.changed_files_count,
                        "tracked_changed_files": porcelain_summary.tracked_changed_files,
                        "untracked_files": porcelain_summary.untracked_files,
                        "ignored_files": porcelain_summary.ignored_files,
                        "exit_code": resp.exit_code,
                    }))
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
                let (porcelain, diff_stat) = split_diff_summary(&stdout);
                let porcelain_summary = parse_porcelain_summary(&porcelain);
                ToolResult::ok(json!({
                    "porcelain": porcelain,
                    "diff_stat": diff_stat,
                    "changed_files": porcelain_summary.changed_files,
                    "changed_files_count": porcelain_summary.changed_files_count,
                    "tracked_changed_files": porcelain_summary.tracked_changed_files,
                    "untracked_files": porcelain_summary.untracked_files,
                    "ignored_files": porcelain_summary.ignored_files,
                    "exit_code": exit_code,
                }))
            }
            Err(e) => ToolResult::err(format!("task join error: {}", e)),
        }
    }

    pub(crate) async fn show_changes(
        &self,
        project: String,
        session_id: Option<String>,
        include_diff: Option<bool>,
        max_hunks: Option<usize>,
        max_hunk_lines: Option<usize>,
        session_event_limit: Option<usize>,
    ) -> ToolResult {
        let include_diff = include_diff.unwrap_or(false);
        let max_hunks = max_hunks
            .filter(|n| *n > 0)
            .unwrap_or(SHOW_CHANGES_DEFAULT_MAX_HUNKS)
            .min(SHOW_CHANGES_MAX_HUNKS);
        let max_hunk_lines = max_hunk_lines
            .filter(|n| *n > 0)
            .unwrap_or(SHOW_CHANGES_DEFAULT_MAX_HUNK_LINES)
            .min(SHOW_CHANGES_MAX_HUNK_LINES);
        let session_event_limit = session_event_limit
            .filter(|n| *n > 0)
            .unwrap_or(SHOW_CHANGES_DEFAULT_SESSION_EVENT_LIMIT)
            .min(SHOW_CHANGES_MAX_SESSION_EVENT_LIMIT);
        let command = show_changes_command(include_diff);
        let output = match self
            .run_project_command_capture(&project, command, 30, None)
            .await
        {
            Ok(output) => output,
            Err(e) => return ToolResult::err(e),
        };
        let (status_stdout, head_stdout, diff_stat, diff_stdout, untracked_preview_stdout) =
            split_show_changes_stdout(&output.stdout, include_diff);
        // Graceful degradation for non-git projects: when the project directory
        // is not inside a git repository, git prints a noisy fatal (and
        // `git diff` dumps its full --no-index usage) once per subcommand and
        // exits non-zero. Rather than surfacing that as a runtime failure with
        // full stderr, return a structured success payload that marks
        // git-backed inspection as unavailable while still reporting the
        // session sub-summary. Real git repositories are unaffected.
        if is_non_git_project_inspection(output.exit_code, &output.stderr, &status_stdout) {
            let mut payload =
                non_git_show_changes_payload(&project, output.exit_code, include_diff);
            let session_summary = session_id
                .as_deref()
                .and_then(|id| self.sessions.summary(id, Some(session_event_limit)));
            apply_show_changes_session(&mut payload, session_id.as_deref(), session_summary);
            return ToolResult::ok(payload);
        }
        let mut payload = parse_show_changes_output(
            &project,
            &status_stdout,
            &head_stdout,
            &diff_stat,
            include_diff.then_some(diff_stdout.as_str()),
            max_hunks,
            max_hunk_lines,
            output.exit_code,
            &output.stderr,
        );
        if include_diff {
            let untracked_paths = show_changes_untracked_paths(&payload);
            let (previews, truncated) = self
                .collect_show_changes_untracked_previews(&project, &untracked_paths)
                .await;
            payload["untracked_previews"] = json!(previews);
            payload["untracked_previews_truncated"] = json!(truncated);
            if !untracked_preview_stdout.trim().is_empty() {
                apply_show_changes_untracked_previews(&mut payload, &untracked_preview_stdout);
            }
        }
        let session_summary = session_id
            .as_deref()
            .and_then(|id| self.sessions.summary(id, Some(session_event_limit)));
        apply_show_changes_session(&mut payload, session_id.as_deref(), session_summary);
        if output.exit_code == Some(0) {
            ToolResult::ok(payload)
        } else {
            ToolResult {
                success: false,
                output: payload,
                error: Some("show_changes git inspection failed".to_string()),
            }
        }
    }
}
