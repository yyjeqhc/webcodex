use crate::projects::{canonicalize_and_verify, ProjectConfig, ProjectsConfig};
use crate::{Database, Message, MessageKind};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

// =============================================================================
// Request / Response types
// =============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextMode {
    Overview,
    Tree,
    Search,
    ReadFile,
    GitStatus,
    GitDiff,
}

#[derive(Debug, Deserialize)]
pub struct ContextRequest {
    pub project: String,
    pub mode: ContextMode,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default = "default_start_line")]
    pub start_line: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_start_line() -> usize {
    1
}
fn default_limit() -> usize {
    200
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PatchRequest {
    pub project: String,
    pub patch: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CheckRequest {
    pub project: String,
    pub suite: String,
}

#[derive(Debug, Deserialize)]
pub struct ReportRequest {
    pub project: String,
    pub status: String,
    pub title: String,
    pub summary: String,
    #[serde(default = "default_channel")]
    pub channel: String,
}

fn default_channel() -> String {
    "omo".to_string()
}

#[derive(Debug, Serialize)]
pub struct ContextResponse {
    pub success: bool,
    pub project: String,
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<String>>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PatchResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CheckResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suite: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout_tail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr_tail: Option<String>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReportResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EditRequest {
    pub project: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
    pub edits: Vec<EditOperation>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EditOperation {
    ReplaceText {
        path: String,
        old_text: String,
        new_text: String,
        occurrence: Option<usize>,
    },
    ReplaceRange {
        path: String,
        start_line: usize,
        end_line: usize,
        new_text: String,
    },
    AppendFile {
        path: String,
        text: String,
    },
    CreateFile {
        path: String,
        content: String,
    },
    WriteFile {
        path: String,
        content: String,
        #[serde(default)]
        allow_overwrite: bool,
    },
}

#[derive(Debug, Serialize)]
pub struct EditResponse {
    pub success: bool,
    pub changed_files: Vec<String>,
    pub diff: String,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

// =============================================================================
// Constants
// =============================================================================

const IGNORED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "dist",
    "build",
    ".cache",
    "__pycache__",
];
const MAX_TREE_ITEMS: usize = 300;
const MAX_SEARCH_RESULTS: usize = 50;
const MAX_OUTPUT_LEN: usize = 50_000;
const CHECK_TIMEOUT_SECS: u64 = 300;
const MAX_EDIT_FILE_SIZE: u64 = 2 * 1024 * 1024;
const MAX_EDIT_TEXT_SIZE: usize = 200 * 1024;

const SENSITIVE_PATHS: &[&str] = &[
    ".git",
    ".env",
    ".pem",
    ".key",
    "id_rsa",
    "id_ed25519",
    "target",
    "node_modules",
    "/etc",
    "/root/.ssh",
];

// =============================================================================
// Helpers
// =============================================================================

fn get_projects(depot: &Depot) -> Option<Arc<ProjectsConfig>> {
    depot.obtain::<Arc<ProjectsConfig>>().ok().cloned()
}

fn get_db(depot: &Depot) -> Option<Arc<Database>> {
    depot.obtain::<Arc<Database>>().ok().cloned()
}

fn truncate_string(s: String, max_len: usize) -> (String, bool) {
    if s.len() <= max_len {
        (s, false)
    } else {
        (s[..max_len].to_string(), true)
    }
}

fn is_ignored_dir(name: &str) -> bool {
    IGNORED_DIRS.contains(&name) || name.starts_with('.')
}

fn collect_tree(dir: &Path, base: &Path, items: &mut Vec<String>, limit: usize) {
    if items.len() >= limit {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    sorted.sort_by_key(|e| e.file_name());
    for entry in sorted {
        if items.len() >= limit {
            break;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if is_ignored_dir(&name) {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        if path.is_dir() {
            items.push(format!("{}/", rel));
            collect_tree(&path, base, items, limit);
        } else {
            items.push(rel);
        }
    }
}

fn simple_search(dir: &Path, query: &str, limit: usize) -> Vec<String> {
    let mut results = Vec::new();
    search_recursive(dir, dir, query, &mut results, limit);
    results
}

fn search_recursive(dir: &Path, base: &Path, query: &str, results: &mut Vec<String>, limit: usize) {
    if results.len() >= limit {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        if results.len() >= limit {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if is_ignored_dir(&name) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            search_recursive(&path, base, query, results, limit);
        } else if path.is_file() {
            // Only search text files (skip large files)
            let metadata = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if metadata.len() > 1_000_000 {
                continue;
            } // skip >1MB
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue, // skip binary files
            };
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            for (i, line) in content.lines().enumerate() {
                if results.len() >= limit {
                    return;
                }
                if line.contains(query) {
                    results.push(format!("{}:{}: {}", rel, i + 1, line.trim()));
                }
            }
        }
    }
}

fn parse_changed_files_from_patch(patch: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            // Format: diff --git a/path b/path
            if let Some(b_pos) = line.rfind(" b/") {
                let file = &line[b_pos + 3..];
                if !files.iter().any(|f: &String| f == file) {
                    files.push(file.to_string());
                }
            }
        }
    }
    files
}

pub fn is_sensitive_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    for sensitive in SENSITIVE_PATHS {
        if *sensitive == ".env" {
            // Match .env exactly or .env.* files
            let parts: Vec<&str> = path.split('/').collect();
            if parts.iter().any(|p| *p == ".env" || p.starts_with(".env.")) {
                return true;
            }
        } else if *sensitive == ".pem" || *sensitive == ".key" {
            if lower.ends_with(sensitive) {
                return true;
            }
        } else if lower.contains(sensitive) {
            return true;
        }
    }
    false
}

fn sanitize_tail(s: &str, max_len: usize) -> (String, bool) {
    let bytes = s.as_bytes();
    if bytes.len() <= max_len {
        (s.to_string(), false)
    } else {
        // Find a valid UTF-8 boundary near max_len
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        (s[end..].to_string(), true)
    }
}

fn run_command(cmd: &str, cwd: &Path, _timeout_secs: u64) -> (i32, String, String, u64) {
    let start = Instant::now();
    let result = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .output();

    match result {
        Ok(output) => {
            let elapsed = start.elapsed().as_millis() as u64;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let code = output.status.code().unwrap_or(-1);
            (code, stdout, stderr, elapsed)
        }
        Err(e) => {
            let elapsed = start.elapsed().as_millis() as u64;
            (
                -1,
                String::new(),
                format!("Failed to execute command: {}", e),
                elapsed,
            )
        }
    }
}

// =============================================================================
// SSH helpers
// =============================================================================

/// Build SSH target string [user@]host from project config.
fn build_ssh_target(proj: &ProjectConfig) -> Result<String, String> {
    proj.ssh_target()
}

/// Run a command on a remote host via SSH.
/// The command is passed as separate arguments to ssh (no local shell wrapping).
/// Remote shell interprets the command string.
fn run_ssh(ssh_target: &str, remote_cmd: &str, _timeout_secs: u64) -> (i32, String, String, u64) {
    let start = Instant::now();
    let result = std::process::Command::new("ssh")
        .arg(ssh_target)
        .arg("--")
        .arg(remote_cmd)
        .output();

    match result {
        Ok(output) => {
            let elapsed = start.elapsed().as_millis() as u64;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let code = output.status.code().unwrap_or(-1);
            (code, stdout, stderr, elapsed)
        }
        Err(e) => {
            let elapsed = start.elapsed().as_millis() as u64;
            (
                -1,
                String::new(),
                format!("Failed to execute SSH command: {}", e),
                elapsed,
            )
        }
    }
}

/// Run a command in the project directory.
/// For SSH: wraps with `cd <path> && <cmd>`.
/// For local: delegates to run_command with cwd.
fn run_project_cmd(
    proj: &ProjectConfig,
    cmd: &str,
    timeout_secs: u64,
) -> (i32, String, String, u64) {
    if proj.is_ssh() {
        let ssh_target = match build_ssh_target(proj) {
            Ok(t) => t,
            Err(e) => return (-1, String::new(), e, 0),
        };
        let remote_cmd = format!("cd {} && {}", proj.path, cmd);
        run_ssh(&ssh_target, &remote_cmd, timeout_secs)
    } else {
        run_command(cmd, &proj.root(), timeout_secs)
    }
}

/// Run an SSH command that receives patch data via stdin.
/// Writes local patch content to a remote temp file via SSH stdin,
/// then runs the remote command with the temp file path.
fn run_ssh_patch(
    ssh_target: &str,
    _project_path: &str,
    patch: &str,
    remote_cmd_template: &str,
) -> (i32, String, String, u64) {
    let patch_id = uuid::Uuid::new_v4();
    let remote_patch = format!("/tmp/private-drop-patch-{}.diff", patch_id);
    let remote_cmd = format!(
        "cat > '{}' && {} && rm -f '{}'",
        remote_patch,
        remote_cmd_template.replace("__PATCH__", &remote_patch),
        remote_patch
    );
    let start = Instant::now();
    let result = std::process::Command::new("ssh")
        .arg(ssh_target)
        .arg("--")
        .arg(&remote_cmd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(patch.as_bytes());
                // stdin is dropped here, closing the pipe
            }
            child.wait_with_output()
        });

    match result {
        Ok(output) => {
            let elapsed = start.elapsed().as_millis() as u64;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let code = output.status.code().unwrap_or(-1);
            (code, stdout, stderr, elapsed)
        }
        Err(e) => {
            let elapsed = start.elapsed().as_millis() as u64;
            (
                -1,
                String::new(),
                format!("Failed to execute SSH patch: {}", e),
                elapsed,
            )
        }
    }
}

/// Validate a path for SSH read_file operations.
fn validate_ssh_read_path(rel_path: &str) -> Result<(), String> {
    if rel_path.starts_with('/') {
        return Err("Absolute paths are not allowed".to_string());
    }
    if rel_path.contains("..") {
        return Err("Path traversal (..) is not allowed".to_string());
    }
    if is_sensitive_path(rel_path) {
        return Err(format!("Cannot access sensitive path: {}", rel_path));
    }
    Ok(())
}

// =============================================================================
// SSH context helpers
// =============================================================================

fn ssh_overview(proj: &ProjectConfig, project_name: &str) -> ContextResponse {
    let branch = run_project_cmd(proj, "git rev-parse --abbrev-ref HEAD", 10)
        .1
        .trim()
        .to_string();
    let status = run_project_cmd(proj, "git status --short", 10)
        .1
        .trim()
        .to_string();
    let important_files = [
        "README.md",
        "TODO.md",
        "Cargo.toml",
        "scripts/e2e_test.sh",
        "src/main.rs",
    ];
    let mut content = format!(
        "Project: {}\nRoot: {}\nBranch: {}\n\nGit Status:\n{}\n\nAllowed Checks: {}\n\nImportant Files:",
        project_name,
        proj.path,
        branch,
        status,
        proj.allowed_checks.join(", ")
    );
    for f in &important_files {
        let check_cmd = format!("test -f '{}' && echo yes || echo no", f);
        let exists = run_project_cmd(proj, &check_cmd, 5).1.trim().to_string();
        content.push_str(&format!(
            "\n  {}: {}",
            f,
            if exists == "yes" { "yes" } else { "no" }
        ));
    }
    ContextResponse {
        success: true,
        project: project_name.to_string(),
        mode: "overview".to_string(),
        content: Some(content),
        items: None,
        truncated: false,
        error: None,
    }
}

fn ssh_tree(proj: &ProjectConfig, project_name: &str) -> ContextResponse {
    // Build find exclusions
    let mut excludes = String::new();
    for dir in IGNORED_DIRS {
        excludes.push_str(&format!(" -not -path '*/{}/*'", dir));
    }
    let cmd = format!(
        "cd {} && find . -mindepth 1 -maxdepth 8{} -type f -print | sort | head -n {} | sed 's|^\\./||'",
        proj.path, excludes, MAX_TREE_ITEMS
    );
    let (code, stdout, stderr, _) = run_ssh(&build_ssh_target(proj).unwrap_or_default(), &cmd, 30);
    if code != 0 {
        return ContextResponse {
            success: false,
            project: project_name.to_string(),
            mode: "tree".to_string(),
            content: None,
            items: None,
            truncated: false,
            error: Some(format!("SSH tree failed: {}", stderr.trim())),
        };
    }
    let mut items: Vec<String> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();
    let truncated = items.len() >= MAX_TREE_ITEMS;
    items.truncate(MAX_TREE_ITEMS);
    ContextResponse {
        success: true,
        project: project_name.to_string(),
        mode: "tree".to_string(),
        content: None,
        items: Some(items),
        truncated,
        error: None,
    }
}

fn ssh_search(proj: &ProjectConfig, project_name: &str, query: &str) -> ContextResponse {
    // Build grep exclusions
    let mut excludes = String::new();
    for dir in IGNORED_DIRS {
        excludes.push_str(&format!(" --exclude-dir='{}'", dir));
    }
    // Use grep -rn, then head to limit results
    let escaped_query = query.replace('\'', "'\\''");
    let cmd = format!(
        "cd {} && grep -rn{} --include='*' '{}' . 2>/dev/null | head -n {} | sed 's|^\\./||'",
        proj.path, excludes, escaped_query, MAX_SEARCH_RESULTS
    );
    let (code, stdout, stderr, _) = run_ssh(&build_ssh_target(proj).unwrap_or_default(), &cmd, 30);
    // grep returns 1 if no match, that's ok
    if code != 0 && code != 1 {
        return ContextResponse {
            success: false,
            project: project_name.to_string(),
            mode: "search".to_string(),
            content: None,
            items: None,
            truncated: false,
            error: Some(format!("SSH search failed: {}", stderr.trim())),
        };
    }
    let items: Vec<String> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();
    let truncated = items.len() >= MAX_SEARCH_RESULTS;
    ContextResponse {
        success: true,
        project: project_name.to_string(),
        mode: "search".to_string(),
        content: None,
        items: Some(items),
        truncated,
        error: None,
    }
}

fn ssh_read_file(
    proj: &ProjectConfig,
    project_name: &str,
    rel_path: &str,
    start_line: usize,
    limit: usize,
) -> ContextResponse {
    if let Err(e) = validate_ssh_read_path(rel_path) {
        return ContextResponse {
            success: false,
            project: project_name.to_string(),
            mode: "read_file".to_string(),
            content: None,
            items: None,
            truncated: false,
            error: Some(e),
        };
    }
    let end_line = start_line + limit - 1;
    let escaped_path = rel_path.replace('\'', "'\\''");
    let cmd = format!(
        "cd {} && sed -n '{},{}p' '{}'",
        proj.path, start_line, end_line, escaped_path
    );
    let (code, stdout, stderr, _) = run_project_cmd(proj, &cmd, 30);
    if code != 0 {
        return ContextResponse {
            success: false,
            project: project_name.to_string(),
            mode: "read_file".to_string(),
            content: None,
            items: None,
            truncated: false,
            error: Some(format!("Failed to read file: {}", stderr.trim())),
        };
    }
    // Add line numbers like the local version
    let lines: Vec<String> = stdout
        .lines()
        .enumerate()
        .map(|(i, l)| format!("{:4} | {}", start_line + i, l))
        .collect();
    let output = lines.join("\n");
    let (output, truncated) = truncate_string(output, MAX_OUTPUT_LEN);
    ContextResponse {
        success: true,
        project: project_name.to_string(),
        mode: "read_file".to_string(),
        content: Some(output),
        items: None,
        truncated,
        error: None,
    }
}

fn ssh_apply_patch(
    proj: &ProjectConfig,
    _project_name: &str,
    patch: &str,
    changed: Vec<String>,
) -> PatchResponse {
    let ssh_target = match build_ssh_target(proj) {
        Ok(t) => t,
        Err(e) => {
            return PatchResponse {
                success: false,
                changed_files: Some(changed),
                stdout: None,
                stderr: None,
                error: Some(e),
            }
        }
    };
    let remote_cmd = format!(
        "cd {} && git apply --check __PATCH__ && git apply __PATCH__",
        proj.path
    );
    let (code, stdout, stderr, _) = run_ssh_patch(&ssh_target, &proj.path, patch, &remote_cmd);
    if code == 0 {
        PatchResponse {
            success: true,
            changed_files: Some(changed),
            stdout: Some(stdout),
            stderr: Some(stderr),
            error: None,
        }
    } else {
        PatchResponse {
            success: false,
            changed_files: Some(changed),
            stdout: Some(stdout),
            stderr: Some(stderr),
            error: Some("git apply failed".to_string()),
        }
    }
}

// =============================================================================
// Handlers
// =============================================================================

#[handler]
pub async fn codex_context(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(projects) = get_projects(depot) else {
        res.render(Json(ContextResponse {
            success: false,
            project: String::new(),
            mode: String::new(),
            content: None,
            items: None,
            truncated: false,
            error: Some(
                "Projects not configured. Set PROJECTS_CONFIG or create projects.toml".to_string(),
            ),
        }));
        return;
    };
    let body: ContextRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ContextResponse {
                success: false,
                project: String::new(),
                mode: String::new(),
                content: None,
                items: None,
                truncated: false,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let proj = match projects.get_project(&body.project) {
        Ok(p) => p,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ContextResponse {
                success: false,
                project: body.project.clone(),
                mode: format!("{:?}", body.mode),
                content: None,
                items: None,
                truncated: false,
                error: Some(e),
            }));
            return;
        }
    };

    // For SSH executor, dispatch to SSH helpers
    if proj.is_ssh() {
        let resp = match body.mode {
            ContextMode::Overview => ssh_overview(proj, &body.project),
            ContextMode::Tree => ssh_tree(proj, &body.project),
            ContextMode::Search => {
                let Some(query) = &body.query else {
                    res.status_code(StatusCode::BAD_REQUEST);
                    res.render(Json(ContextResponse {
                        success: false,
                        project: body.project,
                        mode: "search".to_string(),
                        content: None,
                        items: None,
                        truncated: false,
                        error: Some("query parameter is required for search mode".to_string()),
                    }));
                    return;
                };
                ssh_search(proj, &body.project, query)
            }
            ContextMode::ReadFile => {
                let Some(rel_path) = &body.path else {
                    res.status_code(StatusCode::BAD_REQUEST);
                    res.render(Json(ContextResponse {
                        success: false,
                        project: body.project,
                        mode: "read_file".to_string(),
                        content: None,
                        items: None,
                        truncated: false,
                        error: Some("path parameter is required for read_file mode".to_string()),
                    }));
                    return;
                };
                ssh_read_file(proj, &body.project, rel_path, body.start_line, body.limit)
            }
            ContextMode::GitStatus => {
                let (code, stdout, stderr, _) = run_project_cmd(proj, "git status --short", 10);
                if code != 0 {
                    ContextResponse {
                        success: false,
                        project: body.project.clone(),
                        mode: "git_status".to_string(),
                        content: None,
                        items: None,
                        truncated: false,
                        error: Some(format!("git status failed: {}", stderr.trim())),
                    }
                } else {
                    let (content, truncated) = truncate_string(stdout, MAX_OUTPUT_LEN);
                    ContextResponse {
                        success: true,
                        project: body.project.clone(),
                        mode: "git_status".to_string(),
                        content: Some(content),
                        items: None,
                        truncated,
                        error: None,
                    }
                }
            }
            ContextMode::GitDiff => {
                let (code, stdout, stderr, _) = run_project_cmd(proj, "git diff", 30);
                if code != 0 {
                    ContextResponse {
                        success: false,
                        project: body.project.clone(),
                        mode: "git_diff".to_string(),
                        content: None,
                        items: None,
                        truncated: false,
                        error: Some(format!("git diff failed: {}", stderr.trim())),
                    }
                } else {
                    let (content, truncated) = truncate_string(stdout, MAX_OUTPUT_LEN);
                    ContextResponse {
                        success: true,
                        project: body.project.clone(),
                        mode: "git_diff".to_string(),
                        content: Some(content),
                        items: None,
                        truncated,
                        error: None,
                    }
                }
            }
        };
        res.render(Json(resp));
        return;
    }

    // Local executor
    let root = proj.root();
    if !root.exists() {
        res.render(Json(ContextResponse {
            success: false,
            project: body.project.clone(),
            mode: format!("{:?}", body.mode),
            content: None,
            items: None,
            truncated: false,
            error: Some(format!("Project root does not exist: {:?}", root)),
        }));
        return;
    }

    match body.mode {
        ContextMode::Overview => {
            let branch = run_command("git rev-parse --abbrev-ref HEAD", &root, 10)
                .1
                .trim()
                .to_string();
            let status = run_command("git status --short", &root, 10)
                .1
                .trim()
                .to_string();
            let important_files = [
                "README.md",
                "TODO.md",
                "Cargo.toml",
                "scripts/e2e_test.sh",
                "src/main.rs",
            ];
            let mut content = format!("Project: {}\nRoot: {}\nBranch: {}\n\nGit Status:\n{}\n\nAllowed Checks: {}\n\nImportant Files:",
                body.project, root.display(), branch, status, proj.allowed_checks.join(", "));
            for f in &important_files {
                let exists = root.join(f).exists();
                content.push_str(&format!("\n  {}: {}", f, if exists { "yes" } else { "no" }));
            }
            res.render(Json(ContextResponse {
                success: true,
                project: body.project,
                mode: "overview".to_string(),
                content: Some(content),
                items: None,
                truncated: false,
                error: None,
            }));
        }
        ContextMode::Tree => {
            let mut items = Vec::new();
            collect_tree(&root, &root, &mut items, MAX_TREE_ITEMS);
            let truncated = items.len() >= MAX_TREE_ITEMS;
            res.render(Json(ContextResponse {
                success: true,
                project: body.project,
                mode: "tree".to_string(),
                content: None,
                items: Some(items),
                truncated,
                error: None,
            }));
        }
        ContextMode::Search => {
            let Some(query) = &body.query else {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(Json(ContextResponse {
                    success: false,
                    project: body.project,
                    mode: "search".to_string(),
                    content: None,
                    items: None,
                    truncated: false,
                    error: Some("query parameter is required for search mode".to_string()),
                }));
                return;
            };
            let results = simple_search(&root, query, MAX_SEARCH_RESULTS);
            let truncated = results.len() >= MAX_SEARCH_RESULTS;
            res.render(Json(ContextResponse {
                success: true,
                project: body.project,
                mode: "search".to_string(),
                content: None,
                items: Some(results),
                truncated,
                error: None,
            }));
        }
        ContextMode::ReadFile => {
            let Some(rel_path) = &body.path else {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(Json(ContextResponse {
                    success: false,
                    project: body.project,
                    mode: "read_file".to_string(),
                    content: None,
                    items: None,
                    truncated: false,
                    error: Some("path parameter is required for read_file mode".to_string()),
                }));
                return;
            };
            let full_path = root.join(rel_path);
            match canonicalize_and_verify(&full_path, &root) {
                Ok(canonical) => match std::fs::read_to_string(&canonical) {
                    Ok(content) => {
                        let lines: Vec<&str> = content.lines().collect();
                        let total = lines.len();
                        let start = body.start_line.max(1) - 1;
                        let end = (start + body.limit).min(total);
                        let selected: Vec<String> = if start < total {
                            lines[start..end]
                                .iter()
                                .enumerate()
                                .map(|(i, l)| format!("{:4} | {}", start + i + 1, l))
                                .collect()
                        } else {
                            Vec::new()
                        };
                        let output = selected.join("\n");
                        let (output, truncated) = truncate_string(output, MAX_OUTPUT_LEN);
                        res.render(Json(ContextResponse {
                            success: true,
                            project: body.project,
                            mode: "read_file".to_string(),
                            content: Some(output),
                            items: None,
                            truncated,
                            error: None,
                        }));
                    }
                    Err(e) => {
                        res.render(Json(ContextResponse {
                            success: false,
                            project: body.project,
                            mode: "read_file".to_string(),
                            content: None,
                            items: None,
                            truncated: false,
                            error: Some(format!("Failed to read file: {}", e)),
                        }));
                    }
                },
                Err(e) => {
                    res.status_code(StatusCode::BAD_REQUEST);
                    res.render(Json(ContextResponse {
                        success: false,
                        project: body.project,
                        mode: "read_file".to_string(),
                        content: None,
                        items: None,
                        truncated: false,
                        error: Some(e),
                    }));
                }
            }
        }
        ContextMode::GitStatus => {
            let output = run_command("git status --short", &root, 10);
            let (content, truncated) = truncate_string(output.1, MAX_OUTPUT_LEN);
            res.render(Json(ContextResponse {
                success: true,
                project: body.project,
                mode: "git_status".to_string(),
                content: Some(content),
                items: None,
                truncated,
                error: None,
            }));
        }
        ContextMode::GitDiff => {
            let output = run_command("git diff", &root, 30);
            let (content, truncated) = truncate_string(output.1, MAX_OUTPUT_LEN);
            res.render(Json(ContextResponse {
                success: true,
                project: body.project,
                mode: "git_diff".to_string(),
                content: Some(content),
                items: None,
                truncated,
                error: None,
            }));
        }
    }
}

