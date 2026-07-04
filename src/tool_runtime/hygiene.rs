//! `workspace_hygiene_check` — read-only workspace inspection tool.
//!
//! Detects workspace pollution risks before deployment smoke, model handoff,
//! or real development: dirty worktree, untracked temporary/smoke/anchor
//! files, cache directories, secret-like path names, and large untracked
//! files. Never cleans, deletes, restores, or modifies the project. Never
//! reads file contents, env values, tokens, or stdout/stderr bodies.
//! Suspicious secret files are identified by path/name only — contents are
//! never read.

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

use super::helpers::{shell_escape_simple, validate_project_relative_path};
use super::tool_result::ToolResult;
use super::ToolRuntime;

const DEFAULT_MAX_FINDINGS: usize = 50;
const MAX_MAX_FINDINGS: usize = 200;
const LARGE_UNTRACKED_BYTES: u64 = 5 * 1024 * 1024; // 5 MiB
const HYGIENE_SCRIPT_TIMEOUT_SECS: u64 = 30;
const HYGIENE_MAX_SCRIPT_ENTRIES: usize = 500;
const HYGIENE_DIAGNOSTIC_SENTINEL: &str = "@@WEBCODEX_HYGIENE_STATUS@@";
const HYGIENE_MAX_SIZE_PROBE_COMMAND_LEN: usize = 7_000;

/// Kind of hygiene risk identified for a path or the worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HygieneKind {
    TemporaryFile,
    CachePath,
    SecretLikePath,
    LargeUntrackedFile,
    DirtyWorktree,
}

impl HygieneKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::TemporaryFile => "temporary_file",
            Self::CachePath => "cache_path",
            Self::SecretLikePath => "secret_like_path",
            Self::LargeUntrackedFile => "large_untracked_file",
            Self::DirtyWorktree => "dirty_worktree",
        }
    }
}

/// A single bounded hygiene finding. Never carries file contents, env values,
/// tokens, stdout/stderr bodies, or full diffs.
#[derive(Debug, Clone)]
pub(crate) struct HygieneFinding {
    pub(crate) path: String,
    pub(crate) kind: HygieneKind,
    pub(crate) severity: &'static str,
    pub(crate) tracked_status: String,
    pub(crate) reason: String,
    pub(crate) recommendation: String,
}

// =========================================================================
// Pure classification functions (testable without a runtime)
// =========================================================================

/// True if the path name suggests credentials or environment secrets.
/// Path/name based only — never reads file contents.
pub(crate) fn is_secret_like_path(path: &str) -> bool {
    let normalized = path.to_ascii_lowercase();
    let parts: Vec<&str> = normalized
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if parts.is_empty() {
        return false;
    }
    let last = parts.last().copied().unwrap_or("");

    // .env or .env.*
    if last == ".env" || last.starts_with(".env.") {
        return true;
    }
    // Exact SSH key filenames.
    if matches!(last, "id_rsa" | "id_dsa" | "id_ed25519") {
        return true;
    }
    // Secret-like extensions.
    if last.ends_with(".pem")
        || last.ends_with(".key")
        || last.ends_with(".p12")
        || last.ends_with(".pfx")
    {
        return true;
    }
    // Substring matches in any path component.
    for part in &parts {
        if part.contains("secret")
            || part.contains("token")
            || part.contains("credential")
            || part.contains("passwd")
            || part.contains("password")
        {
            return true;
        }
    }
    false
}

/// True if the path matches a known cache/local-state pattern or a top-level
/// generated/dependency directory.
pub(crate) fn is_cache_path(path: &str) -> bool {
    let normalized = path.trim_end_matches('/').to_ascii_lowercase();
    let parts: Vec<&str> = normalized
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if parts.is_empty() {
        return false;
    }
    for part in &parts {
        if matches_cache_component(part) {
            return true;
        }
    }
    // Top-level generated/dependency directories.
    if parts.len() == 1 && matches_top_level_dir(parts[0]) {
        return true;
    }
    false
}

fn matches_cache_component(part: &str) -> bool {
    matches!(
        part,
        "__pycache__"
            | ".pytest_cache"
            | ".mypy_cache"
            | ".ruff_cache"
            | ".cache"
            | "coverage"
            | ".coverage"
            | ".ds_store"
            | "thumbs.db"
    )
}

