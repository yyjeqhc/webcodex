use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

use super::helpers::{
    run_command_sync, shell_escape_simple, shell_join_paths, validate_limited_cleanup_paths,
    validate_project_relative_path,
};
use super::types::ToolResult;
use super::ToolRuntime;
use crate::shell_protocol::{ShellFileOpRequest, ShellRunRequest};

pub(crate) fn read_file_content_result(
    content: String,
    start_line: Option<usize>,
    limit: Option<usize>,
) -> ToolResult {
    let all_lines: Vec<&str> = content.lines().collect();
    let total_lines = all_lines.len();
    let eff_start = start_line.unwrap_or(1).max(1);
    let eff_limit = limit.unwrap_or(2000).clamp(1, 2000);
    if eff_start > total_lines {
        return ToolResult::ok(json!({
            "content": "",
            "total_lines": total_lines,
            "start_line": eff_start,
            "limit": eff_limit,
        }));
    }
    let start_idx = eff_start - 1;
    let end_idx = (start_idx + eff_limit).min(total_lines);
    let slice = all_lines[start_idx..end_idx].join("\n");
    ToolResult::ok(json!({
        "content": slice,
        "total_lines": total_lines,
        "start_line": eff_start,
        "limit": eff_limit,
    }))
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

/// Maximum accepted size for a single `replace_in_file` `old`/`new` field.
/// Generous for text edits while bounding memory and the agent stdin payload.
pub(crate) const MAX_REPLACE_FIELD_BYTES: usize = 256 * 1024; // 256 KiB

/// Maximum accepted size for `write_project_file` `content`. Bounded by the
/// agent run-shell stdin transport (`RUN_HELPER_STDIN_BUDGET`).
pub(crate) const MAX_WRITE_CONTENT_BYTES: usize = 256 * 1024; // 256 KiB

/// Hard cap on the serialized helper payload sent to the agent over the
/// run-shell stdin transport. Mirrors `MAX_RUN_STDIN_BYTES` in shell_client
/// without coupling this module to that private constant.
pub(crate) const RUN_HELPER_STDIN_BUDGET: usize = 512 * 1024; // 512 KiB

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
    ) -> ToolResult {
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
                        expected_sha256: None,
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
                    read_file_content_result(resp.stdout.unwrap_or_default(), start_line, limit)
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
        read_file_content_result(content, start_line, limit)
    }

    // -------------------------------------------------------------------------
    // Phase A read-only console tools
    // -------------------------------------------------------------------------

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
                        expected_sha256: None,
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
        let cmd = search_project_text_command(&pattern, &rel_path, max_matches);
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
                    let (matches, truncated) = parse_search_matches(&stdout, max_matches);
                    ToolResult::ok(json!({
                        "project": project,
                        "pattern": pattern,
                        "path": rel_path,
                        "matches": matches,
                        "count": matches.len(),
                        "truncated": truncated,
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
                let (matches, truncated) = parse_search_matches(&stdout, max_matches);
                ToolResult::ok(json!({
                    "project": project,
                    "pattern": pattern,
                    "path": rel_path,
                    "matches": matches,
                    "count": matches.len(),
                    "truncated": truncated,
                    "exit_code": exit_code,
                }))
            }
            Err(e) => ToolResult::err(format!("task join error: {}", e)),
        }
    }
}
