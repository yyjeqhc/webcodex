use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::helpers::shell_escape_simple;
use super::types::ToolResult;
use super::ToolRuntime;
use crate::projects::ProjectConfig;

const CHECKPOINT_VERSION: u32 = 1;
const CHECKPOINT_ID_PREFIX: &str = "wc_ckpt_";
const DEFAULT_LIST_LIMIT: usize = 20;
const MAX_LIST_LIMIT: usize = 100;

const CHECKPOINT_AGENT_HELPER_WRAPPER: &str = r#"
import io, json, sys

outer = json.loads(sys.stdin.read() or "{}")
script = outer.get("script")
payload = outer.get("payload") or {}
if not isinstance(script, str):
    sys.stdout.write(json.dumps({"error_kind": "invalid_helper_payload", "error": "checkpoint helper script is required"}))
    sys.exit(0)
sys.stdin = io.StringIO(json.dumps(payload, ensure_ascii=False))
namespace = {"__name__": "__main__"}
exec(compile(script, "<webcodex_checkpoint_helper>", "exec"), namespace, namespace)
"#;

const CHECKPOINT_CREATE_HELPER: &str = r#"
import hashlib, json, os, stat, subprocess, sys

MAX_DIFF_BYTES = 1024 * 1024
MAX_DIFF_STAT_BYTES = 64 * 1024
MAX_UNTRACKED_BYTES = 256 * 1024
MAX_UNTRACKED_TOTAL_BYTES = 1024 * 1024
MAX_UNTRACKED_FILES = 64

def emit(obj):
    sys.stdout.write(json.dumps(obj, ensure_ascii=False))

def fail(kind, message, **extra):
    obj = {"error_kind": kind, "error": message}
    obj.update(extra)
    emit(obj)
    sys.exit(0)

