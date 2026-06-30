use serde_json::{json, Value};
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

const SHOW_CHANGES_UNTRACKED_PREVIEW_SCRIPT: &str = r#"
import json, os, stat, subprocess, sys
MAX_FILES = 5
MAX_BYTES = 8192
MAX_LINES = 40

def emit(obj):
    sys.stdout.write(json.dumps(obj, ensure_ascii=False))

def skipped(path, reason, byte_count=None):
    obj = {"path": path, "kind": "skipped", "reason": reason}
    if byte_count is not None:
        obj["byte_count"] = byte_count
    return obj

def decode_path(raw):
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return raw.decode("utf-8", "backslashreplace")

def path_is_sensitive(path):
    parts = [part.lower() for part in path.replace("\\", "/").split("/") if part and part != "."]
    for part in parts:
        if part in [".git", "target", "node_modules", "projects.d", "agent.toml", "webcodex.env", ".env", "secrets", "tokens"]:
            return True
        if part.startswith(".env") or part.startswith("agent.toml") or part.startswith("webcodex.env"):
            return True
        if part in ["id_rsa", "id_ed25519"] or part.endswith(".pem") or part.endswith(".key"):
            return True
    return False

def path_is_invalid(path):
    if not path or "\x00" in path or os.path.isabs(path):
        return True
    parts = [part for part in path.replace("\\", "/").split("/") if part]
    return any(part == ".." for part in parts)

try:
    status = subprocess.check_output(["git", "status", "--porcelain=v1", "-z"], stderr=subprocess.DEVNULL)
except Exception:
    emit({"previews": [], "truncated": False})
    sys.exit(0)

raw_paths = []
for entry in status.split(b"\x00"):
    if entry.startswith(b"?? "):
        raw_paths.append(entry[3:])

root = os.path.realpath(".")
previews = []
for raw_path in raw_paths[:MAX_FILES]:
    try:
        path = raw_path.decode("utf-8")
    except UnicodeDecodeError:
        previews.append(skipped(decode_path(raw_path), "binary_or_non_utf8"))
        continue
    if path_is_invalid(path) or path_is_sensitive(path):
        previews.append(skipped(path, "sensitive_or_excluded_path"))
        continue
    full_path = os.path.abspath(os.path.join(root, path))
    if full_path != root and not full_path.startswith(root + os.sep):
        previews.append(skipped(path, "sensitive_or_excluded_path"))
        continue
    try:
        real_path = os.path.realpath(full_path)
    except OSError:
        previews.append(skipped(path, "sensitive_or_excluded_path"))
        continue
    if real_path != root and not real_path.startswith(root + os.sep):
        previews.append(skipped(path, "sensitive_or_excluded_path"))
        continue
    try:
        st = os.lstat(full_path)
    except OSError:
        previews.append(skipped(path, "not_found"))
        continue
    if stat.S_ISLNK(st.st_mode):
        previews.append(skipped(path, "sensitive_or_excluded_path"))
        continue
    if not stat.S_ISREG(st.st_mode):
        previews.append(skipped(path, "not_regular_file"))
        continue
    byte_count = int(st.st_size)
    if byte_count > MAX_BYTES:
        previews.append(skipped(path, "too_large", byte_count))
        continue
    try:
        with open(full_path, "rb") as f:
            data = f.read(MAX_BYTES + 1)
    except OSError:
        previews.append(skipped(path, "read_error"))
        continue
    if len(data) > MAX_BYTES:
        previews.append(skipped(path, "too_large", max(byte_count, len(data))))
        continue
    if b"\x00" in data or any(byte < 32 and byte not in (9, 10, 13) for byte in data):
        previews.append(skipped(path, "binary_or_non_utf8", len(data)))
        continue
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        previews.append(skipped(path, "binary_or_non_utf8", len(data)))
        continue
    all_lines = text.splitlines()
    shown_lines = all_lines[:MAX_LINES]
    previews.append({
        "path": path,
        "kind": "text",
        "line_count": len(all_lines),
        "byte_count": len(data),
        "truncated": len(all_lines) > MAX_LINES,
        "lines": [{"line": index + 1, "text": line} for index, line in enumerate(shown_lines)],
    })

emit({"previews": previews, "truncated": len(raw_paths) > MAX_FILES, "limit": MAX_FILES})
"#;

/// Build the read-only `git_diff_summary` command. Runs `git status
/// --porcelain` and `git diff --stat` separated by a unique sentinel. No
/// mutating git subcommand is emitted.
pub(crate) fn git_diff_summary_command() -> String {
    format!(
        "git status --porcelain; printf '\\n{sentinel}\\n'; git diff --stat",
        sentinel = DIFF_SUMMARY_SENTINEL,
    )
}

/// Build the read-only `show_changes` command. It combines the minimal git
/// inspections needed for a model-facing worktree summary. The optional full
/// diff is only emitted when the caller asks for bounded hunks.
pub(crate) fn show_changes_command(include_diff: bool) -> String {
    let diff_part = if include_diff {
        let preview_command = format!(
            "python3 -c {} 2>/dev/null || printf '[]\\n'",
            shell_escape_simple(SHOW_CHANGES_UNTRACKED_PREVIEW_SCRIPT),
        );
        format!(
            "; printf '\\n{sentinel}\\n'; \
             git diff --unified=80; \
             show_changes_status=$?; \
             printf '\\n{sentinel}\\n'; \
             {preview_command}; \
             exit $show_changes_status",
            sentinel = SHOW_CHANGES_SENTINEL,
            preview_command = preview_command,
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
            apply_show_changes_untracked_previews(&mut payload, &untracked_preview_stdout);
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