fn matches_top_level_dir(part: &str) -> bool {
    matches!(
        part,
        "node_modules" | "target" | "vendor" | "dist" | "build" | ".venv" | "venv"
    )
}

/// True if the path name suggests a temporary, smoke, test, or scratch file.
pub(crate) fn is_temporary_file(path: &str) -> bool {
    let normalized = path.to_ascii_lowercase();
    let parts: Vec<&str> = normalized
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if parts.is_empty() {
        return false;
    }
    for part in &parts {
        if part.contains("smoke")
            || part.contains("tmp")
            || part.contains("temp")
            || part.contains("test")
            || part.contains("anchor")
            || part.contains("scratch")
            || part.contains("probe")
            || part.contains("acceptance")
        {
            return true;
        }
    }
    false
}

/// Classify a single path into its highest-priority hygiene risk kind.
///
/// Priority order: secret_like_path > large_untracked_file > cache_path >
/// temporary_file. Only untracked entries are classified for temporary_file
/// and large_untracked_file; the caller decides whether to classify tracked
/// entries at all.
pub(crate) fn classify_hygiene_path(
    path: &str,
    tracked_status: &str,
    size_bytes: Option<u64>,
    large_threshold: u64,
) -> Option<HygieneKind> {
    // Secret-like paths take priority (highest severity).
    if is_secret_like_path(path) {
        return Some(HygieneKind::SecretLikePath);
    }
    // Large untracked files (size metadata only, never read contents).
    if tracked_status == "untracked" {
        if let Some(size) = size_bytes {
            if size > large_threshold {
                return Some(HygieneKind::LargeUntrackedFile);
            }
        }
    }
    // Cache paths.
    if is_cache_path(path) {
        return Some(HygieneKind::CachePath);
    }
    // Temporary files (untracked only — tracked test files are expected).
    if tracked_status == "untracked" && is_temporary_file(path) {
        return Some(HygieneKind::TemporaryFile);
    }
    None
}

/// Severity for a hygiene kind and tracked status.
pub(crate) fn severity_for_hygiene_kind(kind: HygieneKind, tracked_status: &str) -> &'static str {
    match kind {
        HygieneKind::SecretLikePath => {
            if tracked_status == "tracked" {
                "critical"
            } else {
                "high"
            }
        }
        HygieneKind::LargeUntrackedFile => "medium",
        HygieneKind::DirtyWorktree => "medium",
        HygieneKind::CachePath => "low",
        HygieneKind::TemporaryFile => "low",
    }
}

/// Bound findings to `max_findings`, returning the truncated list and whether
/// truncation occurred.
pub(crate) fn bound_hygiene_findings(
    mut findings: Vec<HygieneFinding>,
    max_findings: usize,
) -> (Vec<HygieneFinding>, bool) {
    if findings.len() <= max_findings {
        return (findings, false);
    }
    findings.truncate(max_findings);
    (findings, true)
}

/// Human-readable reason for a hygiene finding.
fn hygiene_reason(kind: HygieneKind, path: &str) -> String {
    match kind {
        HygieneKind::TemporaryFile => "untracked smoke/temporary file".to_string(),
        HygieneKind::CachePath => {
            if matches_top_level_dir(&path.trim_end_matches('/').to_ascii_lowercase()) {
                "large/generated dependency directory".to_string()
            } else {
                "untracked cache or local state path".to_string()
            }
        }
        HygieneKind::SecretLikePath => {
            "path name suggests credentials or environment secrets".to_string()
        }
        HygieneKind::LargeUntrackedFile => {
            format!("untracked file exceeds {} bytes", LARGE_UNTRACKED_BYTES)
        }
        HygieneKind::DirtyWorktree => {
            "git worktree has tracked modifications, staged changes, deletions, or renames"
                .to_string()
        }
    }
}

/// Recommendation for a hygiene finding.
fn hygiene_recommendation(kind: HygieneKind) -> String {
    match kind {
        HygieneKind::TemporaryFile => {
            "review and remove if it was only used for a smoke test".to_string()
        }
        HygieneKind::CachePath => "add to .gitignore or remove if not needed".to_string(),
        HygieneKind::SecretLikePath => {
            "do not print contents; verify it is ignored or remove it from the workspace"
                .to_string()
        }
        HygieneKind::LargeUntrackedFile => {
            "review whether this file should be tracked or ignored".to_string()
        }
        HygieneKind::DirtyWorktree => {
            "review changes with show_changes or git_diff before proceeding".to_string()
        }
    }
}