#[handler]
pub async fn codex_apply_patch(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(projects) = get_projects(depot) else {
        res.render(Json(PatchResponse {
            success: false,
            changed_files: None,
            stdout: None,
            stderr: None,
            error: Some("Projects not configured".to_string()),
        }));
        return;
    };
    let body: PatchRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(PatchResponse {
                success: false,
                changed_files: None,
                stdout: None,
                stderr: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let proj = match projects.get_project(&body.project) {
        Ok(p) => p,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(PatchResponse {
                success: false,
                changed_files: None,
                stdout: None,
                stderr: None,
                error: Some(e),
            }));
            return;
        }
    };
    if !proj.allow_patch() {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(PatchResponse {
            success: false,
            changed_files: None,
            stdout: None,
            stderr: None,
            error: Some("Patch is not allowed for this project".to_string()),
        }));
        return;
    }
    if body.patch.is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(PatchResponse {
            success: false,
            changed_files: None,
            stdout: None,
            stderr: None,
            error: Some("Patch cannot be empty".to_string()),
        }));
        return;
    }

    // Validate changed file paths against sensitive paths
    let changed = parse_changed_files_from_patch(&body.patch);
    for file in &changed {
        if is_sensitive_path(file) {
            res.status_code(StatusCode::FORBIDDEN);
            res.render(Json(PatchResponse {
                success: false,
                changed_files: None,
                stdout: None,
                stderr: None,
                error: Some(format!("Cannot modify sensitive path: {}", file)),
            }));
            return;
        }
    }

    if proj.is_ssh() {
        // SSH executor: pipe patch via stdin
        let resp = ssh_apply_patch(proj, &body.project, &body.patch, changed);
        res.render(Json(resp));
        return;
    }

    // Local executor
    let root = proj.root();
    if !root.exists() {
        res.render(Json(PatchResponse {
            success: false,
            changed_files: None,
            stdout: None,
            stderr: None,
            error: Some("Project root does not exist".to_string()),
        }));
        return;
    }

    // Write patch to temp file, run git apply
    let patch_file = root.join(format!(".codex-patch-{}.diff", uuid::Uuid::new_v4()));
    if let Err(e) = std::fs::write(&patch_file, &body.patch) {
        res.render(Json(PatchResponse {
            success: false,
            changed_files: None,
            stdout: None,
            stderr: None,
            error: Some(format!("Failed to write temp patch file: {}", e)),
        }));
        return;
    }

    // Dry run first
    let check_out = run_command(
        &format!("git apply --check '{}'", patch_file.display()),
        &root,
        60,
    );
    if check_out.0 != 0 {
        let _ = std::fs::remove_file(&patch_file);
        res.render(Json(PatchResponse {
            success: false,
            changed_files: Some(changed),
            stdout: Some(check_out.1),
            stderr: Some(check_out.2),
            error: Some("git apply --check failed".to_string()),
        }));
        return;
    }

    // Apply for real
    let apply_out = run_command(&format!("git apply '{}'", patch_file.display()), &root, 60);
    let _ = std::fs::remove_file(&patch_file);

    if apply_out.0 == 0 {
        res.render(Json(PatchResponse {
            success: true,
            changed_files: Some(changed),
            stdout: Some(apply_out.1),
            stderr: Some(apply_out.2),
            error: None,
        }));
    } else {
        res.render(Json(PatchResponse {
            success: false,
            changed_files: Some(changed),
            stdout: Some(apply_out.1),
            stderr: Some(apply_out.2),
            error: Some("git apply failed".to_string()),
        }));
    }
}