def git(args, input_bytes=None, check=False):
    try:
        proc = subprocess.run(
            ["git"] + args,
            input=input_bytes,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except Exception as exc:
        fail("git_exec_failed", str(exc))
    if check and proc.returncode != 0:
        fail(
            "git_failed",
            "git " + " ".join(args) + " failed",
            exit_code=proc.returncode,
            stderr=proc.stderr.decode("utf-8", "replace")[:4000],
        )
    return proc

def decode_utf8(data, label):
    if b"\x00" in data:
        fail("binary_or_non_utf8_diff", label + " contains NUL bytes")
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        fail("binary_or_non_utf8_diff", label + " is not valid UTF-8")

def bounded_git_text(args, label, max_bytes, check=True):
    proc = git(args, check=check)
    data = proc.stdout
    if len(data) > max_bytes:
        fail(
            "checkpoint_too_large",
            label + " exceeds checkpoint v1 byte limit",
            byte_count=len(data),
            max_bytes=max_bytes,
        )
    text = decode_utf8(data, label)
    if "\nBinary files " in text or text.startswith("Binary files "):
        fail("unsupported_binary_diff", label + " contains binary file changes; checkpoint v1 stores text diffs only")
    return text

def branch_name():
    proc = git(["branch", "--show-current"])
    name = proc.stdout.decode("utf-8", "replace").strip()
    if name:
        return name
    proc = git(["rev-parse", "--abbrev-ref", "HEAD"])
    name = proc.stdout.decode("utf-8", "replace").strip()
    return None if name == "HEAD" or not name else name

def status_summary(status):
    counts = {
        "modified": 0,
        "added": 0,
        "deleted": 0,
        "renamed": 0,
        "copied": 0,
        "untracked": 0,
        "staged": 0,
        "unstaged": 0,
    }
    files = []
    for line in status.splitlines():
        if len(line) < 3:
            continue
        x = line[0]
        y = line[1]
        path = line[3:].strip().strip('"')
        if " -> " in path:
            path = path.split(" -> ", 1)[1].strip().strip('"')
        if not path:
            continue
        if x == "?" and y == "?":
            label = "untracked"
            counts["untracked"] += 1
            staged = False
            unstaged = False
        else:
            staged = x not in (" ", "?")
            unstaged = y not in (" ", "?")
            if staged:
                counts["staged"] += 1
            if unstaged:
                counts["unstaged"] += 1
            code = x if x not in (" ", "?") else y
            if code == "A":
                label = "added"
                counts["added"] += 1
            elif code == "D":
                label = "deleted"
                counts["deleted"] += 1
            elif code == "R":
                label = "renamed"
                counts["renamed"] += 1
            elif code == "C":
                label = "copied"
                counts["copied"] += 1
            else:
                label = "modified"
                counts["modified"] += 1
        files.append({"path": path, "status": label, "staged": staged, "unstaged": unstaged})
    counts["files"] = len(files)
    return {"counts": counts, "files": files, "clean": len(files) == 0}

def invalid_rel_path(path):
    if not path or "\x00" in path or os.path.isabs(path):
        return True
    parts = [p for p in path.replace("\\", "/").split("/") if p]
    return any(p == ".." for p in parts)

def sensitive_path(path):
    parts = [p.lower() for p in path.replace("\\", "/").split("/") if p and p != "."]
    for part in parts:
        if part in [".git", "target", "node_modules", "projects.d", "agent.toml", "webcodex.env", ".env", ".npmrc", ".netrc", "secrets", "secret", "tokens", "token", "credentials", "credential", "passwords", "password"]:
            return True
        if part.startswith(".env") or part.startswith("agent.toml") or part.startswith("webcodex.env"):
            return True
        if any(marker in part for marker in ["secret", "token", "credential", "password"]):
            return True
        if part in ["id_rsa", "id_ed25519"] or part.endswith(".pem") or part.endswith(".key"):
            return True
    return False

def skipped(path, reason, byte_count=None):
    obj = {"path": path, "reason": reason}
    if byte_count is not None:
        obj["byte_count"] = byte_count
    return obj

def binaryish(data):
    if b"\x00" in data:
        return True
    return any(byte < 32 and byte not in (9, 10, 13) for byte in data)

def untracked_paths_from_status_z(raw):
    out = []
    for entry in raw.split(b"\x00"):
        if entry.startswith(b"?? "):
            raw_path = entry[3:]
            try:
                out.append(raw_path.decode("utf-8"))
            except UnicodeDecodeError:
                out.append(raw_path.decode("utf-8", "backslashreplace"))
    return out

def collect_untracked(include_untracked):
    raw = git(["status", "--porcelain=v1", "-z", "--untracked-files=all"], check=True).stdout
    raw_paths = untracked_paths_from_status_z(raw)
    if not include_untracked:
        return [], [skipped(path, "include_untracked_false") for path in raw_paths]
    root = os.path.realpath(".")
    files = []
    skipped_files = []
    total = 0
    for path in raw_paths:
        if len(files) >= MAX_UNTRACKED_FILES:
            skipped_files.append(skipped(path, "too_many_untracked_files"))
            continue
        if invalid_rel_path(path) or sensitive_path(path):
            skipped_files.append(skipped(path, "sensitive_or_invalid_path"))
            continue
        full = os.path.abspath(os.path.join(root, path))
        real = os.path.realpath(full)
        if real != root and not real.startswith(root + os.sep):
            skipped_files.append(skipped(path, "path_escapes_project"))
            continue
        try:
            st = os.lstat(full)
        except OSError:
            skipped_files.append(skipped(path, "not_found"))
            continue
        if stat.S_ISLNK(st.st_mode):
            skipped_files.append(skipped(path, "symlink"))
            continue
        if not stat.S_ISREG(st.st_mode):
            skipped_files.append(skipped(path, "not_regular_file"))
            continue
        byte_count = int(st.st_size)
        if byte_count > MAX_UNTRACKED_BYTES:
            skipped_files.append(skipped(path, "too_large", byte_count))
            continue
        if total + byte_count > MAX_UNTRACKED_TOTAL_BYTES:
            skipped_files.append(skipped(path, "total_untracked_budget_exceeded", byte_count))
            continue
        try:
            with open(full, "rb") as fh:
                data = fh.read(MAX_UNTRACKED_BYTES + 1)
        except OSError:
            skipped_files.append(skipped(path, "read_error"))
            continue
        if len(data) > MAX_UNTRACKED_BYTES:
            skipped_files.append(skipped(path, "too_large", len(data)))
            continue
        if binaryish(data):
            skipped_files.append(skipped(path, "binary_or_non_utf8", len(data)))
            continue
        try:
            content = data.decode("utf-8")
        except UnicodeDecodeError:
            skipped_files.append(skipped(path, "binary_or_non_utf8", len(data)))
            continue
        total += len(data)
        files.append({
            "path": path,
            "content": content,
            "byte_count": len(data),
            "sha256": hashlib.sha256(data).hexdigest(),
        })
    return files, skipped_files

payload = json.loads(sys.stdin.read() or "{}")
include_untracked = bool(payload.get("include_untracked", False))

head = git(["rev-parse", "HEAD"], check=True).stdout.decode("utf-8", "replace").strip()
branch = branch_name()
status = bounded_git_text(["status", "--porcelain=v1", "--untracked-files=all"], "git status", 256 * 1024)
tracked_diff = bounded_git_text(["diff", "--no-ext-diff", "--"], "tracked diff", MAX_DIFF_BYTES)
staged_diff = bounded_git_text(["diff", "--cached", "--no-ext-diff", "--"], "staged diff", MAX_DIFF_BYTES)
diff_stat = bounded_git_text(["diff", "--stat", "--"], "tracked diff stat", MAX_DIFF_STAT_BYTES)
staged_diff_stat = bounded_git_text(["diff", "--cached", "--stat", "--"], "staged diff stat", MAX_DIFF_STAT_BYTES)
untracked_files, skipped_files = collect_untracked(include_untracked)

emit({
    "format": "webcodex.workspace_checkpoint.v1",
    "version": 1,
    "head": head,
    "branch": branch,
    "status_porcelain": status,
    "status_summary": status_summary(status),
    "tracked_diff": tracked_diff,
    "staged_diff": staged_diff,
    "tracked_diff_bytes": len(tracked_diff.encode("utf-8")),
    "staged_diff_bytes": len(staged_diff.encode("utf-8")),
    "diff_stat": diff_stat,
    "staged_diff_stat": staged_diff_stat,
    "untracked_files": untracked_files,
    "skipped_files": skipped_files,
    "complete": True,
    "limitations": ["text_diffs_only", "ignored_files_excluded", "large_binary_secret_like_untracked_files_skipped"],
})
"#;

const CHECKPOINT_RESTORE_HELPER: &str = r#"
import hashlib, json, os, subprocess, sys

MAX_DIFF_BYTES = 1024 * 1024

def emit(obj):
    sys.stdout.write(json.dumps(obj, ensure_ascii=False))

def fail(kind, message, **extra):
    obj = {"error_kind": kind, "error": message}
    obj.update(extra)
    emit(obj)
    sys.exit(0)

def git(args, input_text=None, check=False):
    try:
        proc = subprocess.run(
            ["git"] + args,
            input=None if input_text is None else input_text.encode("utf-8"),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except Exception as exc:
        if check:
            raise RuntimeError(str(exc))
        fail("git_exec_failed", str(exc))
    if check and proc.returncode != 0:
        stderr = proc.stderr.decode("utf-8", "replace")[:4000]
        raise RuntimeError("git " + " ".join(args) + " failed: " + stderr)
    return proc

def git_text(args, label):
    proc = git(args, check=True)
    data = proc.stdout
    if len(data) > MAX_DIFF_BYTES:
        fail("current_diff_too_large", label + " is too large to restore safely", byte_count=len(data), max_bytes=MAX_DIFF_BYTES)
    if b"\x00" in data:
        fail("unsupported_current_binary_diff", label + " contains NUL bytes")
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        fail("unsupported_current_binary_diff", label + " is not valid UTF-8")
    if "\nBinary files " in text or text.startswith("Binary files "):
        fail("unsupported_current_binary_diff", label + " contains binary file changes")
    return text

def apply_patch(args, patch):
    if not patch:
        return
    git(["apply"] + args + ["-"], input_text=patch, check=True)

def invalid_rel_path(path):
    if not path or "\x00" in path or os.path.isabs(path):
        return True
    parts = [p for p in path.replace("\\", "/").split("/") if p]
    return any(p == ".." for p in parts)

def file_sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()

def changed_paths_from_diff(diff):
    paths = []
    for line in diff.splitlines():
        if line.startswith("diff --git "):
            marker = " b/"
            pos = line.rfind(marker)
            if pos >= 0:
                path = line[pos + len(marker):]
                if path and path != "/dev/null" and path not in paths:
                    paths.append(path)
            continue
        for prefix in ("+++ b/", "--- a/"):
            if line.startswith(prefix):
                path = line[len(prefix):]
                if path and path != "/dev/null" and path not in paths:
                    paths.append(path)
    return paths

payload = json.loads(sys.stdin.read() or "{}")
checkpoint = payload.get("checkpoint") or {}
checkpoint_id = checkpoint.get("checkpoint_id")
expected_head = checkpoint.get("head")
if not checkpoint_id or not expected_head:
    fail("invalid_checkpoint", "checkpoint_id and head are required")

head = git(["rev-parse", "HEAD"], check=True).stdout.decode("utf-8", "replace").strip()
if head != expected_head:
    fail("head_mismatch", "current HEAD does not match checkpoint head", current_head=head, checkpoint_head=expected_head)

tracked_diff = checkpoint.get("tracked_diff") or ""
staged_diff = checkpoint.get("staged_diff") or ""
untracked_files = checkpoint.get("untracked_files") or []
if not isinstance(untracked_files, list):
    fail("invalid_checkpoint", "untracked_files must be an array")

root = os.path.realpath(".")
for item in untracked_files:
    path = item.get("path") if isinstance(item, dict) else None
    content = item.get("content") if isinstance(item, dict) else None
    if not isinstance(path, str) or not isinstance(content, str) or invalid_rel_path(path):
        fail("invalid_checkpoint", "checkpoint contains invalid untracked file entry")
    full = os.path.abspath(os.path.join(root, path))
    real = os.path.realpath(full)
    if real != root and not real.startswith(root + os.sep):
        fail("invalid_checkpoint", "checkpoint untracked path escapes project", path=path)
    if os.path.exists(full):
        current = file_sha256(full)
        expected = item.get("sha256") or hashlib.sha256(content.encode("utf-8")).hexdigest()
        if current != expected:
            fail("untracked_conflict", "current file differs from checkpoint untracked content", path=path)

current_unstaged = git_text(["diff", "--no-ext-diff", "--"], "current unstaged diff")
current_staged = git_text(["diff", "--cached", "--no-ext-diff", "--"], "current staged diff")

try:
    if current_unstaged:
        apply_patch(["--reverse", "--check"], current_unstaged)
    if current_staged:
        apply_patch(["--reverse", "--cached", "--check"], current_staged)
except Exception as exc:
    fail("unsafe_current_state", "current tracked changes cannot be safely reversed", detail=str(exc))

applied_checkpoint_staged_index = False
applied_checkpoint_staged_worktree = False
applied_checkpoint_unstaged = False
created_untracked = []

def reapply_current():
    if current_staged:
        apply_patch(["--cached"], current_staged)
        apply_patch([], current_staged)
    if current_unstaged:
        apply_patch([], current_unstaged)

def rollback_checkpoint():
    for path in reversed(created_untracked):
        try:
            os.remove(path)
        except OSError:
            pass
    if applied_checkpoint_unstaged:
        apply_patch(["--reverse"], tracked_diff)
    if applied_checkpoint_staged_worktree:
        apply_patch(["--reverse"], staged_diff)
    if applied_checkpoint_staged_index:
        apply_patch(["--reverse", "--cached"], staged_diff)

try:
    if current_unstaged:
        apply_patch(["--reverse"], current_unstaged)
    if current_staged:
        apply_patch(["--reverse", "--cached"], current_staged)
        apply_patch(["--reverse"], current_staged)

    if staged_diff:
        apply_patch(["--cached", "--check"], staged_diff)
        apply_patch(["--cached"], staged_diff)
        applied_checkpoint_staged_index = True
        apply_patch(["--check"], staged_diff)
        apply_patch([], staged_diff)
        applied_checkpoint_staged_worktree = True
    if tracked_diff:
        apply_patch(["--check"], tracked_diff)
        apply_patch([], tracked_diff)
        applied_checkpoint_unstaged = True

    for item in untracked_files:
        rel = item["path"]
        content = item["content"]
        full = os.path.abspath(os.path.join(root, rel))
        if os.path.exists(full):
            continue
        parent = os.path.dirname(full)
        if parent:
            os.makedirs(parent, exist_ok=True)
        with open(full, "w", encoding="utf-8", newline="") as fh:
            fh.write(content)
        created_untracked.append(full)
except Exception as exc:
    rollback_ok = True
    try:
        rollback_checkpoint()
        reapply_current()
    except Exception:
        rollback_ok = False
    fail("restore_failed", "checkpoint restore failed", detail=str(exc), rolled_back=rollback_ok)

changed_paths = []
for path in changed_paths_from_diff(staged_diff) + changed_paths_from_diff(tracked_diff):
    if path not in changed_paths:
        changed_paths.append(path)
for item in untracked_files:
    path = item.get("path")
    if path and path not in changed_paths:
        changed_paths.append(path)

emit({
    "restored": True,
    "checkpoint_id": checkpoint_id,
    "changed_paths": changed_paths,
    "warnings": [],
})
"#;

#[derive(Debug, Clone)]
pub(crate) struct CheckpointStore {
    state_dir: PathBuf,
}

impl CheckpointStore {
    pub(crate) fn new(state_dir: impl Into<PathBuf>) -> Self {
        Self {
            state_dir: state_dir.into(),
        }
    }

    #[cfg(test)]
    pub(crate) fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    fn project_dir(&self, resolved_project: &str) -> PathBuf {
        self.state_dir
            .join("checkpoints")
            .join(safe_project_id(resolved_project))
    }

    fn checkpoint_path(
        &self,
        resolved_project: &str,
        checkpoint_id: &str,
    ) -> Result<PathBuf, String> {
        validate_checkpoint_id(checkpoint_id)?;
        Ok(self
            .project_dir(resolved_project)
            .join(format!("{checkpoint_id}.json")))
    }

    fn write(
        &self,
        resolved_project: &str,
        checkpoint_id: &str,
        checkpoint: &Value,
    ) -> Result<PathBuf, String> {
        let path = self.checkpoint_path(resolved_project, checkpoint_id)?;
        let parent = path
            .parent()
            .ok_or_else(|| "checkpoint path has no parent".to_string())?;
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create checkpoint dir: {err}"))?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("checkpoint.json");
        let tmp = path.with_file_name(format!(
            ".{file_name}.tmp-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let data = serde_json::to_vec_pretty(checkpoint)
            .map_err(|err| format!("failed to serialize checkpoint: {err}"))?;
        fs::write(&tmp, data)
            .and_then(|_| fs::rename(&tmp, &path))
            .map_err(|err| {
                let _ = fs::remove_file(&tmp);
                format!("failed to write checkpoint: {err}")
            })?;
        Ok(path)
    }

    fn load(
        &self,
        resolved_project: &str,
        checkpoint_id: &str,
    ) -> Result<(Value, PathBuf), String> {
        let path = self.checkpoint_path(resolved_project, checkpoint_id)?;
        let content =
            fs::read_to_string(&path).map_err(|err| format!("failed to read checkpoint: {err}"))?;
        let value: Value = serde_json::from_str(&content)
            .map_err(|err| format!("invalid checkpoint JSON: {err}"))?;
        Ok((value, path))
    }

    fn delete(&self, resolved_project: &str, checkpoint_id: &str) -> Result<PathBuf, String> {
        let path = self.checkpoint_path(resolved_project, checkpoint_id)?;
        fs::remove_file(&path).map_err(|err| format!("failed to delete checkpoint: {err}"))?;
        Ok(path)
    }

    fn list(&self, resolved_project: &str, limit: usize) -> Result<Vec<(Value, PathBuf)>, String> {
        let dir = self.project_dir(resolved_project);
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(format!("failed to list checkpoints: {err}")),
        };
        let mut checkpoints = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if validate_checkpoint_id(stem).is_err() {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<Value>(&content) else {
                continue;
            };
            checkpoints.push((value, path));
        }
        checkpoints.sort_by(|(a, _), (b, _)| {
            let a_time = a
                .get("created_at")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            let b_time = b
                .get("created_at")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            b_time
                .cmp(&a_time)
                .then_with(|| checkpoint_id_of(b).cmp(&checkpoint_id_of(a)))
        });
        checkpoints.truncate(limit);
        Ok(checkpoints)
    }
}

impl Default for CheckpointStore {
    fn default() -> Self {
        Self::new(crate::config::runtime_state_dir())
    }
}

impl ToolRuntime {
    pub fn with_checkpoint_state_dir(mut self, state_dir: impl Into<PathBuf>) -> Self {
        self.checkpoint_store = CheckpointStore::new(state_dir);
        self
    }

    #[cfg(test)]
    pub(crate) fn checkpoint_state_dir(&self) -> &Path {
        self.checkpoint_store.state_dir()
    }

    pub(crate) async fn workspace_checkpoint_create(
        &self,
        project: String,
        title: Option<String>,
        note: Option<String>,
        include_untracked: Option<bool>,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let checkpoint_id = format!("{CHECKPOINT_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let include_untracked = include_untracked.unwrap_or(false);
        let helper_output = match self
            .run_checkpoint_helper(
                &resolved.config,
                CHECKPOINT_CREATE_HELPER,
                json!({ "include_untracked": include_untracked }),
                60,
            )
            .await
        {
            Ok(value) => value,
            Err(err) => return ToolResult::err(err),
        };
        if helper_output.get("error").is_some() {
            return ToolResult {
                success: false,
                error: helper_output
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                output: helper_output,
            };
        }
        let mut checkpoint = helper_output;
        checkpoint["version"] = json!(CHECKPOINT_VERSION);
        checkpoint["checkpoint_id"] = json!(checkpoint_id);
        checkpoint["project"] = json!(project);
        checkpoint["project_input"] = json!(resolved.input);
        checkpoint["resolved_project"] = json!(resolved.resolved_id);
        checkpoint["title"] = json!(title);
        checkpoint["note"] = json!(note);
        checkpoint["include_untracked"] = json!(include_untracked);
        checkpoint["created_at"] = json!(chrono::Utc::now().timestamp());

        let storage_path = match self.checkpoint_store.write(
            checkpoint["resolved_project"].as_str().unwrap_or_default(),
            checkpoint["checkpoint_id"].as_str().unwrap_or_default(),
            &checkpoint,
        ) {
            Ok(path) => path,
            Err(err) => return ToolResult::err(err),
        };
        let mut output = checkpoint_summary(&checkpoint, SummaryMode::Create);
        output["storage_path"] = json!(storage_path);
        ToolResult::ok(output)
    }

    pub(crate) async fn workspace_checkpoint_list(
        &self,
        project: String,
        limit: Option<usize>,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT);
        let checkpoints = match self.checkpoint_store.list(&resolved.resolved_id, limit) {
            Ok(values) => values,
            Err(err) => return ToolResult::err(err),
        };
        let items: Vec<Value> = checkpoints
            .iter()
            .map(|(checkpoint, _)| checkpoint_summary(checkpoint, SummaryMode::List))
            .collect();
        ToolResult::ok(json!({
            "project": project,
            "resolved_project": resolved.resolved_id,
            "limit": limit,
            "checkpoints": items,
        }))
    }

    pub(crate) async fn workspace_checkpoint_show(
        &self,
        project: String,
        checkpoint_id: String,
        include_diff_stat: Option<bool>,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let (checkpoint, path) = match self
            .checkpoint_store
            .load(&resolved.resolved_id, &checkpoint_id)
        {
            Ok(value) => value,
            Err(err) => return ToolResult::err(err),
        };
        let mut output = checkpoint_summary(&checkpoint, SummaryMode::Show);
        output["storage_path"] = json!(path);
        if include_diff_stat.unwrap_or(false) {
            output["diff_stat"] = json!({
                "tracked": checkpoint.get("diff_stat").cloned().unwrap_or(Value::String(String::new())),
                "staged": checkpoint.get("staged_diff_stat").cloned().unwrap_or(Value::String(String::new())),
            });
        }
        ToolResult::ok(output)
    }

    pub(crate) async fn workspace_checkpoint_restore(
        &self,
        project: String,
        checkpoint_id: String,
        confirm: Option<bool>,
    ) -> ToolResult {
        if confirm != Some(true) {
            return ToolResult::err_with_output(
                "confirm must be true to restore a workspace checkpoint",
                json!({
                    "error_kind": "confirm_required",
                    "checkpoint_id": checkpoint_id,
                    "restored": false,
                }),
            );
        }
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let (checkpoint, _path) = match self
            .checkpoint_store
            .load(&resolved.resolved_id, &checkpoint_id)
        {
            Ok(value) => value,
            Err(err) => return ToolResult::err(err),
        };
        if checkpoint.get("version").and_then(Value::as_u64) != Some(CHECKPOINT_VERSION as u64) {
            return ToolResult::err("unsupported checkpoint version");
        }
        let helper_output = match self
            .run_checkpoint_helper(
                &resolved.config,
                CHECKPOINT_RESTORE_HELPER,
                json!({ "checkpoint": checkpoint }),
                60,
            )
            .await
        {
            Ok(value) => value,
            Err(err) => return ToolResult::err(err),
        };
        if helper_output.get("error").is_some() {
            return ToolResult {
                success: false,
                error: helper_output
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                output: helper_output,
            };
        }
        ToolResult::ok(json!({
            "restored": true,
            "checkpoint_id": checkpoint_id,
            "project": project,
            "resolved_project": resolved.resolved_id,
            "changed_paths": helper_output.get("changed_paths").cloned().unwrap_or_else(|| json!([])),
            "warnings": helper_output.get("warnings").cloned().unwrap_or_else(|| json!([])),
        }))
    }

    pub(crate) async fn workspace_checkpoint_delete(
        &self,
        project: String,
        checkpoint_id: String,
        confirm: Option<bool>,
    ) -> ToolResult {
        if confirm != Some(true) {
            return ToolResult::err_with_output(
                "confirm must be true to delete a workspace checkpoint",
                json!({
                    "error_kind": "confirm_required",
                    "checkpoint_id": checkpoint_id,
                    "deleted": false,
                }),
            );
        }
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let path = match self
            .checkpoint_store
            .delete(&resolved.resolved_id, &checkpoint_id)
        {
            Ok(path) => path,
            Err(err) => return ToolResult::err(err),
        };
        ToolResult::ok(json!({
            "deleted": true,
            "checkpoint_id": checkpoint_id,
            "project": project,
            "resolved_project": resolved.resolved_id,
            "storage_path": path,
        }))
    }

    async fn run_checkpoint_helper(
        &self,
        config: &ProjectConfig,
        script: &'static str,
        payload: Value,
        timeout_secs: u64,
    ) -> Result<Value, String> {
        if config.is_agent() {
            let client_id = config.agent_client_id()?.to_string();
            let command = format!(
                "python3 -c {}",
                shell_escape_simple(CHECKPOINT_AGENT_HELPER_WRAPPER)
            );
            return self
                .run_agent_helper(
                    client_id,
                    config.path.clone(),
                    command,
                    json!({
                        "script": script,
                        "payload": payload,
                    }),
                )
                .await;
        }
        let root = config
            .root()
            .canonicalize()
            .map_err(|err| format!("Project root does not exist: {err}"))?;
        let input = serde_json::to_vec(&payload)
            .map_err(|err| format!("failed to serialize checkpoint helper payload: {err}"))?;
        tokio::task::spawn_blocking(move || {
            run_local_python_helper(script, &root, &input, timeout_secs)
        })
        .await
        .map_err(|err| format!("task join error: {err}"))?
    }
}

#[derive(Debug, Clone, Copy)]
enum SummaryMode {
    Create,
    List,
    Show,
}

fn checkpoint_summary(checkpoint: &Value, mode: SummaryMode) -> Value {
    let untracked_files = checkpoint
        .get("untracked_files")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(|file| {
                    let path = file.get("path").and_then(Value::as_str)?;
                    Some(json!({
                        "path": path,
                        "byte_count": file.get("byte_count").cloned().unwrap_or(Value::Null),
                        "sha256": file.get("sha256").cloned().unwrap_or(Value::Null),
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let files = checkpoint_file_list(checkpoint);
    let mut output = json!({
        "checkpoint_id": checkpoint_id_of(checkpoint),
        "project": checkpoint.get("project").cloned().unwrap_or(Value::Null),
        "resolved_project": checkpoint.get("resolved_project").cloned().unwrap_or(Value::Null),
        "title": checkpoint.get("title").cloned().unwrap_or(Value::Null),
        "created_at": checkpoint.get("created_at").cloned().unwrap_or(Value::Null),
        "head": checkpoint.get("head").cloned().unwrap_or(Value::Null),
        "branch": checkpoint.get("branch").cloned().unwrap_or(Value::Null),
        "complete": checkpoint.get("complete").cloned().unwrap_or(Value::Bool(false)),
        "tracked_diff_bytes": checkpoint.get("tracked_diff_bytes").cloned().unwrap_or(Value::Number(0.into())),
        "staged_diff_bytes": checkpoint.get("staged_diff_bytes").cloned().unwrap_or(Value::Number(0.into())),
        "untracked_files": untracked_files,
        "untracked_file_count": checkpoint.get("untracked_files").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "skipped_files": checkpoint.get("skipped_files").cloned().unwrap_or_else(|| json!([])),
        "status_summary": checkpoint.get("status_summary").cloned().unwrap_or_else(|| json!({})),
    });
    match mode {
        SummaryMode::Create => {
            output["note"] = checkpoint.get("note").cloned().unwrap_or(Value::Null);
            output["include_untracked"] = checkpoint
                .get("include_untracked")
                .cloned()
                .unwrap_or(Value::Bool(false));
        }
        SummaryMode::List => {}
        SummaryMode::Show => {
            output["note"] = checkpoint.get("note").cloned().unwrap_or(Value::Null);
            output["files"] = json!(files);
            output["limitations"] = checkpoint
                .get("limitations")
                .cloned()
                .unwrap_or_else(|| json!([]));
        }
    }
    output
}

fn checkpoint_file_list(checkpoint: &Value) -> Vec<Value> {
    let mut files = Vec::new();
    for diff_key in ["staged_diff", "tracked_diff"] {
        if let Some(diff) = checkpoint.get(diff_key).and_then(Value::as_str) {
            for path in changed_paths_from_diff(diff) {
                if !files.iter().any(|file: &Value| {
                    file.get("path").and_then(Value::as_str) == Some(path.as_str())
                }) {
                    files.push(json!({
                        "path": path,
                        "kind": "tracked",
                    }));
                }
            }
        }
    }
    if let Some(untracked) = checkpoint.get("untracked_files").and_then(Value::as_array) {
        for item in untracked {
            let Some(path) = item.get("path").and_then(Value::as_str) else {
                continue;
            };
            if !files
                .iter()
                .any(|file| file.get("path").and_then(Value::as_str) == Some(path))
            {
                files.push(json!({
                    "path": path,
                    "kind": "untracked",
                }));
            }
        }
    }
    files
}

fn changed_paths_from_diff(diff: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some(pos) = rest.rfind(" b/") {
                let path = &rest[pos + 3..];
                push_unique(&mut paths, path);
            }
            continue;
        }
        for prefix in ["+++ b/", "--- a/"] {
            if let Some(path) = line.strip_prefix(prefix) {
                if path != "/dev/null" {
                    push_unique(&mut paths, path);
                }
            }
        }
    }
    paths
}

fn push_unique(paths: &mut Vec<String>, path: &str) {
    let path = path.trim();
    if path.is_empty() || paths.iter().any(|existing| existing == path) {
        return;
    }
    paths.push(path.to_string());
}

fn checkpoint_id_of(checkpoint: &Value) -> String {
    checkpoint
        .get("checkpoint_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn safe_project_id(project: &str) -> String {
    let mut safe = project
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if safe.is_empty() {
        safe.push_str("project");
    }
    safe.truncate(96);
    let mut hasher = Sha256::new();
    hasher.update(project.as_bytes());
    let digest = hasher.finalize();
    safe.push('_');
    for byte in digest.iter().take(6) {
        safe.push_str(&format!("{byte:02x}"));
    }
    safe
}

fn validate_checkpoint_id(checkpoint_id: &str) -> Result<(), String> {
    let Some(rest) = checkpoint_id.strip_prefix(CHECKPOINT_ID_PREFIX) else {
        return Err("checkpoint_id must start with wc_ckpt_".to_string());
    };
    if rest.is_empty() || rest.len() > 64 {
        return Err("checkpoint_id has invalid length".to_string());
    }
    if !rest
        .as_bytes()
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        return Err("checkpoint_id contains invalid characters".to_string());
    }
    Ok(())
}

fn run_local_python_helper(
    script: &'static str,
    cwd: &Path,
    input: &[u8],
    timeout_secs: u64,
) -> Result<Value, String> {
    let mut child = std::process::Command::new("python3")
        .arg("-c")
        .arg(script)
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to start checkpoint helper: {err}"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| "checkpoint helper stdin unavailable".to_string())?
        .write_all(input)
        .map_err(|err| format!("failed to write checkpoint helper input: {err}"))?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() >= Duration::from_secs(timeout_secs.max(1)) => {
                let _ = child.kill();
                let output = child.wait_with_output().map_err(|err| {
                    format!("checkpoint helper timed out and output collection failed: {err}")
                })?;
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("checkpoint helper timed out: {stderr}"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(err) => return Err(format!("failed to wait for checkpoint helper: {err}")),
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("failed to collect checkpoint helper output: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "checkpoint helper failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice::<Value>(&output.stdout).map_err(|err| {
        format!(
            "checkpoint helper returned invalid JSON: {err} (stdout: {})",
            String::from_utf8_lossy(&output.stdout)
                .chars()
                .take(200)
                .collect::<String>()
        )
    })
}