/// Build the bounded suggested_next_actions list from the findings and git
/// availability.
fn suggested_hygiene_actions(findings: &[HygieneFinding], git_available: bool) -> Vec<Value> {
    let mut actions: Vec<Value> = Vec::new();
    if !git_available {
        actions.push(Value::String(
            "project is not a git repository; git-backed hygiene checks unavailable".to_string(),
        ));
    }
    if findings.is_empty() && git_available {
        actions.push(Value::String(
            "workspace is clean; no hygiene risks detected".to_string(),
        ));
    }
    if !findings.is_empty() {
        actions.push(Value::String(
            "review findings before continuing".to_string(),
        ));
    }
    if findings
        .iter()
        .any(|f| f.kind == HygieneKind::SecretLikePath)
    {
        actions.push(Value::String(
            "do not print or read secret-like file contents; verify they are gitignored or remove them".to_string(),
        ));
    }
    if findings
        .iter()
        .any(|f| f.kind == HygieneKind::TemporaryFile)
    {
        actions.push(Value::String(
            "use discard_untracked only for files you intentionally created".to_string(),
        ));
    }
    if actions.is_empty() {
        actions.push(Value::String("no action needed".to_string()));
    }
    actions
}

/// Build the structured hygiene summary JSON from the resolved inputs.
pub(crate) fn build_hygiene_summary(
    project: &str,
    resolved_project: Option<&str>,
    git_available: bool,
    findings: &[HygieneFinding],
    truncated: bool,
    warnings: &[String],
) -> Value {
    let mut critical = 0u64;
    let mut high = 0u64;
    let mut medium = 0u64;
    let mut low = 0u64;
    let mut untracked_count = 0u64;
    let mut tracked_count = 0u64;
    let mut large_files = 0u64;
    let mut secret_like = 0u64;
    let mut cache_paths = 0u64;

    for finding in findings {
        match finding.severity {
            "critical" => critical += 1,
            "high" => high += 1,
            "medium" => medium += 1,
            "low" => low += 1,
            _ => {}
        }
        if finding.tracked_status == "untracked" {
            untracked_count += 1;
        } else {
            tracked_count += 1;
        }
        match finding.kind {
            HygieneKind::LargeUntrackedFile => large_files += 1,
            HygieneKind::SecretLikePath => secret_like += 1,
            HygieneKind::CachePath => cache_paths += 1,
            _ => {}
        }
    }

    let clean = git_available && findings.is_empty();

    let findings_json: Vec<Value> = findings
        .iter()
        .map(|f| {
            json!({
                "path": f.path,
                "kind": f.kind.as_str(),
                "severity": f.severity,
                "tracked_status": f.tracked_status,
                "reason": f.reason,
                "recommendation": f.recommendation,
            })
        })
        .collect();

    json!({
        "project": project,
        "resolved_project": resolved_project,
        "git_available": git_available,
        "clean": clean,
        "counts": {
            "findings": findings.len(),
            "critical": critical,
            "high": high,
            "medium": medium,
            "low": low,
            "untracked": untracked_count,
            "tracked": tracked_count,
            "large_files": large_files,
            "secret_like_paths": secret_like,
            "cache_paths": cache_paths,
        },
        "findings": findings_json,
        "truncated": truncated,
        "warnings": warnings,
        "suggested_next_actions": suggested_hygiene_actions(findings, git_available),
    })
}

// =========================================================================
// Fixed read-only diagnostic commands
// =========================================================================

pub(crate) fn hygiene_diagnostic_command() -> String {
    format!(
        "git status --porcelain=v1 2>/dev/null; \
         status=$?; \
         printf '\\n{sentinel}%s\\n' \"$status\"; \
         if [ \"$status\" -eq 0 ]; then git ls-files 2>/dev/null; fi; \
         exit 0",
        sentinel = HYGIENE_DIAGNOSTIC_SENTINEL,
    )
}