#[handler]
pub async fn codex_check(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(projects) = get_projects(depot) else {
        res.render(Json(CheckResponse {
            success: false,
            suite: None,
            exit_code: None,
            duration_ms: None,
            stdout_tail: None,
            stderr_tail: None,
            truncated: false,
            error: Some("Projects not configured".to_string()),
        }));
        return;
    };
    let body: CheckRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(CheckResponse {
                success: false,
                suite: None,
                exit_code: None,
                duration_ms: None,
                stdout_tail: None,
                stderr_tail: None,
                truncated: false,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let proj = match projects.get_project(&body.project) {
        Ok(p) => p,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(CheckResponse {
                success: false,
                suite: Some(body.suite),
                exit_code: None,
                duration_ms: None,
                stdout_tail: None,
                stderr_tail: None,
                truncated: false,
                error: Some(e),
            }));
            return;
        }
    };
    if !proj.is_check_allowed(&body.suite) {
        res.status_code(StatusCode::FORBIDDEN);
        let suite = body.suite.clone();
        res.render(Json(CheckResponse {
            success: false,
            suite: Some(body.suite),
            exit_code: None,
            duration_ms: None,
            stdout_tail: None,
            stderr_tail: None,
            truncated: false,
            error: Some(format!(
                "Check '{}' is not allowed. Allowed: {}",
                suite,
                proj.allowed_checks.join(", ")
            )),
        }));
        return;
    }
    let cmd = match proj.get_check_command(&body.suite) {
        Ok(c) => c,
        Err(e) => {
            res.render(Json(CheckResponse {
                success: false,
                suite: Some(body.suite),
                exit_code: None,
                duration_ms: None,
                stdout_tail: None,
                stderr_tail: None,
                truncated: false,
                error: Some(e),
            }));
            return;
        }
    };
    let (code, stdout, stderr, duration_ms) = run_project_cmd(proj, &cmd, CHECK_TIMEOUT_SECS);
    let (stdout_tail, stdout_trunc) = sanitize_tail(&stdout, MAX_OUTPUT_LEN);
    let (stderr_tail, stderr_trunc) = sanitize_tail(&stderr, MAX_OUTPUT_LEN);
    let truncated = stdout_trunc || stderr_trunc;

    res.render(Json(CheckResponse {
        success: code == 0,
        suite: Some(body.suite),
        exit_code: Some(code),
        duration_ms: Some(duration_ms),
        stdout_tail: Some(stdout_tail),
        stderr_tail: Some(stderr_tail),
        truncated,
        error: None,
    }));
}

