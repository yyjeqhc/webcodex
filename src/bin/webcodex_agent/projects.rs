use super::config::{
    default_true, projects_dir, validate_shell_profile_name, AgentConfig, AgentPolicy,
};
use super::shell::canonicalize_existing;
use crate::shell_protocol::{ShellAgentProjectSummary, ShellAgentShellRequest};
use crate::{err_cmd, ok_cmd, write_created_file};
use crate::{CommandResult, CreatedProjectPaths};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const PROJECT_SCAN_CACHE_MS: u64 = 5000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentProjectFile {
    pub(crate) id: String,
    pub(crate) path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) shell_profile: Option<String>,
    #[serde(default = "default_true")]
    pub(crate) allow_patch: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) disabled: bool,
    #[serde(default)]
    pub(crate) hooks: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AgentProjectCache {
    projects: Vec<ShellAgentProjectSummary>,
    refreshed_at: Option<Instant>,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentProjectShellContext {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) shell_profile: Option<String>,
}

fn validate_project_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("id cannot be empty".to_string());
    }
    if id == "." || id == ".." {
        return Err("id cannot be '.' or '..'".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err("id may only contain ASCII letters, digits, '-', '_', and '.'".to_string());
    }
    Ok(())
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn agent_project_server_format_hint(content: &str, err: &str) -> Option<String> {
    let normalized = err.replace('`', "");
    if normalized.contains("missing field id") && content.contains("[projects.") {
        Some(
            "looks like a server projects.toml entry. Agent projects.d files must use top-level fields:\n\
             id = \"smoke\"\n\
             path = \"/path/to/repo\""
                .to_string(),
        )
    } else {
        None
    }
}

pub(crate) fn parse_agent_project_toml(content: &str) -> Result<AgentProjectFile, String> {
    let mut project: AgentProjectFile = toml::from_str(content).map_err(|e| {
        let err = e.to_string();
        let base = format!("failed to parse project toml: {}", err);
        match agent_project_server_format_hint(content, &err) {
            Some(hint) => format!("{}; {}", base, hint),
            None => base,
        }
    })?;
    project.id = project.id.trim().to_string();
    validate_project_id(&project.id)?;
    project.path = project.path.trim().to_string();
    if project.path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    project.name = trim_optional(project.name);
    project.kind = trim_optional(project.kind);
    project.description = trim_optional(project.description);
    if let Some(shell_profile) = &project.shell_profile {
        validate_shell_profile_name("project.shell_profile", shell_profile)?;
    }
    let mut hooks = HashMap::new();
    for (name, commands) in project.hooks {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("hook name cannot be empty".to_string());
        }
        hooks.insert(name, commands);
    }
    project.hooks = hooks;
    Ok(project)
}

fn load_agent_project_shell_contexts_from_dir(dir: &Path) -> Vec<AgentProjectShellContext> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            files.push(path);
        }
    }
    files.sort();
    let mut seen = HashSet::new();
    let mut projects = Vec::new();
    for file in files {
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(project) = parse_agent_project_toml(&content) else {
            continue;
        };
        if project.disabled || !seen.insert(project.id.clone()) {
            continue;
        }
        projects.push(AgentProjectShellContext {
            id: project.id,
            path: project.path,
            shell_profile: project.shell_profile,
        });
    }
    projects
}

pub(crate) fn find_project_shell_context(
    projects_dir: &Path,
    cwd_path: &Path,
) -> Option<AgentProjectShellContext> {
    let cwd = cwd_path.canonicalize().ok()?;
    load_agent_project_shell_contexts_from_dir(projects_dir)
        .into_iter()
        .filter_map(|project| {
            let project_path = PathBuf::from(&project.path).canonicalize().ok()?;
            if cwd == project_path || cwd.starts_with(&project_path) {
                Some((project_path.components().count(), project))
            } else {
                None
            }
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, project)| project)
}

