use crate::projects::{canonicalize_and_verify, ProjectsConfig};
use crate::{Database, Message, MessageKind};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;
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

fn is_sensitive_path(path: &str) -> bool {
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
    let root = proj.root();
    let (code, stdout, stderr, duration_ms) = run_command(&cmd, &root, CHECK_TIMEOUT_SECS);
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