fn decode_hygiene_porcelain_path(path: &str) -> String {
    let path = path.trim();
    if path.len() >= 2 && path.starts_with('"') && path.ends_with('"') {
        path[1..path.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        path.to_string()
    }
}

fn hygiene_path_is_metadata_safe(path: &str) -> bool {
    let trimmed = path.trim();
    !trimmed.is_empty()
        && trimmed != "."
        && validate_project_relative_path(trimmed).is_ok()
        && !trimmed.split('/').any(|part| part.is_empty())
}

fn parse_hygiene_diagnostic_stdout(stdout: &str) -> (bool, Vec<Value>) {
    let Some((status_stdout, rest)) = stdout.split_once(HYGIENE_DIAGNOSTIC_SENTINEL) else {
        return (false, Vec::new());
    };
    let mut rest_lines = rest.lines();
    let git_available = rest_lines
        .next()
        .and_then(|line| line.trim().parse::<i32>().ok())
        == Some(0);
    if !git_available {
        return (false, Vec::new());
    }

    let mut entries = Vec::new();
    let mut seen_paths: HashSet<String> = HashSet::new();
    for line in status_stdout.lines() {
        if entries.len() >= HYGIENE_MAX_SCRIPT_ENTRIES {
            break;
        }
        if line.len() < 4 {
            continue;
        }
        let x = &line[0..1];
        let y = &line[1..2];
        let path_part = line[3..]
            .split_once(" -> ")
            .map(|(_, dst)| dst)
            .unwrap_or(&line[3..]);
        let path = decode_hygiene_porcelain_path(path_part);
        if path.is_empty() {
            continue;
        }
        let tracked_status = if x == "?" && y == "?" {
            "untracked"
        } else {
            "tracked"
        };
        entries.push(json!({
            "path": path,
            "x": x,
            "y": y,
            "tracked_status": tracked_status,
            "size_bytes": null,
        }));
        seen_paths.insert(path);
    }

    for line in rest_lines {
        if entries.len() >= HYGIENE_MAX_SCRIPT_ENTRIES.saturating_mul(2) {
            break;
        }
        let path = line.trim();
        if path.is_empty() || seen_paths.contains(path) {
            continue;
        }
        entries.push(json!({
            "path": path,
            "x": " ",
            "y": " ",
            "tracked_status": "tracked",
            "size_bytes": null,
        }));
        seen_paths.insert(path.to_string());
    }

    (true, entries)
}

fn collect_local_untracked_sizes(
    root: &std::path::Path,
    entries: &[Value],
) -> HashMap<String, u64> {
    let canonical_root = match root.canonicalize() {
        Ok(root) => root,
        Err(_) => return HashMap::new(),
    };
    let mut sizes = HashMap::new();
    for entry in entries {
        if entry.get("tracked_status").and_then(Value::as_str) != Some("untracked") {
            continue;
        }
        let Some(path) = entry.get("path").and_then(Value::as_str) else {
            continue;
        };
        if !hygiene_path_is_metadata_safe(path) {
            continue;
        }
        let full_path = root.join(path);
        let Ok(metadata) = std::fs::symlink_metadata(&full_path) else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }
        let Ok(canonical) = full_path.canonicalize() else {
            continue;
        };
        if canonical.starts_with(&canonical_root) {
            sizes.insert(path.to_string(), metadata.len());
        }
    }
    sizes
}

fn hygiene_size_probe_command(paths: &[String]) -> String {
    let mut command = String::new();
    for path in paths {
        command.push_str("p=");
        command.push_str(&shell_escape_simple(path));
        command.push_str(
            "; if [ -f \"$p\" ] && [ ! -L \"$p\" ]; then \
                 bytes=$(wc -c < \"$p\" 2>/dev/null | tr -d '[:space:]'); \
                 case \"$bytes\" in ''|*[!0-9]*) ;; *) printf '%s\\t%s\\n' \"$bytes\" \"$p\" ;; esac; \
               fi\n",
        );
    }
    command
}

fn parse_hygiene_size_probe_stdout(stdout: &str) -> HashMap<String, u64> {
    let mut sizes = HashMap::new();
    for line in stdout.lines() {
        let Some((size, path)) = line.split_once('\t') else {
            continue;
        };
        let Ok(size) = size.parse::<u64>() else {
            continue;
        };
        if hygiene_path_is_metadata_safe(path) {
            sizes.insert(path.to_string(), size);
        }
    }
    sizes
}