fn run_git_capture(path: &str, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(crate) fn agent_project_summary(
    project: &AgentProjectFile,
    updated_at: i64,
    include_git: bool,
) -> ShellAgentProjectSummary {
    let mut hooks = project.hooks.keys().cloned().collect::<Vec<_>>();
    hooks.sort();
    let (git_branch, git_head, git_dirty) = if include_git {
        let branch = run_git_capture(&project.path, &["rev-parse", "--abbrev-ref", "HEAD"]);
        let head = run_git_capture(&project.path, &["log", "-1", "--pretty=format:%h"]);
        let dirty = run_git_capture(&project.path, &["status", "--short"])
            .map(|status| !status.trim().is_empty());
        (branch, head, dirty)
    } else {
        (None, None, None)
    };
    ShellAgentProjectSummary {
        id: project.id.clone(),
        name: project.name.clone().or_else(|| Some(project.id.clone())),
        path: project.path.clone(),
        allow_patch: project.allow_patch,
        kind: project.kind.clone(),
        description: project.description.clone(),
        hooks,
        disabled: project.disabled,
        git_branch,
        git_head,
        git_dirty,
        updated_at,
        shell_profile: project.shell_profile.clone(),
    }
}

fn warn_empty_hook_commands(source: &Path, project: &AgentProjectFile) {
    for (hook, commands) in &project.hooks {
        for (idx, command) in commands.iter().enumerate() {
            if command.trim().is_empty() {
                eprintln!(
                    "webcodex-agent project warning: {} hook {} command {} is empty",
                    source.display(),
                    hook,
                    idx
                );
            }
        }
    }
}

pub(crate) fn load_agent_project_summaries_from_dir(dir: &Path) -> Vec<ShellAgentProjectSummary> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            eprintln!(
                "webcodex-agent project warning: failed to read {}: {}",
                dir.display(),
                e
            );
            return Vec::new();
        }
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            files.push(path);
        }
    }
    files.sort();

    let updated_at = chrono::Utc::now().timestamp();
    let mut seen = HashSet::new();
    let mut projects = Vec::new();
    for file in files {
        let content = match std::fs::read_to_string(&file) {
            Ok(content) => content,
            Err(e) => {
                eprintln!(
                    "webcodex-agent project warning: failed to read {}: {}",
                    file.display(),
                    e
                );
                continue;
            }
        };
        let project = match parse_agent_project_toml(&content) {
            Ok(project) => project,
            Err(e) => {
                eprintln!(
                    "webcodex-agent project warning: skipping {}: {}",
                    file.display(),
                    e
                );
                continue;
            }
        };
        if project.disabled {
            continue;
        }
        if !seen.insert(project.id.clone()) {
            eprintln!(
                "webcodex-agent project warning: duplicate project id {} in {}; skipping",
                project.id,
                file.display()
            );
            continue;
        }
        warn_empty_hook_commands(&file, &project);
        projects.push(agent_project_summary(&project, updated_at, true));
    }
    projects.sort_by(|a, b| a.id.cmp(&b.id));
    projects
}

fn load_agent_project_summaries(cfg: &AgentConfig) -> Vec<ShellAgentProjectSummary> {
    load_agent_project_summaries_from_dir(&projects_dir(cfg))
}

impl AgentProjectCache {
    pub(crate) fn get(&mut self, cfg: &AgentConfig) -> Vec<ShellAgentProjectSummary> {
        if self.refreshed_at.is_some_and(|refreshed_at| {
            refreshed_at.elapsed() < Duration::from_millis(PROJECT_SCAN_CACHE_MS)
        }) {
            return self.projects.clone();
        }
        self.projects = load_agent_project_summaries(cfg);
        self.refreshed_at = Some(Instant::now());
        self.projects.clone()
    }

    pub(crate) fn invalidate(&mut self) {
        self.projects.clear();
        self.refreshed_at = None;
    }
}

/// System directories that must never be used as a project root unless they are
/// explicitly under an `allowed_roots` entry. Even when `allow_cwd_anywhere`
/// is true, these roots are rejected to prevent accidental registration of
/// critical system paths.
const DANGEROUS_PROJECT_ROOTS: &[&str] = &[
    "/", "/etc", "/bin", "/sbin", "/usr", "/var", "/proc", "/sys", "/dev", "/run", "/boot",
];

/// Escape a string for use as a TOML basic string (double-quoted). NUL is
/// rejected up front by validation, so we only handle backslash, quote, and
/// common control characters.
fn toml_basic_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{}\"", escaped)
}

/// Build a deterministic project TOML string compatible with the existing
/// `parse_agent_project_toml` parser. The field order is fixed so the output
/// is reproducible.
fn build_project_toml(
    id: &str,
    name: &str,
    path: &str,
    description: &Option<String>,
    allow_patch: bool,
) -> String {
    let mut toml = String::new();
    toml.push_str(&format!("id = {}\n", toml_basic_string(id)));
    toml.push_str(&format!("name = {}\n", toml_basic_string(name)));
    toml.push_str(&format!("path = {}\n", toml_basic_string(path)));
    if let Some(desc) = description {
        toml.push_str(&format!("description = {}\n", toml_basic_string(desc)));
    }
    toml.push_str(&format!("allow_patch = {}\n", allow_patch));
    toml
}