#[handler]
pub async fn codex_report(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(projects) = get_projects(depot) else {
        res.render(Json(ReportResponse {
            success: false,
            report_id: None,
            message_id: None,
            path: None,
            error: Some("Projects not configured".to_string()),
        }));
        return;
    };
    let Some(db) = get_db(depot) else {
        res.render(Json(ReportResponse {
            success: false,
            report_id: None,
            message_id: None,
            path: None,
            error: Some("No database".to_string()),
        }));
        return;
    };
    let body: ReportRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ReportResponse {
                success: false,
                report_id: None,
                message_id: None,
                path: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let _proj = match projects.get_project(&body.project) {
        Ok(p) => p,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ReportResponse {
                success: false,
                report_id: None,
                message_id: None,
                path: None,
                error: Some(e),
            }));
            return;
        }
    };

    let report_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let timestamp = now.format("%Y%m%d_%H%M%S").to_string();
    let filename = format!("{}_{}.json", timestamp, &report_id[..8]);
    let report_dir = std::env::var("DROP_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("./data"))
        .join("reports");

    if let Err(e) = std::fs::create_dir_all(&report_dir) {
        res.render(Json(ReportResponse {
            success: false,
            report_id: None,
            message_id: None,
            path: None,
            error: Some(format!("Failed to create reports directory: {}", e)),
        }));
        return;
    }

    let report_path = report_dir.join(&filename);
    let report_json = serde_json::json!({
        "id": report_id,
        "project": body.project,
        "status": body.status,
        "title": body.title,
        "summary": body.summary,
        "channel": body.channel,
        "created_at": now.timestamp(),
    });
    if let Err(e) = std::fs::write(
        &report_path,
        serde_json::to_string_pretty(&report_json).unwrap(),
    ) {
        res.render(Json(ReportResponse {
            success: false,
            report_id: None,
            message_id: None,
            path: None,
            error: Some(format!("Failed to write report: {}", e)),
        }));
        return;
    }

    // Write message to channel
    let msg_text = format!("[{}] {}\n\n{}", body.status, body.title, body.summary);
    let message = Message {
        id: uuid::Uuid::new_v4().to_string(),
        channel: body.channel.clone(),
        kind: MessageKind::Text,
        title: Some(format!("[codex] {}", body.title)),
        text: Some(msg_text),
        file_name: None,
        file_path: None,
        file_size: None,
        mime_type: None,
        created_at: now.timestamp(),
        expires_at: None,
    };
    let message_id = message.id.clone();
    if let Err(e) = db.insert_message(&message) {
        // Report was written but message failed
        res.render(Json(ReportResponse {
            success: true,
            report_id: Some(report_id),
            message_id: None,
            path: Some(report_path.to_string_lossy().to_string()),
            error: Some(format!("Report written but message insert failed: {}", e)),
        }));
        return;
    }

    res.render(Json(ReportResponse {
        success: true,
        report_id: Some(report_id),
        message_id: Some(message_id),
        path: Some(report_path.to_string_lossy().to_string()),
        error: None,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_ssh_path_rejects_absolute() {
        assert!(validate_ssh_read_path("/etc/passwd").is_err());
    }

    #[test]
    fn test_validate_ssh_path_rejects_traversal() {
        assert!(validate_ssh_read_path("../evil.txt").is_err());
        assert!(validate_ssh_read_path("src/../../../etc/passwd").is_err());
    }

    #[test]
    fn test_validate_ssh_path_rejects_sensitive() {
        assert!(validate_ssh_read_path(".env").is_err());
        assert!(validate_ssh_read_path("secret.pem").is_err());
        assert!(validate_ssh_read_path(".git/config").is_err());
        assert!(validate_ssh_read_path("target/debug/binary").is_err());
        assert!(validate_ssh_read_path("node_modules/pkg/index.js").is_err());
    }

    #[test]
    fn test_validate_ssh_path_allows_normal() {
        assert!(validate_ssh_read_path("src/main.rs").is_ok());
        assert!(validate_ssh_read_path("README.md").is_ok());
        assert!(validate_ssh_read_path("src/lib/helper.rs").is_ok());
    }

    #[test]
    fn test_is_sensitive_path_variants() {
        assert!(is_sensitive_path(".env"));
        assert!(is_sensitive_path(".env.local"));
        assert!(is_sensitive_path("secret.pem"));
        assert!(is_sensitive_path("id_rsa"));
        assert!(is_sensitive_path(".git/config"));
        assert!(!is_sensitive_path("src/main.rs"));
        assert!(!is_sensitive_path("README.md"));
    }

    #[test]
    fn test_build_ssh_target() {
        let proj = ProjectConfig {
            path: "/tmp/test".to_string(),
            executor: crate::projects::Executor::Ssh,
            host: Some("msi".to_string()),
            user: Some("root".to_string()),
            allow_patch: false,
            allowed_checks: vec![],
            checks: None,
        };
        assert_eq!(proj.ssh_target().unwrap(), "root@msi");

        let proj_no_user = ProjectConfig {
            path: "/tmp/test".to_string(),
            executor: crate::projects::Executor::Ssh,
            host: Some("msi".to_string()),
            user: None,
            allow_patch: false,
            allowed_checks: vec![],
            checks: None,
        };
        assert_eq!(proj_no_user.ssh_target().unwrap(), "msi");
    }

    #[test]
    fn test_build_ssh_target_no_host() {
        let proj = ProjectConfig {
            path: "/tmp/test".to_string(),
            executor: crate::projects::Executor::Ssh,
            host: None,
            user: None,
            allow_patch: false,
            allowed_checks: vec![],
            checks: None,
        };
        assert!(proj.ssh_target().is_err());
    }

    #[test]
    fn test_local_executor_is_default() {
        let proj = ProjectConfig {
            path: "/tmp/test".to_string(),
            executor: crate::projects::Executor::default(),
            host: None,
            user: None,
            allow_patch: false,
            allowed_checks: vec![],
            checks: None,
        };
        assert!(!proj.is_ssh());
    }
}

fn edit_error(error: String) -> EditResponse {
    EditResponse {
        success: false,
        changed_files: Vec::new(),
        diff: String::new(),
        warnings: Vec::new(),
        error: Some(error),
    }
}

fn edit_path(edit: &EditOperation) -> &str {
    match edit {
        EditOperation::ReplaceText { path, .. }
        | EditOperation::ReplaceRange { path, .. }
        | EditOperation::AppendFile { path, .. }
        | EditOperation::CreateFile { path, .. }
        | EditOperation::WriteFile { path, .. } => path,
    }
}

fn edit_text_len(edit: &EditOperation) -> usize {
    match edit {
        EditOperation::ReplaceText { new_text, .. } => new_text.len(),
        EditOperation::ReplaceRange { new_text, .. } => new_text.len(),
        EditOperation::AppendFile { text, .. } => text.len(),
        EditOperation::CreateFile { content, .. } => content.len(),
        EditOperation::WriteFile { content, .. } => content.len(),
    }
}

fn validate_edit_path(rel_path: &str) -> Result<(), String> {
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

fn simple_file_diff(path: &str, old: Option<&str>, new: &str) -> String {
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

fn resolve_edit_path(root: &Path, rel_path: &str, must_exist: bool) -> Result<PathBuf, String> {
    validate_edit_path(rel_path)?;
    let full_path = root.join(rel_path);
    if must_exist {
        return canonicalize_and_verify(&full_path, root);
    }
    let parent = full_path
        .parent()
        .ok_or_else(|| "path has no parent directory".to_string())?;
    let canonical_parent = canonicalize_and_verify(parent, root)?;
    let file_name = full_path
        .file_name()
        .ok_or_else(|| "path has no file name".to_string())?;
    Ok(canonical_parent.join(file_name))
}

fn read_edit_file(path: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Failed to stat file: {}", e))?;
    if meta.len() > MAX_EDIT_FILE_SIZE {
        return Err(format!(
            "File is too large for edit API: {} bytes",
            meta.len()
        ));
    }
    std::fs::read_to_string(path).map_err(|e| format!("Failed to read UTF-8 text file: {}", e))
}

fn replace_nth(
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

fn replace_line_range(
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

fn load_edit_content(
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

fn local_apply_project_edit(proj: &ProjectConfig, body: &EditRequest) -> EditResponse {
    let root = proj.root();
    if !root.exists() {
        return edit_error("Project root does not exist".to_string());
    }
    let mut paths: HashMap<String, PathBuf> = HashMap::new();
    let mut originals: HashMap<String, Option<String>> = HashMap::new();
    let mut current: HashMap<String, Option<String>> = HashMap::new();
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
        }
    }
    if !body.dry_run {
        for path in &changed_files {
            if let (Some(full_path), Some(Some(new_content))) = (paths.get(path), current.get(path))
            {
                if let Err(e) = std::fs::write(full_path, new_content) {
                    return edit_error(format!("Failed to write {}: {}", path, e));
                }
            }
        }
    }
    EditResponse {
        success: true,
        changed_files,
        diff,
        warnings: Vec::new(),
        error: None,
    }
}

#[handler]
pub async fn codex_edit(req: &mut Request, depot: &mut Depot, res: &mut Response) {
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
    if proj.is_ssh() {
        res.render(Json(edit_error(
            "SSH edit support requires remote python3 implementation".to_string(),
        )));
        return;
    }
    res.render(Json(local_apply_project_edit(proj, &body)));
}