fn hygiene_size_probe_batches(entries: &[Value]) -> Vec<Vec<String>> {
    let mut batches: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut current_len = 0usize;
    for entry in entries.iter().take(HYGIENE_MAX_SCRIPT_ENTRIES) {
        if entry.get("tracked_status").and_then(Value::as_str) != Some("untracked") {
            continue;
        }
        let Some(path) = entry.get("path").and_then(Value::as_str) else {
            continue;
        };
        if !hygiene_path_is_metadata_safe(path) {
            continue;
        }
        let escaped_len = shell_escape_simple(path).len().saturating_add(180);
        if !current.is_empty()
            && current_len.saturating_add(escaped_len) > HYGIENE_MAX_SIZE_PROBE_COMMAND_LEN
        {
            batches.push(std::mem::take(&mut current));
            current_len = 0;
        }
        current.push(path.to_string());
        current_len = current_len.saturating_add(escaped_len);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

// =========================================================================
// Runtime method
// =========================================================================

impl ToolRuntime {
    pub(crate) async fn workspace_hygiene_check(
        &self,
        project: String,
        max_findings: Option<usize>,
        include_tracked: Option<bool>,
        session_id: Option<String>,
    ) -> ToolResult {
        let _ = session_id; // Recorded by the dispatch layer.

        let max_findings = max_findings
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_MAX_FINDINGS)
            .clamp(1, MAX_MAX_FINDINGS);
        let include_tracked = include_tracked.unwrap_or(false);

        // Resolve the project to get the canonical resolved id for the output.
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let resolved_project = resolved.resolved_id.clone();

        // Run fixed read-only git diagnostics. The command always exits 0 and
        // carries the git status code in-band so non-git projects degrade to a
        // structured hygiene response.
        let command = hygiene_diagnostic_command();
        let output = match self
            .run_project_command_capture(&project, command, HYGIENE_SCRIPT_TIMEOUT_SECS, None)
            .await
        {
            Ok(output) => output,
            Err(e) => return ToolResult::err(e),
        };

        let (git_available, entries) = parse_hygiene_diagnostic_stdout(&output.stdout);
        let size_bytes_by_path = if git_available {
            match self.resolve_project(&project).await {
                Ok(proj) if proj.is_agent() => {
                    let mut sizes = HashMap::new();
                    for batch in hygiene_size_probe_batches(&entries) {
                        let command = hygiene_size_probe_command(&batch);
                        if let Ok(output) = self
                            .run_project_command_capture(
                                &project,
                                command,
                                HYGIENE_SCRIPT_TIMEOUT_SECS,
                                None,
                            )
                            .await
                        {
                            sizes.extend(parse_hygiene_size_probe_stdout(&output.stdout));
                        }
                    }
                    sizes
                }
                Ok(proj) => collect_local_untracked_sizes(&proj.root(), &entries),
                Err(_) => HashMap::new(),
            }
        } else {
            HashMap::new()
        };

        // Build findings from the entries.
        let mut findings: Vec<HygieneFinding> = Vec::new();
        let mut has_tracked_changes = false;

        for entry in entries.iter().take(HYGIENE_MAX_SCRIPT_ENTRIES) {
            let path = entry.get("path").and_then(Value::as_str).unwrap_or("");
            let tracked_status = entry
                .get("tracked_status")
                .and_then(Value::as_str)
                .unwrap_or("untracked");
            let size_bytes = entry
                .get("size_bytes")
                .and_then(Value::as_u64)
                .or_else(|| size_bytes_by_path.get(path).copied());
            let x = entry.get("x").and_then(Value::as_str).unwrap_or(" ");
            let y = entry.get("y").and_then(Value::as_str).unwrap_or(" ");

            if path.is_empty() {
                continue;
            }

            // Only entries from `git status` with non-space codes are actual
            // tracked changes (modified/staged/deleted/renamed). Clean tracked
            // files from `git ls-files` have space codes and do NOT count.
            if tracked_status == "tracked" && (x != " " || y != " ") {
                has_tracked_changes = true;
            }

            if tracked_status == "untracked" {
                if let Some(kind) =
                    classify_hygiene_path(path, tracked_status, size_bytes, LARGE_UNTRACKED_BYTES)
                {
                    let severity = severity_for_hygiene_kind(kind, tracked_status);
                    findings.push(HygieneFinding {
                        path: path.to_string(),
                        kind,
                        severity,
                        tracked_status: tracked_status.to_string(),
                        reason: hygiene_reason(kind, path),
                        recommendation: hygiene_recommendation(kind),
                    });
                }
            } else if include_tracked && is_secret_like_path(path) {
                // For tracked entries, only report secret-like path names
                // (suspicious tracked files). Never read contents.
                let kind = HygieneKind::SecretLikePath;
                let severity = severity_for_hygiene_kind(kind, tracked_status);
                findings.push(HygieneFinding {
                    path: path.to_string(),
                    kind,
                    severity,
                    tracked_status: tracked_status.to_string(),
                    reason: hygiene_reason(kind, path),
                    recommendation: hygiene_recommendation(kind),
                });
            }
        }

        // Add a single dirty_worktree summary finding if there are tracked
        // changes (modified/staged/deleted/renamed).
        if has_tracked_changes {
            findings.push(HygieneFinding {
                path: "(worktree)".to_string(),
                kind: HygieneKind::DirtyWorktree,
                severity: severity_for_hygiene_kind(HygieneKind::DirtyWorktree, "tracked"),
                tracked_status: "tracked".to_string(),
                reason: hygiene_reason(HygieneKind::DirtyWorktree, ""),
                recommendation: hygiene_recommendation(HygieneKind::DirtyWorktree),
            });
        }

        // Bound the findings.
        let (findings, truncated) = bound_hygiene_findings(findings, max_findings);

        // Build warnings.
        let warnings: Vec<String> = if git_available {
            Vec::new()
        } else {
            vec!["non_git_project".to_string()]
        };

        let summary = build_hygiene_summary(
            &project,
            Some(&resolved_project),
            git_available,
            &findings,
            truncated,
            &warnings,
        );

        ToolResult::ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_like_path_detection() {
        assert!(is_secret_like_path(".env"));
        assert!(is_secret_like_path(".env.local"));
        assert!(is_secret_like_path(".env.production"));
        assert!(is_secret_like_path("secrets/api.key"));
        assert!(is_secret_like_path("config/token.json"));
        assert!(is_secret_like_path("id_rsa"));
        assert!(is_secret_like_path("id_ed25519"));
        assert!(is_secret_like_path("cert.pem"));
        assert!(is_secret_like_path("private.key"));
        assert!(is_secret_like_path("cert.p12"));
        assert!(is_secret_like_path("cert.pfx"));
        assert!(is_secret_like_path("config/credentials.yaml"));
        assert!(is_secret_like_path("passwd"));
        assert!(is_secret_like_path(".password"));

        assert!(!is_secret_like_path("src/main.rs"));
        assert!(!is_secret_like_path("README.md"));
        assert!(!is_secret_like_path("environment.rs"));
    }

    #[test]
    fn cache_path_detection() {
        assert!(is_cache_path("__pycache__/"));
        assert!(is_cache_path(".pytest_cache/"));
        assert!(is_cache_path(".mypy_cache/"));
        assert!(is_cache_path(".ruff_cache/"));
        assert!(is_cache_path("src/.cache"));
        assert!(is_cache_path(".coverage"));
        assert!(is_cache_path("coverage/"));
        assert!(is_cache_path(".DS_Store"));
        assert!(is_cache_path("Thumbs.db"));
        assert!(is_cache_path("node_modules/"));
        assert!(is_cache_path("target/"));
        assert!(is_cache_path("vendor/"));
        assert!(is_cache_path("dist/"));
        assert!(is_cache_path("build/"));
        assert!(is_cache_path(".venv/"));
        assert!(is_cache_path("venv/"));

        assert!(!is_cache_path("src/main.rs"));
        assert!(!is_cache_path("README.md"));
    }

    #[test]
    fn temporary_file_detection() {
        assert!(is_temporary_file(".webcodex-smoke-acceptance.txt"));
        assert!(is_temporary_file("tmp/foo.txt"));
        assert!(is_temporary_file("scratch.txt"));
        assert!(is_temporary_file("test_probe.py"));
        assert!(is_temporary_file("anchor.txt"));
        assert!(is_temporary_file("acceptance_result.json"));

        assert!(!is_temporary_file("src/main.rs"));
        assert!(!is_temporary_file("README.md"));
    }

    #[test]
    fn classify_prioritizes_secret_over_large() {
        let kind = classify_hygiene_path(
            ".env.local",
            "untracked",
            Some(10 * 1024 * 1024),
            LARGE_UNTRACKED_BYTES,
        );
        assert_eq!(kind, Some(HygieneKind::SecretLikePath));
    }

    #[test]
    fn classify_large_untracked_file() {
        let kind = classify_hygiene_path(
            "big.bin",
            "untracked",
            Some(6 * 1024 * 1024),
            LARGE_UNTRACKED_BYTES,
        );
        assert_eq!(kind, Some(HygieneKind::LargeUntrackedFile));
    }

    #[test]
    fn classify_temporary_file_untracked_only() {
        let kind = classify_hygiene_path("scratch.txt", "untracked", None, LARGE_UNTRACKED_BYTES);
        assert_eq!(kind, Some(HygieneKind::TemporaryFile));

        // Tracked temporary files are not classified (tracked test files are expected).
        let kind = classify_hygiene_path("scratch.txt", "tracked", None, LARGE_UNTRACKED_BYTES);
        assert_eq!(kind, None);
    }

    #[test]
    fn classify_cache_path() {
        let kind =
            classify_hygiene_path(".pytest_cache/", "untracked", None, LARGE_UNTRACKED_BYTES);
        assert_eq!(kind, Some(HygieneKind::CachePath));
    }

    #[test]
    fn severity_for_secret_like_path() {
        assert_eq!(
            severity_for_hygiene_kind(HygieneKind::SecretLikePath, "untracked"),
            "high"
        );
        assert_eq!(
            severity_for_hygiene_kind(HygieneKind::SecretLikePath, "tracked"),
            "critical"
        );
    }

    #[test]
    fn bound_findings_truncates() {
        let findings: Vec<HygieneFinding> = (0..60)
            .map(|i| HygieneFinding {
                path: format!("tmp/file_{i}.txt"),
                kind: HygieneKind::TemporaryFile,
                severity: "low",
                tracked_status: "untracked".to_string(),
                reason: "test".to_string(),
                recommendation: "test".to_string(),
            })
            .collect();
        let (bounded, truncated) = bound_hygiene_findings(findings, 10);
        assert_eq!(bounded.len(), 10);
        assert!(truncated);
    }

    #[test]
    fn bound_findings_no_truncation() {
        let findings = vec![HygieneFinding {
            path: "tmp/file.txt".to_string(),
            kind: HygieneKind::TemporaryFile,
            severity: "low",
            tracked_status: "untracked".to_string(),
            reason: "test".to_string(),
            recommendation: "test".to_string(),
        }];
        let (bounded, truncated) = bound_hygiene_findings(findings, 50);
        assert_eq!(bounded.len(), 1);
        assert!(!truncated);
    }

    #[test]
    fn build_summary_clean_git_repo() {
        let summary = build_hygiene_summary(
            "agent:oe:demo",
            Some("agent:oe:demo"),
            true,
            &[],
            false,
            &[],
        );
        assert_eq!(summary["git_available"], true);
        assert_eq!(summary["clean"], true);
        assert_eq!(summary["counts"]["findings"], 0);
        assert_eq!(summary["truncated"], false);
        assert!(summary["warnings"].as_array().unwrap().is_empty());
    }

    #[test]
    fn build_summary_non_git_project() {
        let summary = build_hygiene_summary(
            "agent:oe:demo",
            Some("agent:oe:demo"),
            false,
            &[],
            false,
            &["non_git_project".to_string()],
        );
        assert_eq!(summary["git_available"], false);
        assert_eq!(summary["clean"], false);
        assert!(summary["warnings"].as_array().unwrap().len() >= 1);
    }

    #[test]
    fn build_summary_with_findings() {
        let findings = vec![
            HygieneFinding {
                path: ".env.local".to_string(),
                kind: HygieneKind::SecretLikePath,
                severity: "high",
                tracked_status: "untracked".to_string(),
                reason: "test".to_string(),
                recommendation: "test".to_string(),
            },
            HygieneFinding {
                path: "scratch.txt".to_string(),
                kind: HygieneKind::TemporaryFile,
                severity: "low",
                tracked_status: "untracked".to_string(),
                reason: "test".to_string(),
                recommendation: "test".to_string(),
            },
        ];
        let summary = build_hygiene_summary(
            "agent:oe:demo",
            Some("agent:oe:demo"),
            true,
            &findings,
            false,
            &[],
        );
        assert_eq!(summary["clean"], false);
        assert_eq!(summary["counts"]["findings"], 2);
        assert_eq!(summary["counts"]["high"], 1);
        assert_eq!(summary["counts"]["low"], 1);
        assert_eq!(summary["counts"]["secret_like_paths"], 1);
        assert_eq!(summary["counts"]["untracked"], 2);
        assert_eq!(summary["findings"].as_array().unwrap().len(), 2);
    }
}