/// Validate the project `id` for project-management operations. Stricter than
/// the existing `validate_project_id`: no dots (prevents any path-like
/// interpretation), only ASCII letters/digits/dash/underscore.
fn validate_project_op_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("id cannot be empty".to_string());
    }
    if id.contains('\0') {
        return Err("id must not contain NUL".to_string());
    }
    if id.len() > 64 {
        return Err("id must be at most 64 characters".to_string());
    }
    if id.contains('/') || id.contains('\\') {
        return Err("id must not contain slash or backslash".to_string());
    }
    if id == ".." || id == "." || id.contains("..") {
        return Err("id must not contain dot-dot traversal".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("id may only contain ASCII letters, digits, '-', and '_'".to_string());
    }
    Ok(())
}

/// Validate the project `name`: non-empty after trim, <= 120 chars, no NUL.
fn validate_project_op_name(name: &str) -> Result<(), String> {
    if name.contains('\0') {
        return Err("name must not contain NUL".to_string());
    }
    if name.trim().is_empty() {
        return Err("name cannot be empty".to_string());
    }
    if name.len() > 120 {
        return Err("name must be at most 120 characters".to_string());
    }
    Ok(())
}

/// Validate the optional `description`: <= 500 chars, no NUL.
fn validate_project_op_description(desc: &str) -> Result<(), String> {
    if desc.contains('\0') {
        return Err("description must not contain NUL".to_string());
    }
    if desc.len() > 500 {
        return Err("description must be at most 500 characters".to_string());
    }
    Ok(())
}

/// Check whether a canonicalized project path is allowed by the agent policy.
/// Returns Ok(()) if the path is safe, Err otherwise.
///
/// - If `allow_cwd_anywhere` is false, the path must be under an explicit
///   `allowed_roots` entry.
/// - If `allow_cwd_anywhere` is true, the path is allowed unless it is one of
///   the `DANGEROUS_PROJECT_ROOTS` (and not under an explicit `allowed_roots`).
pub(crate) fn validate_project_path_policy(
    policy: &AgentPolicy,
    canonical_path: &Path,
) -> Result<(), String> {
    let path_str = canonical_path.to_string_lossy().to_string();
    // If under an explicit allowed_root, always allow.
    for root in &policy.allowed_roots {
        if let Ok(canonical_root) = canonicalize_existing(root) {
            if canonical_path == &canonical_root || canonical_path.starts_with(&canonical_root) {
                return Ok(());
            }
        }
    }
    if !policy.allow_cwd_anywhere {
        return Err(format!(
            "path {} is outside allowed_roots and allow_cwd_anywhere is false",
            path_str
        ));
    }
    // allow_cwd_anywhere is true: reject dangerous system roots.
    for &dangerous in DANGEROUS_PROJECT_ROOTS {
        let dangerous_root = Path::new(dangerous);
        let is_dangerous = if dangerous_root == Path::new("/") {
            canonical_path == dangerous_root
        } else {
            canonical_path == dangerous_root || canonical_path.starts_with(dangerous_root)
        };
        if is_dangerous {
            return Err(format!(
                "path {} is under a dangerous system root; register it under an explicit allowed_roots entry if intended",
                path_str
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ProjectTomlWriteResult {
    config_path: PathBuf,
    created_config: bool,
    overwritten: bool,
}

/// Write a project TOML file atomically into `projects_dir`. Creates
/// `projects_dir` if missing. Returns write metadata on success.
/// The temp file is written and fsynced, then renamed to `<id>.toml`.
fn write_project_toml_atomic(
    projects_dir: &Path,
    id: &str,
    toml_content: &str,
    overwrite: bool,
) -> Result<ProjectTomlWriteResult, String> {
    // Ensure projects_dir exists.
    std::fs::create_dir_all(projects_dir).map_err(|e| {
        format!(
            "failed to create projects_dir {}: {}",
            projects_dir.display(),
            e
        )
    })?;
    let canonical_dir = canonicalize_existing(projects_dir)?;
    let config_path = canonical_dir.join(format!("{}.toml", id));
    // Guard against path escape: the final config path must be inside the
    // canonical projects_dir. The id validation already rejects slashes and
    // dot-dot, but this is a defense-in-depth check.
    if !config_path.starts_with(&canonical_dir) {
        return Err("project config path would escape projects_dir".to_string());
    }
    let existed_before = config_path.exists();
    if existed_before && !overwrite {
        return Err(format!(
            "project config already exists at {}; set overwrite=true to replace",
            config_path.display()
        ));
    }
    let temp_path = canonical_dir.join(format!(".{}.toml.tmp", id));
    {
        let mut file = std::fs::File::create(&temp_path)
            .map_err(|e| format!("failed to create temp file {}: {}", temp_path.display(), e))?;
        file.write_all(toml_content.as_bytes())
            .map_err(|e| format!("failed to write temp file {}: {}", temp_path.display(), e))?;
        let _ = file.sync_all();
    }
    std::fs::rename(&temp_path, &config_path).map_err(|e| {
        format!(
            "failed to rename temp file to {}: {}",
            config_path.display(),
            e
        )
    })?;
    Ok(ProjectTomlWriteResult {
        config_path,
        created_config: !existed_before,
        overwritten: existed_before && overwrite,
    })
}

/// Handle `register_project` / `create_project` agent requests. Parses the
/// JSON payload from `request.stdin`, validates fields and path against
/// policy, writes `projects_dir/<id>.toml` atomically (and for
/// `create_project` creates the directory / templates / optional git init),
/// and returns structured JSON in `CommandResult.stdout`.
pub(crate) fn handle_project_op(
    policy: &AgentPolicy,
    projects_dir: &Path,
    request: &ShellAgentShellRequest,
) -> CommandResult {
    let start = Instant::now();
    let kind = request.kind.as_str();
    let payload = match request.stdin.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("{} request missing stdin payload", kind)),
            };
        }
    };
    let json: serde_json::Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("failed to parse {} payload: {}", kind, e)),
            };
        }
    };
    let get_str = |key: &str| -> Result<String, String> {
        json.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("{} missing required field '{}'", kind, key))
    };
    let id = match get_str("id") {
        Ok(v) => v,
        Err(e) => return err_cmd(start, e),
    };
    let name = match get_str("name") {
        Ok(v) => v,
        Err(e) => return err_cmd(start, e),
    };
    let path = match get_str("path") {
        Ok(v) => v,
        Err(e) => return err_cmd(start, e),
    };
    let description = json
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let allow_patch = json
        .get("allow_patch")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let overwrite = json
        .get("overwrite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if let Err(e) = validate_project_op_id(&id) {
        return err_cmd(start, e);
    }
    if let Err(e) = validate_project_op_name(&name) {
        return err_cmd(start, e);
    }
    if let Some(ref desc) = description {
        if let Err(e) = validate_project_op_description(desc) {
            return err_cmd(start, e);
        }
    }
    if path.is_empty() || path.contains('\0') || !path.starts_with('/') {
        return err_cmd(start, "path must be a non-empty absolute path".to_string());
    }

    let client_id = request.client_id.clone();
    let runtime_id = format!("agent:{}:{}", client_id, id);

    let toml_content = build_project_toml(&id, &name, &path, &description, allow_patch);

    if kind == "register_project" {
        // The directory must exist and be a directory.
        let path_buf = PathBuf::from(&path);
        let canonical = match path_buf.canonicalize() {
            Ok(c) => c,
            Err(e) => {
                return err_cmd(
                    start,
                    format!(
                        "path does not exist or cannot be canonicalized: {}: {}",
                        path, e
                    ),
                );
            }
        };
        if !canonical.is_dir() {
            return err_cmd(start, format!("path {} is not a directory", path));
        }
        if let Err(e) = validate_project_path_policy(policy, &canonical) {
            return err_cmd(start, e);
        }
        let write_result =
            match write_project_toml_atomic(projects_dir, &id, &toml_content, overwrite) {
                Ok(p) => p,
                Err(e) => return err_cmd(start, e),
            };
        let result = serde_json::json!({
            "id": runtime_id,
            "agent_project_id": id,
            "client_id": client_id,
            "name": name,
            "path": path,
            "description": description,
            "projects_config_path": write_result.config_path.to_string_lossy(),
            "created_config": write_result.created_config,
            "overwritten": write_result.overwritten,
            "allow_patch": allow_patch,
        });
        return ok_cmd(start, result);
    }

    // create_project
    let template = json
        .get("template")
        .and_then(|v| v.as_str())
        .unwrap_or("empty")
        .to_string();
    let git_init = json
        .get("git_init")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let allow_existing_empty = json
        .get("allow_existing_empty")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if template != "empty" && template != "basic" {
        return err_cmd(
            start,
            format!("unknown template '{}'; supported: empty, basic", template),
        );
    }

    let path_buf = PathBuf::from(&path);
    let mut created_directory = false;
    let mut created_paths = CreatedProjectPaths::default();

    // Determine the canonical parent for policy validation. If the path exists,
    // canonicalize it directly. If not, canonicalize the existing ancestor.
    let canonical_for_policy = if path_buf.exists() {
        match path_buf.canonicalize() {
            Ok(c) => c,
            Err(e) => {
                return err_cmd(
                    start,
                    format!("path cannot be canonicalized: {}: {}", path, e),
                );
            }
        }
    } else {
        // Find the nearest existing ancestor and canonicalize it.
        let mut ancestor = path_buf.clone();
        while !ancestor.exists() {
            if let Some(parent) = ancestor.parent() {
                ancestor = parent.to_path_buf();
            } else {
                break;
            }
        }
        match ancestor.canonicalize() {
            Ok(c) => c,
            Err(e) => {
                return err_cmd(
                    start,
                    format!(
                        "parent path cannot be canonicalized: {}: {}",
                        ancestor.display(),
                        e
                    ),
                );
            }
        }
    };
    if let Err(e) = validate_project_path_policy(policy, &canonical_for_policy) {
        return err_cmd(start, e);
    }

    // Handle existing vs new directory.
    if path_buf.exists() {
        let meta = match std::fs::metadata(&path_buf) {
            Ok(m) => m,
            Err(e) => return err_cmd(start, format!("failed to stat path {}: {}", path, e)),
        };
        if !meta.is_dir() {
            return err_cmd(
                start,
                format!("path {} exists but is not a directory", path),
            );
        }
        // Check if the directory is empty.
        let is_empty = match std::fs::read_dir(&path_buf) {
            Ok(mut it) => it.next().is_none(),
            Err(e) => {
                return err_cmd(start, format!("failed to read directory {}: {}", path, e));
            }
        };
        if !is_empty {
            return err_cmd(
                start,
                format!("path {} already exists and is not empty", path),
            );
        }
        if !allow_existing_empty {
            return err_cmd(
                start,
                format!(
                    "path {} already exists; set allow_existing_empty=true to use it",
                    path
                ),
            );
        }
    } else {
        // Create the directory.
        if let Err(e) = std::fs::create_dir_all(&path_buf) {
            return err_cmd(start, format!("failed to create directory {}: {}", path, e));
        }
        created_directory = true;
        created_paths.mark_project_dir_created(path_buf.clone());
    }

    // Apply template.
    if template == "basic" {
        let readme = if let Some(ref desc) = description {
            format!("# {}\n\n{}\n", name, desc)
        } else {
            format!("# {}\n", name)
        };
        let readme_path = path_buf.join("README.md");
        if let Err(e) = write_created_file(&readme_path, readme.as_bytes(), &mut created_paths) {
            created_paths.cleanup();
            return err_cmd(start, format!("failed to write README.md: {}", e));
        }
        let gitignore = "target/\nnode_modules/\n.env\n*.log\n";
        let gitignore_path = path_buf.join(".gitignore");
        if let Err(e) =
            write_created_file(&gitignore_path, gitignore.as_bytes(), &mut created_paths)
        {
            created_paths.cleanup();
            return err_cmd(start, format!("failed to write .gitignore: {}", e));
        }
    } else if template == "empty" {
        // For empty template, optionally create README.md if description is provided.
        if let Some(ref desc) = description {
            let readme = format!("# {}\n\n{}\n", name, desc);
            let readme_path = path_buf.join("README.md");
            if let Err(e) = write_created_file(&readme_path, readme.as_bytes(), &mut created_paths)
            {
                created_paths.cleanup();
                return err_cmd(start, format!("failed to write README.md: {}", e));
            }
        }
    }

    // git init.
    let mut git_initialized = false;
    if git_init {
        match Command::new("git")
            .arg("init")
            .current_dir(&path_buf)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
        {
            Ok(output) if output.status.success() => {
                git_initialized = true;
                created_paths.track(path_buf.join(".git"));
            }
            Ok(output) => {
                created_paths.cleanup();
                let stderr = String::from_utf8_lossy(&output.stderr);
                return err_cmd(start, format!("git init failed: {}", stderr.trim()));
            }
            Err(e) => {
                created_paths.cleanup();
                return err_cmd(start, format!("git init failed (is git installed?): {}", e));
            }
        }
    }

    // Write project TOML.
    let write_result = match write_project_toml_atomic(projects_dir, &id, &toml_content, overwrite)
    {
        Ok(p) => p,
        Err(e) => {
            created_paths.cleanup();
            return err_cmd(start, e);
        }
    };
    let result = serde_json::json!({
        "id": runtime_id,
        "agent_project_id": id,
        "client_id": client_id,
        "name": name,
        "path": path,
        "description": description,
        "projects_config_path": write_result.config_path.to_string_lossy(),
        "created_directory": created_directory,
        "created_config": write_result.created_config,
        "overwritten": write_result.overwritten,
        "allow_patch": allow_patch,
        "template": template,
        "git_initialized": git_initialized,
    });
    ok_cmd(start, result)
}
