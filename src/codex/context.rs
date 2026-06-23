use super::get_projects;
use super::shell::{run_command, shell_escape};
use super::types::{
    ContextBatchItem, ContextBatchRequest, ContextBatchResponse, ContextBatchResultMetadata,
    ContextMode, ContextRequest, ContextResponse,
};
use super::{agent_context_shell_fragment, truncate_string, MAX_OUTPUT_LEN};
use crate::action_sessions::{
    record_action_event, request_action_session_id, ActionAuditEventInput,
};
use crate::get_db;
use crate::projects::{canonicalize_and_verify, ProjectConfig};
use salvo::prelude::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::{Component, Path};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub(super) const IGNORED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "dist",
    "build",
    ".cache",
    "__pycache__",
];
pub(super) const MAX_TREE_ITEMS: usize = 300;
pub(super) const MAX_SEARCH_RESULTS: usize = 50;
pub(super) const MAX_CONTEXT_LINE_LEN: usize = 2_000;
pub(super) const MAX_TREE_DEPTH: usize = 8;
pub(super) const MAX_READ_FILE_LIMIT: usize = 2_000;
const CONTEXT_MAX_OUTPUT_LEN: usize = 50_000;

/// Server hard limit for max_total_chars — requests above this are rejected by preflight.
const PREFLIGHT_MAX_TOTAL_CHARS: usize = 180_000;
/// Recommended max_total_chars for compact/GPT usage.
const PREFLIGHT_RECOMMENDED_MAX_CHARS: usize = 120_000;
/// Max batch items for local projects before warning.
const PREFLIGHT_LOCAL_MAX_ITEMS: usize = 24;
/// Hard max batch items for SSH projects — requests above this are rejected.
const PREFLIGHT_SSH_MAX_ITEMS: usize = 12;
/// Max read_file limit before preflight rejection.
const PREFLIGHT_MAX_READ_FILE_LIMIT: usize = 800;

pub(super) fn truncate_context_line(line: &str) -> (String, bool) {
    if line.len() <= MAX_CONTEXT_LINE_LEN {
        return (line.to_string(), false);
    }
    let mut end = MAX_CONTEXT_LINE_LEN;
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }
    (format!("{}… [line truncated]", &line[..end]), true)
}

pub(super) fn format_context_line(line_no: usize, line: &str) -> (String, bool) {
    let (line, truncated) = truncate_context_line(line);
    (format!("{:4} | {}", line_no, line), truncated)
}

pub(super) fn git_status_command() -> &'static str {
    "git status --short --untracked-files=no"
}

fn truncate_output_string(s: String, max_len: usize) -> (String, bool) {
    if s.len() <= max_len {
        (s, false)
    } else {
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        (s[..end].to_string(), true)
    }
}

pub(super) fn normalize_tree_depth(max_depth: usize) -> usize {
    max_depth.clamp(1, MAX_TREE_DEPTH)
}

pub(super) fn normalize_tree_limit(limit: usize) -> usize {
    limit.clamp(1, MAX_TREE_ITEMS)
}

pub(super) fn is_ignored_dir(name: &str) -> bool {
    IGNORED_DIRS.contains(&name) || name.starts_with('.')
}

pub(super) fn collect_tree(
    dir: &Path,
    base: &Path,
    items: &mut Vec<String>,
    limit: usize,
    max_depth: usize,
) {
    if items.len() >= limit || max_depth == 0 {
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
            collect_tree(&path, base, items, limit, max_depth - 1);
        } else {
            items.push(rel);
        }
    }
}

pub(super) fn simple_search(dir: &Path, query: &str, limit: usize) -> Vec<String> {
    let mut results = Vec::new();
    search_recursive(dir, dir, query, &mut results, limit);
    results
}

fn grep_context_file(
    path: &Path,
    base: &Path,
    query: &str,
    output: &mut Vec<String>,
    limit: usize,
) {
    if output.len() >= limit {
        return;
    }
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return,
    };
    if metadata.len() > 1_000_000 {
        return;
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let lines: Vec<&str> = content.lines().collect();
    let rel = path
        .strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();
    let mut last_end = 0usize;
    for (idx, line) in lines.iter().enumerate() {
        if output.len() >= limit {
            return;
        }
        if !line.contains(query) {
            continue;
        }
        let start = idx.saturating_sub(3);
        let end = (idx + 4).min(lines.len());
        if start > last_end && !output.is_empty() {
            output.push("--".to_string());
            if output.len() >= limit {
                return;
            }
        }
        for line_idx in start.max(last_end)..end {
            if output.len() >= limit {
                return;
            }
            let marker = if line_idx == idx { ">" } else { " " };
            let (line, _) = truncate_context_line(lines[line_idx]);
            output.push(format!("{}:{}{} | {}", rel, line_idx + 1, marker, line));
        }
        last_end = end;
    }
}

fn grep_context_recursive(
    dir: &Path,
    base: &Path,
    query: &str,
    output: &mut Vec<String>,
    limit: usize,
) {
    if output.len() >= limit {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    sorted.sort_by_key(|e| e.file_name());
    for entry in sorted {
        if output.len() >= limit {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if is_ignored_dir(&name) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            grep_context_recursive(&path, base, query, output, limit);
        } else if path.is_file() {
            grep_context_file(&path, base, query, output, limit);
        }
    }
}

pub(super) fn local_grep_context(
    root: &Path,
    project_name: &str,
    rel_path: Option<&str>,
    query: &str,
    limit: usize,
) -> ContextResponse {
    let limit = limit.clamp(1, MAX_READ_FILE_LIMIT);
    let start = match rel_path {
        Some(path) => match canonicalize_and_verify(&root.join(path), root) {
            Ok(path) => path,
            Err(e) => return context_error(project_name, &ContextMode::GrepContext, e),
        },
        None => root.to_path_buf(),
    };
    let mut output = Vec::new();
    if start.is_file() {
        grep_context_file(&start, root, query, &mut output, limit);
    } else {
        grep_context_recursive(&start, root, query, &mut output, limit);
    }
    let truncated = output.len() >= limit;
    mode_content_response(
        project_name,
        "grep_context",
        output.join("\n"),
        MAX_OUTPUT_LEN,
    )
    .tap_truncated(truncated)
}

trait ContextResponseExt {
    fn tap_truncated(self, truncated: bool) -> Self;
}

impl ContextResponseExt for ContextResponse {
    fn tap_truncated(mut self, truncated: bool) -> Self {
        self.truncated = self.truncated || truncated;
        self
    }
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

pub(super) fn mode_name(mode: &ContextMode) -> &'static str {
    match mode {
        ContextMode::Overview => "overview",
        ContextMode::Tree => "tree",
        ContextMode::Search => "search",
        ContextMode::GrepContext => "grep_context",
        ContextMode::ReadFile => "read_file",
        ContextMode::MarkdownOutline => "markdown_outline",
        ContextMode::ReadSection => "read_section",
        ContextMode::AgentContext => "agent_context",
        ContextMode::GitStatus => "git_status",
        ContextMode::GitDiff => "git_diff",
        ContextMode::ExperimentOutputs => "experiment_outputs",
    }
}

pub(super) fn system_time_unix_ms(time: SystemTime) -> Option<u64> {
    let ms = time.duration_since(UNIX_EPOCH).ok()?.as_millis();
    Some(ms.min(u128::from(u64::MAX)) as u64)
}

pub(super) fn file_fingerprint(
    prefix: &str,
    rel_path: &str,
    file_size: u64,
    modified_unix_ms: u64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(rel_path.as_bytes());
    hasher.update([0]);
    hasher.update(file_size.to_string().as_bytes());
    hasher.update([0]);
    hasher.update(modified_unix_ms.to_string().as_bytes());
    let digest = hasher.finalize();
    let short = digest[..12]
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>();
    format!("{}-{}", prefix, short)
}

pub(super) fn content_sha256_fingerprint(prefix: &str, rel_path: &str, sha256_hex: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(rel_path.as_bytes());
    hasher.update([0]);
    hasher.update(sha256_hex.trim().as_bytes());
    let digest = hasher.finalize();
    let short = digest[..12]
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>();
    format!("{}-{}", prefix, short)
}

fn local_read_file_metadata(
    root: &Path,
    rel_path: &str,
    request_index: usize,
    unchanged: bool,
) -> Result<ContextBatchResultMetadata, String> {
    let canonical = canonicalize_and_verify(&root.join(rel_path), root)?;
    let metadata =
        std::fs::metadata(&canonical).map_err(|e| format!("Failed to stat file: {}", e))?;
    if !metadata.is_file() {
        return Err("path is not a file".to_string());
    }
    let file_size = metadata.len();
    let modified_unix_ms = metadata
        .modified()
        .ok()
        .and_then(system_time_unix_ms)
        .unwrap_or(0);
    Ok(ContextBatchResultMetadata {
        request_index,
        mode: "read_file".to_string(),
        path: Some(rel_path.to_string()),
        fingerprint: Some(file_fingerprint(
            "local-v1",
            rel_path,
            file_size,
            modified_unix_ms,
        )),
        unchanged,
        file_size: Some(file_size),
        modified_unix_ms: Some(modified_unix_ms),
        total_lines: None,
    })
}

fn local_read_file_cache_hit_response(
    root: &Path,
    project_name: &str,
    item: &ContextBatchItem,
    request_index: usize,
) -> Option<(ContextResponse, ContextBatchResultMetadata)> {
    if !matches!(item.mode, ContextMode::ReadFile) {
        return None;
    }
    let rel_path = item.path.as_deref()?;
    let expected = item.if_fingerprint.as_deref()?.trim();
    if expected.is_empty() || validate_read_file_range(item.start_line, item.limit).is_err() {
        return None;
    }
    let mut metadata = local_read_file_metadata(root, rel_path, request_index, false).ok()?;
    if metadata.fingerprint.as_deref() != Some(expected) {
        return None;
    }
    metadata.unchanged = true;
    Some((
        ContextResponse {
            success: true,
            project: project_name.to_string(),
            mode: "read_file".to_string(),
            content: None,
            items: None,
            truncated: false,
            error: None,
        },
        metadata,
    ))
}

fn context_batch_recommended_next_action(
    success: bool,
    results: &[ContextResponse],
    preflight_rejected: bool,
    cache_hits: usize,
) -> String {
    if preflight_rejected {
        return "Split the context_batch by file or section, then retry only the needed pieces."
            .to_string();
    }
    if !success {
        return "Retry only failed result indexes or switch to narrower read_file/read_section requests."
            .to_string();
    }
    if results.iter().any(|result| result.truncated) {
        return "Use narrower read_file ranges or read_section before editing truncated areas."
            .to_string();
    }
    if cache_hits > 0 {
        return "Proceed with cached context; use applyProjectEdit or runJobOp for the next change."
            .to_string();
    }
    "Use applyProjectEdit for targeted changes, or runJobOp for long checks/builds.".to_string()
}

fn context_batch_action_budget_hint(cache_hits: usize) -> String {
    if cache_hits > 0 {
        "Some read_file content was omitted via if_fingerprint; keep reusing fingerprints for unchanged files."
            .to_string()
    } else {
        "Batch related reads and pass result_metadata.fingerprint as if_fingerprint on repeated read_file calls."
            .to_string()
    }
}

pub(super) fn context_error(project: &str, mode: &ContextMode, error: String) -> ContextResponse {
    ContextResponse {
        success: false,
        project: project.to_string(),
        mode: mode_name(mode).to_string(),
        content: None,
        items: None,
        truncated: false,
        error: Some(error),
    }
}

pub(super) fn validate_read_file_range(start_line: usize, limit: usize) -> Result<usize, String> {
    if start_line == 0 {
        return Err("start_line must be >= 1".to_string());
    }
    if limit == 0 {
        return Err("limit must be >= 1".to_string());
    }
    if limit > MAX_READ_FILE_LIMIT {
        return Err(format!("limit must be <= {}", MAX_READ_FILE_LIMIT));
    }
    start_line
        .checked_add(limit - 1)
        .ok_or_else(|| "start_line + limit - 1 overflowed".to_string())
}

pub(super) const AGENT_CONTEXT_FILES: &[&str] = &[
    "AGENTS.md",
    ".codex/memory/project.md",
    ".codex/memory/pitfalls.md",
    ".codex/memory/workflows.md",
    ".codex/memory/decisions.md",
    ".codex/memory/user_preferences.md",
];

pub(super) fn mode_content_response(
    project_name: &str,
    mode: &str,
    content: String,
    max_len: usize,
) -> ContextResponse {
    let (content, truncated) = truncate_output_string(content, max_len);
    ContextResponse {
        success: true,
        project: project_name.to_string(),
        mode: mode.to_string(),
        content: Some(content),
        items: None,
        truncated,
        error: None,
    }
}

pub(super) fn local_agent_context(root: &Path, project_name: &str) -> ContextResponse {
    let mut content = format!(
        "# Agent context for {}\n\nLoaded project rules and memory files for alignment before planning or editing.\n",
        project_name
    );
    for rel in AGENT_CONTEXT_FILES {
        content.push_str(&format!("\n## {}\n\n", rel));
        let path = root.join(rel);
        match canonicalize_and_verify(&path, root) {
            Ok(canonical) => match std::fs::read_to_string(&canonical) {
                Ok(text) => content.push_str(text.trim_end()),
                Err(_) => content.push_str("(missing)"),
            },
            Err(_) => content.push_str("(missing)"),
        }
        content.push('\n');
    }
    mode_content_response(
        project_name,
        "agent_context",
        content,
        CONTEXT_MAX_OUTPUT_LEN,
    )
}

fn markdown_outline_from_text(project_name: &str, text: &str, limit: usize) -> ContextResponse {
    let max = limit.clamp(1, MAX_READ_FILE_LIMIT);
    let mut lines = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed.chars().nth(hashes) == Some(' ') {
            lines.push(format!("{:4} | {}", idx + 1, trimmed));
            if lines.len() >= max {
                break;
            }
        }
    }
    let truncated = lines.len() >= max;
    ContextResponse {
        success: true,
        project: project_name.to_string(),
        mode: "markdown_outline".to_string(),
        content: Some(lines.join("\n")),
        items: None,
        truncated,
        error: None,
    }
}

fn markdown_section_from_text(
    project_name: &str,
    text: &str,
    query: &str,
    limit: usize,
) -> ContextResponse {
    let max = limit.clamp(1, MAX_READ_FILE_LIMIT);
    let query_lower = query.to_lowercase();
    let mut found = false;
    let mut level = 0usize;
    let mut selected = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        let is_heading = (1..=6).contains(&hashes) && trimmed.chars().nth(hashes) == Some(' ');
        if is_heading {
            if found && hashes <= level {
                break;
            }
            if !found && trimmed.to_lowercase().contains(&query_lower) {
                found = true;
                level = hashes;
            }
        }
        if found {
            if selected.len() >= max {
                break;
            }
            selected.push(format!("{:4} | {}", idx + 1, line));
        }
    }
    if !found {
        return ContextResponse {
            success: false,
            project: project_name.to_string(),
            mode: "read_section".to_string(),
            content: None,
            items: None,
            truncated: false,
            error: Some(format!("Section not found: {}", query)),
        };
    }
    let truncated = selected.len() >= max;
    ContextResponse {
        success: true,
        project: project_name.to_string(),
        mode: "read_section".to_string(),
        content: Some(selected.join("\n")),
        items: None,
        truncated,
        error: None,
    }
}

pub(super) fn enforce_context_batch_total_limit(
    results: &mut [ContextResponse],
    max_total_chars: usize,
) {
    let max_total = max_total_chars.clamp(4_000, 200_000);
    let mut used = 0usize;
    for result in results {
        if let Some(content) = result.content.as_mut() {
            if used >= max_total {
                content.clear();
                result.truncated = true;
                result.error = Some("Omitted: max_total_chars budget exceeded".to_string());
                continue;
            }
            let remaining = max_total - used;
            if content.len() > remaining {
                let (truncated_content, _) = truncate_output_string(content.clone(), remaining);
                *content = truncated_content;
                result.truncated = true;
                result.error = Some("Omitted: max_total_chars budget exceeded".to_string());
                used = max_total;
            } else {
                used += content.len();
            }
        }
        if let Some(items) = result.items.as_mut() {
            let mut kept = Vec::new();
            for item in items.iter() {
                if used + item.len() + 1 > max_total {
                    result.truncated = true;
                    if result.error.is_none() {
                        result.error = Some("Omitted: max_total_chars budget exceeded".to_string());
                    }
                    break;
                }
                used += item.len() + 1;
                kept.push(item.clone());
            }
            *items = kept;
        }
    }
}

/// Lightweight cost estimate for a context batch request.
/// Used to reject obviously oversized requests before execution.
pub(super) struct BatchCostEstimate {
    /// Estimated total character output.
    pub estimated_chars: usize,
    /// Number of batch items.
    #[allow(dead_code)]
    pub item_count: usize,
    /// Whether any item has a very large limit (e.g., read_file limit > 800).
    pub has_oversized_item: bool,
    /// Warnings generated during estimation (not blocking).
    pub warnings: Vec<String>,
}

/// Estimate the cost of a context batch request before execution.
/// This is a rough heuristic — the goal is to catch obviously oversized requests,
/// not to predict exact output sizes.
pub(super) fn estimate_context_batch_cost(
    req: &ContextBatchRequest,
    project_is_ssh: bool,
) -> BatchCostEstimate {
    let mut estimated_chars = 0usize;
    let mut has_oversized_item = false;
    let mut warnings = Vec::new();

    for item in &req.requests {
        let item_est = match item.mode {
            ContextMode::Overview => 5_000,
            ContextMode::Tree => {
                let depth_factor = if item.max_depth > 5 { 1.5 } else { 1.0 };
                ((item.limit.min(MAX_TREE_ITEMS)) as f64 * 120.0 * depth_factor) as usize
            }
            ContextMode::Search => (item.limit.min(MAX_SEARCH_RESULTS) as f64 * 200.0) as usize,
            ContextMode::GrepContext => (item.limit as f64 * 250.0) as usize,
            ContextMode::ReadFile => {
                let eff_limit = item.limit.min(MAX_READ_FILE_LIMIT);
                if eff_limit > PREFLIGHT_MAX_READ_FILE_LIMIT {
                    has_oversized_item = true;
                }
                eff_limit * 120
            }
            ContextMode::MarkdownOutline => (item.limit as f64 * 80.0) as usize,
            ContextMode::ReadSection => (item.limit as f64 * 150.0) as usize,
            ContextMode::AgentContext => 50_000,
            ContextMode::GitStatus => 8_000,
            ContextMode::GitDiff => 40_000,
            ContextMode::ExperimentOutputs => 30_000,
        };
        estimated_chars = estimated_chars.saturating_add(item_est);
    }

    // Warn if local batch has many items
    if !project_is_ssh && req.requests.len() > PREFLIGHT_LOCAL_MAX_ITEMS {
        warnings.push(format!(
            "Batch has {} items; consider splitting into smaller batches of at most {}.",
            req.requests.len(),
            PREFLIGHT_LOCAL_MAX_ITEMS
        ));
    }

    BatchCostEstimate {
        estimated_chars,
        item_count: req.requests.len(),
        has_oversized_item,
        warnings,
    }
}

/// Run preflight checks on a context batch request.
/// Returns `Ok(warnings)` if the request is allowed, or `Err(response)` if it should be rejected.
pub(super) fn preflight_context_batch(
    req: &ContextBatchRequest,
    project_is_ssh: bool,
    project_name: &str,
) -> Result<Vec<String>, ContextBatchResponse> {
    let estimate = estimate_context_batch_cost(req, project_is_ssh);
    let mut warnings = estimate.warnings.clone();

    // Check 1: max_total_chars exceeds server hard limit
    if req.max_total_chars > PREFLIGHT_MAX_TOTAL_CHARS {
        return Err(ContextBatchResponse {
            success: false,
            project: project_name.to_string(),
            results: Vec::new(),
            duration_ms: 0,
            ssh_calls: 0,
            error: Some(format!(
                "context batch too large: max_total_chars={} exceeds server limit {}",
                req.max_total_chars, PREFLIGHT_MAX_TOTAL_CHARS
            )),
            preflight_rejected: Some(true),
            estimated_chars: Some(estimate.estimated_chars),
            max_allowed_chars: Some(PREFLIGHT_MAX_TOTAL_CHARS),
            max_allowed_items: None,
            project_is_ssh: Some(project_is_ssh),
            suggestion: Some(format!(
                "Reduce max_total_chars to {} or below. Split this request into smaller batches.",
                PREFLIGHT_RECOMMENDED_MAX_CHARS
            )),
            warnings: Vec::new(),
            result_metadata: Vec::new(),
            cache_hits: None,
            recommended_next_action: Some(context_batch_recommended_next_action(
                false,
                &[],
                true,
                0,
            )),
            action_budget_hint: Some(context_batch_action_budget_hint(0)),
        });
    }

    // Check 2: estimated output exceeds max_total_chars significantly.
    // This catches requests where the sum of per-item estimates far exceeds what the client
    // asked for — even if they set max_total_chars under the server limit.
    if estimate.estimated_chars > req.max_total_chars.saturating_mul(3) {
        if project_is_ssh && estimate.estimated_chars > req.max_total_chars.saturating_mul(5) {
            return Err(ContextBatchResponse {
                success: false,
                project: project_name.to_string(),
                results: Vec::new(),
                duration_ms: 0,
                ssh_calls: 0,
                error: Some(format!(
                    "context batch too large for SSH: estimated {} chars exceeds 5x budget of {}",
                    estimate.estimated_chars, req.max_total_chars
                )),
                preflight_rejected: Some(true),
                estimated_chars: Some(estimate.estimated_chars),
                max_allowed_chars: Some(PREFLIGHT_MAX_TOTAL_CHARS),
                max_allowed_items: None,
                project_is_ssh: Some(project_is_ssh),
                suggestion: Some(
                    "Split this SSH request by file or section, or raise max_total_chars."
                        .to_string(),
                ),
                warnings: Vec::new(),
                result_metadata: Vec::new(),
                cache_hits: None,
                recommended_next_action: Some(context_batch_recommended_next_action(
                    false,
                    &[],
                    true,
                    0,
                )),
                action_budget_hint: Some(context_batch_action_budget_hint(0)),
            });
        }
        warnings.push(format!(
            "Estimated output is {} chars; response will be truncated to max_total_chars={}.",
            estimate.estimated_chars, req.max_total_chars
        ));
    }

    // Check 3: SSH project — too many batch items
    if project_is_ssh && req.requests.len() > PREFLIGHT_SSH_MAX_ITEMS {
        return Err(ContextBatchResponse {
            success: false,
            project: project_name.to_string(),
            results: Vec::new(),
            duration_ms: 0,
            ssh_calls: 0,
            error: Some(format!(
                "context batch too large for SSH project: {} items exceeds limit of {}",
                req.requests.len(),
                PREFLIGHT_SSH_MAX_ITEMS
            )),
            preflight_rejected: Some(true),
            estimated_chars: Some(estimate.estimated_chars),
            max_allowed_chars: Some(PREFLIGHT_MAX_TOTAL_CHARS),
            max_allowed_items: Some(PREFLIGHT_SSH_MAX_ITEMS),
            project_is_ssh: Some(project_is_ssh),
            suggestion: Some(format!(
                "Split into smaller batches of at most {} items for SSH projects.",
                PREFLIGHT_SSH_MAX_ITEMS
            )),
            warnings: Vec::new(),
            result_metadata: Vec::new(),
            cache_hits: None,
            recommended_next_action: Some(context_batch_recommended_next_action(
                false,
                &[],
                true,
                0,
            )),
            action_budget_hint: Some(context_batch_action_budget_hint(0)),
        });
    }

    // Check 4: oversized read_file limit (limit > 800) on SSH
    if project_is_ssh && estimate.has_oversized_item {
        return Err(ContextBatchResponse {
            success: false,
            project: project_name.to_string(),
            results: Vec::new(),
            duration_ms: 0,
            ssh_calls: 0,
            error: Some(
                "context batch too large for SSH project: read_file limit exceeds 800 lines"
                    .to_string(),
            ),
            preflight_rejected: Some(true),
            estimated_chars: Some(estimate.estimated_chars),
            max_allowed_chars: Some(PREFLIGHT_MAX_TOTAL_CHARS),
            max_allowed_items: Some(PREFLIGHT_SSH_MAX_ITEMS),
            project_is_ssh: Some(project_is_ssh),
            suggestion: Some(
                "Reduce read_file limit to 800 or below for SSH projects, \
                 or split into multiple smaller reads."
                    .to_string(),
            ),
            warnings: Vec::new(),
            result_metadata: Vec::new(),
            cache_hits: None,
            recommended_next_action: Some(context_batch_recommended_next_action(
                false,
                &[],
                true,
                0,
            )),
            action_budget_hint: Some(context_batch_action_budget_hint(0)),
        });
    }

    Ok(warnings)
}

pub(super) fn markdown_outline_shell_fragment(path: &str, limit: usize) -> String {
    format!(
        " if test -f {0}; then grep -n -E '^#{{1,6}}[[:space:]]+' -- {0} | head -n {1}; else printf '__PDCTX_ERROR__:File not found: {0}\\n'; fi;",
        shell_escape(path),
        limit.clamp(1, MAX_READ_FILE_LIMIT)
    )
}

pub(super) fn markdown_section_shell_fragment(path: &str, query: &str, limit: usize) -> String {
    format!(
        " if test -f {path}; then awk -v q={query} -v max={limit} 'BEGIN{{found=0;level=0;count=0}} /^#{{1,6}}[ \\t]+/{{ if(found){{ match($0,/^#+/); if(RLENGTH<=level) exit }} if(!found && index(tolower($0),tolower(q))>0){{ match($0,/^#+/); level=RLENGTH; found=1 }} }} found && count<max {{ printf \"%4d | %s\\n\", NR, $0; count++ }} END{{ if(!found) printf \"__PDCTX_ERROR__:Section not found: %s\\n\", q }}' -- {path}; else printf '__PDCTX_ERROR__:File not found: {path}\\n'; fi;",
        path = shell_escape(path),
        query = shell_escape(query),
        limit = limit.clamp(1, MAX_READ_FILE_LIMIT)
    )
}

pub(super) fn local_markdown_file_response(
    root: &Path,
    project_name: &str,
    item: &ContextBatchItem,
) -> (ContextResponse, u64) {
    let Some(rel_path) = &item.path else {
        return (
            context_error(
                project_name,
                &item.mode,
                "path parameter is required for markdown mode".to_string(),
            ),
            0,
        );
    };
    let full_path = root.join(rel_path);
    match canonicalize_and_verify(&full_path, root) {
        Ok(canonical) => match std::fs::read_to_string(&canonical) {
            Ok(content) => match item.mode {
                ContextMode::MarkdownOutline => (
                    markdown_outline_from_text(project_name, &content, item.limit),
                    0,
                ),
                ContextMode::ReadSection => {
                    let Some(query) = item.query.as_deref() else {
                        return (
                            context_error(
                                project_name,
                                &item.mode,
                                "query parameter is required for read_section mode".to_string(),
                            ),
                            0,
                        );
                    };
                    (
                        markdown_section_from_text(project_name, &content, query, item.limit),
                        0,
                    )
                }
                _ => unreachable!(),
            },
            Err(e) => (
                context_error(
                    project_name,
                    &item.mode,
                    format!("Failed to read file: {}", e),
                ),
                0,
            ),
        },
        Err(e) => (context_error(project_name, &item.mode, e), 0),
    }
}

fn validate_agent_context_rel_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("path cannot be empty".to_string());
    }
    let path = Path::new(path);
    if path.is_absolute() {
        return Err("absolute paths are not allowed for agent context reads".to_string());
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => return Err("path traversal (..) is not allowed".to_string()),
            _ => return Err("unsupported path component".to_string()),
        }
    }
    Ok(())
}

fn agent_find_prune_expr() -> &'static str {
    "-path './.git' -o -path './target' -o -path './node_modules' -o -path './dist' -o -path './build' -o -path './.cache' -o -path './__pycache__'"
}

fn agent_tree_command(
    path: Option<&str>,
    limit: usize,
    max_depth: usize,
) -> Result<String, String> {
    if let Some(path) = path {
        validate_agent_context_rel_path(path)?;
    }
    let limit = normalize_tree_limit(limit);
    let max_depth = normalize_tree_depth(max_depth);
    let target = path.unwrap_or(".");
    Ok(format!(
        "target={target}; if ! test -e \"$target\"; then printf '__PDCTX_ERROR__:Path not found: %s\\n' \"$target\"; exit 0; fi; find \"$target\" -maxdepth {depth} \\( {prune} \\) -prune -o -print 2>/dev/null | sed 's#^\\./##' | sort | head -n {limit}",
        target = shell_escape(target), depth = max_depth, prune = agent_find_prune_expr(), limit = limit,
    ))
}

fn agent_search_command(path: Option<&str>, query: &str, limit: usize) -> Result<String, String> {
    if let Some(path) = path {
        validate_agent_context_rel_path(path)?;
    }
    let target = path.unwrap_or(".");
    let limit = limit.clamp(1, MAX_SEARCH_RESULTS);
    Ok(format!(
        "target={target}; if ! test -e \"$target\"; then printf '__PDCTX_ERROR__:Path not found: %s\\n' \"$target\"; exit 0; fi; grep -RIn --exclude-dir=.git --exclude-dir=target --exclude-dir=node_modules --exclude-dir=dist --exclude-dir=build --exclude-dir=.cache --exclude-dir=__pycache__ -- {query} \"$target\" 2>/dev/null | sed 's#^\\./##' | head -n {limit}",
        target = shell_escape(target), query = shell_escape(query), limit = limit,
    ))
}

fn agent_read_file_command(path: &str, start_line: usize, limit: usize) -> Result<String, String> {
    validate_agent_context_rel_path(path)?;
    let end_line = validate_read_file_range(start_line, limit)?;
    Ok(format!(
        "file={path}; if ! test -f \"$file\"; then printf '__PDCTX_ERROR__:File not found: %s\\n' \"$file\"; exit 0; fi; awk 'NR>={start} && NR<={end} {{ printf \"%4d | %s\\n\", NR, $0 }}' \"$file\"",
        path = shell_escape(path), start = start_line, end = end_line,
    ))
}

fn agent_overview_command(project_name: &str, allowed_checks: &[String]) -> String {
    let important = "README.md TODO.md Cargo.toml package.json pyproject.toml src/main.rs";
    format!(
        "printf 'Project: {project}\\nRoot: '; pwd; printf 'Branch: '; git rev-parse --abbrev-ref HEAD 2>/dev/null || true; printf '\\nGit Status:\\n'; git status --short --untracked-files=no 2>/dev/null || true; printf '\\nAllowed Checks: {checks}\\n\\nImportant Files:'; for f in {important}; do if test -e \"$f\"; then printf '\\n  %s: yes' \"$f\"; else printf '\\n  %s: no' \"$f\"; fi; done; printf '\\n'",
        project = project_name, checks = allowed_checks.join(", "), important = important,
    )
}

async fn agent_read_file_metadata(
    depot: &Depot,
    proj: &ProjectConfig,
    rel_path: &str,
    request_index: usize,
    unchanged: bool,
) -> Result<ContextBatchResultMetadata, String> {
    validate_agent_context_rel_path(rel_path)?;
    let cmd = format!(
        "file={path}; if ! test -f \"$file\"; then printf '__PDCTX_ERROR__:File not found: %s\\n' \"$file\"; exit 0; fi; sha=$(sha256sum \"$file\" 2>/dev/null | awk '{{print $1}}'); size=$(wc -c < \"$file\" 2>/dev/null | tr -d ' '); lines=$(wc -l < \"$file\" 2>/dev/null | tr -d ' '); printf '%s\\t%s\\t%s\\n' \"$sha\" \"$size\" \"$lines\"",
        path = shell_escape(rel_path),
    );
    let (code, stdout, stderr, _) = super::agent_exec::run_agent_project_command(
        depot,
        proj,
        &cmd,
        10,
        "codex_context_agent_executor",
        "agent context metadata command",
    )
    .await;
    if code != 0 {
        return Err(format!("agent metadata command failed: {}", stderr.trim()));
    }
    if let Some(err) = stdout.trim().strip_prefix("__PDCTX_ERROR__:") {
        return Err(err.trim().to_string());
    }
    let parts = stdout.trim().split('\t').collect::<Vec<_>>();
    if parts.len() != 3 || parts[0].len() != 64 || !parts[0].chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err("agent metadata output was invalid".to_string());
    }
    let file_size = parts[1].parse::<u64>().ok();
    let total_lines = parts[2].parse::<usize>().ok();
    Ok(ContextBatchResultMetadata {
        request_index,
        mode: "read_file".to_string(),
        path: Some(rel_path.to_string()),
        fingerprint: Some(content_sha256_fingerprint("agent-v1", rel_path, parts[0])),
        unchanged,
        file_size,
        modified_unix_ms: None,
        total_lines,
    })
}

async fn agent_read_file_cache_hit_response(
    depot: &Depot,
    proj: &ProjectConfig,
    project_name: &str,
    item: &ContextBatchItem,
    request_index: usize,
) -> Option<(ContextResponse, ContextBatchResultMetadata)> {
    if !matches!(item.mode, ContextMode::ReadFile) {
        return None;
    }
    let rel_path = item.path.as_deref()?;
    let expected = item.if_fingerprint.as_deref()?.trim();
    if expected.is_empty() || validate_read_file_range(item.start_line, item.limit).is_err() {
        return None;
    }
    let mut metadata = agent_read_file_metadata(depot, proj, rel_path, request_index, false)
        .await
        .ok()?;
    if metadata.fingerprint.as_deref() != Some(expected) {
        return None;
    }
    metadata.unchanged = true;
    Some((
        ContextResponse {
            success: true,
            project: project_name.to_string(),
            mode: "read_file".to_string(),
            content: None,
            items: None,
            truncated: false,
            error: None,
        },
        metadata,
    ))
}

async fn execute_agent_context_item(
    depot: &Depot,
    proj: &ProjectConfig,
    project_name: &str,
    item: &ContextBatchItem,
) -> ContextResponse {
    let command_result = match item.mode {
        ContextMode::Overview => Some((
            agent_overview_command(project_name, &proj.effective_allowed_checks()),
            "overview",
            10,
        )),
        ContextMode::Tree => {
            match agent_tree_command(item.path.as_deref(), item.limit, item.max_depth) {
                Ok(cmd) => Some((cmd, "tree", 10)),
                Err(e) => return context_error(project_name, &item.mode, e),
            }
        }
        ContextMode::Search => match &item.query {
            Some(query) => match agent_search_command(item.path.as_deref(), query, item.limit) {
                Ok(cmd) => Some((cmd, "search", 20)),
                Err(e) => return context_error(project_name, &item.mode, e),
            },
            None => {
                return context_error(
                    project_name,
                    &item.mode,
                    "query parameter is required for search mode".to_string(),
                )
            }
        },
        ContextMode::GrepContext => match &item.query {
            Some(query) => match agent_search_command(item.path.as_deref(), query, item.limit) {
                Ok(cmd) => Some((cmd, "grep_context", 20)),
                Err(e) => return context_error(project_name, &item.mode, e),
            },
            None => {
                return context_error(
                    project_name,
                    &item.mode,
                    "query parameter is required for grep_context mode".to_string(),
                )
            }
        },
        ContextMode::ReadFile => match &item.path {
            Some(path) => match agent_read_file_command(path, item.start_line, item.limit) {
                Ok(cmd) => Some((cmd, "read_file", 10)),
                Err(e) => return context_error(project_name, &item.mode, e),
            },
            None => {
                return context_error(
                    project_name,
                    &item.mode,
                    "path parameter is required for read_file mode".to_string(),
                )
            }
        },
        ContextMode::MarkdownOutline => match &item.path {
            Some(path) => {
                if let Err(e) = validate_agent_context_rel_path(path) {
                    return context_error(project_name, &item.mode, e);
                }
                Some((
                    markdown_outline_shell_fragment(path, item.limit),
                    "markdown_outline",
                    10,
                ))
            }
            None => {
                return context_error(
                    project_name,
                    &item.mode,
                    "path parameter is required for markdown_outline mode".to_string(),
                )
            }
        },
        ContextMode::ReadSection => match (&item.path, &item.query) {
            (Some(path), Some(query)) => {
                if let Err(e) = validate_agent_context_rel_path(path) {
                    return context_error(project_name, &item.mode, e);
                }
                Some((
                    markdown_section_shell_fragment(path, query, item.limit),
                    "read_section",
                    10,
                ))
            }
            _ => {
                return context_error(
                    project_name,
                    &item.mode,
                    "path and query parameters are required for read_section mode".to_string(),
                )
            }
        },
        ContextMode::AgentContext => Some((agent_context_shell_fragment(), "agent_context", 10)),
        ContextMode::GitStatus => Some((
            format!("{} 2>/dev/null || true", git_status_command()),
            "git_status",
            10,
        )),
        ContextMode::GitDiff => Some(("git diff 2>/dev/null || true".to_string(), "git_diff", 30)),
        ContextMode::ExperimentOutputs => Some((
            experiment_outputs_shell_fragment(item.limit.max(1).min(500), item.query.as_deref()),
            "experiment_outputs",
            30,
        )),
    };
    let Some((cmd, mode, timeout_secs)) = command_result else {
        return context_error(
            project_name,
            &item.mode,
            "unsupported context mode".to_string(),
        );
    };
    let (code, stdout, stderr, _) = super::agent_exec::run_agent_project_command(
        depot,
        proj,
        &cmd,
        timeout_secs,
        "codex_context_agent_executor",
        "agent context command",
    )
    .await;
    if code != 0 {
        return context_error(
            project_name,
            &item.mode,
            format!("agent context command failed: {}", stderr.trim()),
        );
    }
    if let Some(err) = stdout.trim().strip_prefix("__PDCTX_ERROR__:") {
        return context_error(project_name, &item.mode, err.trim().to_string());
    }
    match item.mode {
        ContextMode::Tree => {
            let items = stdout
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| line.to_string())
                .collect::<Vec<_>>();
            ContextResponse {
                success: true,
                project: project_name.to_string(),
                mode: mode.to_string(),
                content: None,
                items: Some(items.clone()),
                truncated: items.len() >= normalize_tree_limit(item.limit),
                error: None,
            }
        }
        ContextMode::Search => {
            let items = stdout
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| line.to_string())
                .collect::<Vec<_>>();
            ContextResponse {
                success: true,
                project: project_name.to_string(),
                mode: mode.to_string(),
                content: None,
                items: Some(items.clone()),
                truncated: items.len() >= item.limit.clamp(1, MAX_SEARCH_RESULTS),
                error: None,
            }
        }
        _ => mode_content_response(project_name, mode, stdout, MAX_OUTPUT_LEN),
    }
}

pub(super) fn execute_context_item(
    proj: &ProjectConfig,
    project_name: &str,
    item: &ContextBatchItem,
) -> (ContextResponse, u64) {
    let _start = Instant::now();

    let root = proj.root();
    if !root.exists() {
        return (
            context_error(
                project_name,
                &item.mode,
                format!("Project root does not exist: {:?}", root),
            ),
            0,
        );
    }
    match item.mode {
        ContextMode::Overview => {
            let branch = run_command("git rev-parse --abbrev-ref HEAD", &root, 10)
                .1
                .trim()
                .to_string();
            let status = run_command(git_status_command(), &root, 10)
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
            let mut content = format!("Project: {}\nRoot: {}\nBranch: {}\n\nGit Status:\n{}\n\nAllowed Checks: {}\n\nImportant Files:", project_name, root.display(), branch, status, proj.effective_allowed_checks().join(", "));
            for f in &important_files {
                let exists = root.join(f).exists();
                content.push_str(&format!("\n  {}: {}", f, if exists { "yes" } else { "no" }));
            }
            (
                ContextResponse {
                    success: true,
                    project: project_name.to_string(),
                    mode: "overview".to_string(),
                    content: Some(content),
                    items: None,
                    truncated: false,
                    error: None,
                },
                0,
            )
        }
        ContextMode::Tree => {
            let limit = normalize_tree_limit(item.limit);
            let max_depth = normalize_tree_depth(item.max_depth);
            let tree_root = match &item.path {
                Some(rel_path) => match canonicalize_and_verify(&root.join(rel_path), &root) {
                    Ok(path) => path,
                    Err(e) => return (context_error(project_name, &item.mode, e), 0),
                },
                None => root.clone(),
            };
            let mut items = Vec::new();
            collect_tree(&tree_root, &root, &mut items, limit, max_depth);
            let truncated = items.len() >= limit;
            (
                ContextResponse {
                    success: true,
                    project: project_name.to_string(),
                    mode: "tree".to_string(),
                    content: None,
                    items: Some(items),
                    truncated,
                    error: None,
                },
                0,
            )
        }
        ContextMode::Search => match &item.query {
            Some(query) => {
                let results = simple_search(&root, query, MAX_SEARCH_RESULTS);
                let truncated = results.len() >= MAX_SEARCH_RESULTS;
                (
                    ContextResponse {
                        success: true,
                        project: project_name.to_string(),
                        mode: "search".to_string(),
                        content: None,
                        items: Some(results),
                        truncated,
                        error: None,
                    },
                    0,
                )
            }
            None => (
                context_error(
                    project_name,
                    &item.mode,
                    "query parameter is required for search mode".to_string(),
                ),
                0,
            ),
        },
        ContextMode::GrepContext => match &item.query {
            Some(query) => (
                local_grep_context(&root, project_name, item.path.as_deref(), query, item.limit),
                0,
            ),
            None => (
                context_error(
                    project_name,
                    &item.mode,
                    "query parameter is required for grep_context mode".to_string(),
                ),
                0,
            ),
        },
        ContextMode::ReadFile => match &item.path {
            Some(rel_path) => {
                let full_path = root.join(rel_path);
                match canonicalize_and_verify(&full_path, &root) {
                    Ok(canonical) => match std::fs::read_to_string(&canonical) {
                        Ok(content) => {
                            let lines: Vec<&str> = content.lines().collect();
                            let total = lines.len();
                            let end_line =
                                match validate_read_file_range(item.start_line, item.limit) {
                                    Ok(end_line) => end_line,
                                    Err(e) => {
                                        return (context_error(project_name, &item.mode, e), 0)
                                    }
                                };
                            let start = item.start_line - 1;
                            let end = end_line.min(total);
                            let selected: Vec<String> = if start < total {
                                lines[start..end]
                                    .iter()
                                    .enumerate()
                                    .map(|(i, l)| format_context_line(start + i + 1, l).0)
                                    .collect()
                            } else {
                                Vec::new()
                            };
                            let (output, truncated) =
                                truncate_string(selected.join("\n"), MAX_OUTPUT_LEN);
                            (
                                ContextResponse {
                                    success: true,
                                    project: project_name.to_string(),
                                    mode: "read_file".to_string(),
                                    content: Some(output),
                                    items: None,
                                    truncated,
                                    error: None,
                                },
                                0,
                            )
                        }
                        Err(e) => (
                            context_error(
                                project_name,
                                &item.mode,
                                format!("Failed to read file: {}", e),
                            ),
                            0,
                        ),
                    },
                    Err(e) => (context_error(project_name, &item.mode, e), 0),
                }
            }
            None => (
                context_error(
                    project_name,
                    &item.mode,
                    "path parameter is required for read_file mode".to_string(),
                ),
                0,
            ),
        },
        ContextMode::MarkdownOutline | ContextMode::ReadSection => {
            local_markdown_file_response(&root, project_name, item)
        }
        ContextMode::AgentContext => (local_agent_context(&root, project_name), 0),
        ContextMode::GitStatus => {
            let output = run_command(git_status_command(), &root, 10);
            let (content, truncated) = truncate_string(output.1, MAX_OUTPUT_LEN);
            (
                ContextResponse {
                    success: true,
                    project: project_name.to_string(),
                    mode: "git_status".to_string(),
                    content: Some(content),
                    items: None,
                    truncated,
                    error: None,
                },
                0,
            )
        }
        ContextMode::GitDiff => {
            let output = run_command("git diff", &root, 30);
            let (content, truncated) = truncate_string(output.1, MAX_OUTPUT_LEN);
            (
                ContextResponse {
                    success: true,
                    project: project_name.to_string(),
                    mode: "git_diff".to_string(),
                    content: Some(content),
                    items: None,
                    truncated,
                    error: None,
                },
                0,
            )
        }
        ContextMode::ExperimentOutputs => {
            let (out, truncated) =
                local_experiment_outputs(&root, item.limit.max(1).min(500), item.query.as_deref());
            (
                ContextResponse {
                    success: true,
                    project: project_name.to_string(),
                    mode: "experiment_outputs".to_string(),
                    content: Some(out),
                    items: None,
                    truncated,
                    error: None,
                },
                0,
            )
        }
    }
}

/// Shell fragment for experiment_outputs mode (SSH executor).
pub(super) fn experiment_outputs_shell_fragment(
    max_files: usize,
    since_minutes: Option<&str>,
) -> String {
    let max_files = max_files.min(500).max(1);
    let since_filter = if let Some(mins) = since_minutes {
        if let Ok(n) = mins.parse::<u64>() {
            format!(" -mmin -{}", n)
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    let output_exts = "\\( -name '*.csv' -o -name '*.json' -o -name '*.md' -o -name '*.png' -o -name '*.jpg' -o -name '*.log' -o -name '*.txt' -o -name '*.html' \\)";
    let checkpoint_exts = "\\( -name '*.pt' -o -name '*.pth' -o -name '*.ckpt' -o -name '*.joblib' -o -name '*.npz' -o -name '*.parquet' -o -name '*.pkl' -o -name '*.h5' -o -name '*.hdf5' \\)";
    format!(
        concat!(
            "printf '=== git_status ===\\n'; git status --short 2>/dev/null | head -50 || true;",
            " printf '\\n=== output_files ===\\n'; find . -not -path './.git/*' -not -path './target/*' -not -path './node_modules/*' -not -path './__pycache__/*' {output}{since} -type f -print 2>/dev/null | sed 's|^\\./||' | sort | head -n {max} || true;",
            " printf '\\n=== checkpoint_files ===\\n'; find . -not -path './.git/*' -not -path './target/*' -not -path './node_modules/*' -not -path './__pycache__/*' {ckpt}{since} -type f -print 2>/dev/null | sed 's|^\\./||' | sort | head -n {max} || true;",
            " printf '\\n=== large_files (>20MB) ===\\n'; find . -not -path './.git/*' -not -path './target/*' -not -path './node_modules/*' -size +20M -type f -print 2>/dev/null | sed 's|^\\./||' | head -n {max} | while IFS= read -r f; do size=$(du -sh \"$f\" 2>/dev/null | cut -f1); ignored=$(git check-ignore -q \"$f\" 2>/dev/null && echo yes || echo no); printf '%s\\tsize=%s\\tgitignored=%s\\n' \"$f\" \"$size\" \"$ignored\"; done || true;",
            " printf '\\n=== untracked_new ===\\n'; git status --short --untracked-files=all 2>/dev/null | grep '^?' | sed 's/^?? //' | head -30 || true;"
        ),
        output = output_exts,
        since = since_filter,
        max = max_files,
        ckpt = checkpoint_exts,
    )
}

/// Local implementation of experiment_outputs mode. Returns (content, truncated).
pub(super) fn local_experiment_outputs(
    root: &Path,
    max_files: usize,
    since_minutes: Option<&str>,
) -> (String, bool) {
    let max_files = max_files.min(500).max(1);
    let mut out = String::new();

    // git status
    let git_status = run_command("git status --short", root, 10).1;
    out.push_str("=== git_status ===\n");
    for line in git_status.lines().take(50) {
        out.push_str(line);
        out.push('\n');
    }

    let output_exts = [
        "csv", "json", "md", "png", "jpg", "jpeg", "log", "txt", "html",
    ];
    let checkpoint_exts = [
        "pt", "pth", "ckpt", "joblib", "npz", "parquet", "pkl", "h5", "hdf5",
    ];
    let since_secs: Option<u64> = since_minutes
        .and_then(|s| s.parse::<u64>().ok())
        .map(|m| m * 60);
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut output_files: Vec<String> = Vec::new();
    let mut checkpoint_files: Vec<String> = Vec::new();
    let mut large_files: Vec<(String, u64)> = Vec::new();

    fn walk(
        dir: &Path,
        base: &Path,
        output_exts: &[&str],
        checkpoint_exts: &[&str],
        since_secs: Option<u64>,
        now_secs: u64,
        output_files: &mut Vec<String>,
        checkpoint_files: &mut Vec<String>,
        large_files: &mut Vec<(String, u64)>,
        max_files: usize,
        depth: usize,
    ) {
        if depth > 8 {
            return;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        sorted.sort_by_key(|e| e.file_name());
        for entry in sorted {
            let name = entry.file_name().to_string_lossy().to_string();
            if matches!(
                name.as_str(),
                ".git" | "target" | "node_modules" | "__pycache__" | ".cache" | "dist" | "build"
            ) {
                continue;
            }
            let path = entry.path();
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            if path.is_dir() {
                walk(
                    &path,
                    base,
                    output_exts,
                    checkpoint_exts,
                    since_secs,
                    now_secs,
                    output_files,
                    checkpoint_files,
                    large_files,
                    max_files,
                    depth + 1,
                );
            } else if path.is_file() {
                let meta = match std::fs::metadata(&path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let size = meta.len();
                if let Some(since) = since_secs {
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    if now_secs.saturating_sub(mtime) > since {
                        if size > 20 * 1024 * 1024 && large_files.len() < max_files {
                            large_files.push((rel, size));
                        }
                        continue;
                    }
                }
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if output_exts.contains(&ext.as_str()) && output_files.len() < max_files {
                    output_files.push(rel.clone());
                }
                if checkpoint_exts.contains(&ext.as_str()) && checkpoint_files.len() < max_files {
                    checkpoint_files.push(rel.clone());
                }
                if size > 20 * 1024 * 1024 && large_files.len() < max_files {
                    large_files.push((rel, size));
                }
            }
        }
    }

    walk(
        root,
        root,
        &output_exts,
        &checkpoint_exts,
        since_secs,
        now_secs,
        &mut output_files,
        &mut checkpoint_files,
        &mut large_files,
        max_files,
        0,
    );

    out.push_str("\n=== output_files ===\n");
    for f in &output_files {
        out.push_str(f);
        out.push('\n');
    }
    out.push_str("\n=== checkpoint_files ===\n");
    for f in &checkpoint_files {
        out.push_str(f);
        out.push('\n');
    }
    out.push_str("\n=== large_files (>20MB) ===\n");
    for (f, size) in &large_files {
        let size_mb = *size as f64 / (1024.0 * 1024.0);
        let gitignored = std::process::Command::new("git")
            .args(["check-ignore", "-q", f])
            .current_dir(root)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        out.push_str(&format!(
            "{}\tsize={:.1}MB\tgitignored={}\n",
            f,
            size_mb,
            if gitignored { "yes" } else { "no" }
        ));
    }
    out.push_str("\n=== untracked_new ===\n");
    let untracked = run_command("git status --short --untracked-files=all", root, 10).1;
    for line in untracked.lines().filter(|l| l.starts_with("??")).take(30) {
        out.push_str(line.trim_start_matches("?? "));
        out.push('\n');
    }

    truncate_string(out, CONTEXT_MAX_OUTPUT_LEN)
}

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
    let _request_start = Instant::now();
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
            let status = run_command(git_status_command(), &root, 10)
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
                body.project, root.display(), branch, status, proj.effective_allowed_checks().join(", "));
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
            let limit = normalize_tree_limit(body.limit);
            let max_depth = normalize_tree_depth(body.max_depth);
            let tree_root = match &body.path {
                Some(rel_path) => match canonicalize_and_verify(&root.join(rel_path), &root) {
                    Ok(path) => path,
                    Err(e) => {
                        res.status_code(StatusCode::BAD_REQUEST);
                        res.render(Json(ContextResponse {
                            success: false,
                            project: body.project,
                            mode: "tree".to_string(),
                            content: None,
                            items: None,
                            truncated: false,
                            error: Some(e),
                        }));
                        return;
                    }
                },
                None => root.clone(),
            };
            let mut items = Vec::new();
            collect_tree(&tree_root, &root, &mut items, limit, max_depth);
            let truncated = items.len() >= limit;
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
        ContextMode::GrepContext => {
            let Some(query) = &body.query else {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(Json(ContextResponse {
                    success: false,
                    project: body.project,
                    mode: "grep_context".to_string(),
                    content: None,
                    items: None,
                    truncated: false,
                    error: Some("query parameter is required for grep_context mode".to_string()),
                }));
                return;
            };
            res.render(Json(local_grep_context(
                &root,
                &body.project,
                body.path.as_deref(),
                query,
                body.limit,
            )));
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
                        let end_line = match validate_read_file_range(body.start_line, body.limit) {
                            Ok(end_line) => end_line,
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
                                return;
                            }
                        };
                        let start = body.start_line - 1;
                        let end = end_line.min(total);
                        let selected: Vec<String> = if start < total {
                            lines[start..end]
                                .iter()
                                .enumerate()
                                .map(|(i, l)| format_context_line(start + i + 1, l).0)
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
        ContextMode::MarkdownOutline | ContextMode::ReadSection => {
            let item = ContextBatchItem {
                mode: body.mode,
                path: body.path.clone(),
                query: body.query.clone(),
                if_fingerprint: None,
                start_line: body.start_line,
                limit: body.limit,
                max_depth: body.max_depth,
            };
            let (resp, _) = local_markdown_file_response(&root, &body.project, &item);
            res.render(Json(resp));
        }
        ContextMode::AgentContext => {
            res.render(Json(local_agent_context(&root, &body.project)));
        }
        ContextMode::GitStatus => {
            let output = run_command(git_status_command(), &root, 10);
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
        ContextMode::ExperimentOutputs => {
            let (out, truncated) =
                local_experiment_outputs(&root, body.limit.max(1).min(500), body.query.as_deref());
            res.render(Json(ContextResponse {
                success: true,
                project: body.project,
                mode: "experiment_outputs".to_string(),
                content: Some(out),
                items: None,
                truncated,
                error: None,
            }));
        }
    }
}

#[handler]
pub async fn codex_context_batch(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit_started_at = chrono::Utc::now().timestamp();
    let audit_clock = Instant::now();
    let audit_db = get_db(depot);
    let explicit_session_id = request_action_session_id(req);
    let Some(projects) = get_projects(depot) else {
        res.render(Json(ContextBatchResponse {
            success: false,
            project: String::new(),
            results: Vec::new(),
            duration_ms: 0,
            ssh_calls: 0,
            error: Some(
                "Projects not configured. Set PROJECTS_CONFIG or create projects.toml".to_string(),
            ),
            preflight_rejected: None,
            estimated_chars: None,
            max_allowed_chars: None,
            max_allowed_items: None,
            project_is_ssh: None,
            suggestion: None,
            warnings: Vec::new(),
            result_metadata: Vec::new(),
            cache_hits: None,
            recommended_next_action: Some(
                "Create projects.toml or set PROJECTS_CONFIG, restart, then retry context_batch."
                    .to_string(),
            ),
            action_budget_hint: Some(context_batch_action_budget_hint(0)),
        }));
        return;
    };
    let body: ContextBatchRequest = match req.parse_json().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ContextBatchResponse {
                success: false,
                project: String::new(),
                results: Vec::new(),
                duration_ms: 0,
                ssh_calls: 0,
                error: Some(format!("Invalid JSON: {}", e)),
                preflight_rejected: None,
                estimated_chars: None,
                max_allowed_chars: None,
                max_allowed_items: None,
                project_is_ssh: None,
                suggestion: None,
                warnings: Vec::new(),
                result_metadata: Vec::new(),
                cache_hits: None,
                recommended_next_action: Some(
                    "Fix the JSON body, then retry the same context_batch intent.".to_string(),
                ),
                action_budget_hint: Some(context_batch_action_budget_hint(0)),
            }));
            return;
        }
    };
    if body.requests.is_empty() || body.requests.len() > 30 {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ContextBatchResponse {
            success: false,
            project: body.project,
            results: Vec::new(),
            duration_ms: 0,
            ssh_calls: 0,
            error: Some("requests must contain 1-30 items".to_string()),
            preflight_rejected: None,
            estimated_chars: None,
            max_allowed_chars: None,
            max_allowed_items: None,
            project_is_ssh: None,
            suggestion: None,
            warnings: Vec::new(),
            result_metadata: Vec::new(),
            cache_hits: None,
            recommended_next_action: Some(
                "Use 1-30 batch items and group related reads into one context_batch.".to_string(),
            ),
            action_budget_hint: Some(context_batch_action_budget_hint(0)),
        }));
        return;
    }
    let proj = match projects.get_project(&body.project) {
        Ok(p) => p,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ContextBatchResponse {
                success: false,
                project: body.project,
                results: Vec::new(),
                duration_ms: 0,
                ssh_calls: 0,
                error: Some(e),
                preflight_rejected: None,
                estimated_chars: None,
                max_allowed_chars: None,
                max_allowed_items: None,
                project_is_ssh: None,
                suggestion: None,
                warnings: Vec::new(),
                result_metadata: Vec::new(),
                cache_hits: None,
                recommended_next_action: Some(
                    "Call getCodexProjects to choose a valid project, then retry context_batch."
                        .to_string(),
                ),
                action_budget_hint: Some(context_batch_action_budget_hint(0)),
            }));
            return;
        }
    };


    // Preflight check: reject obviously oversized requests before execution
    let project_is_ssh = proj.is_ssh();
    let project_is_agent = proj.is_agent();
    match preflight_context_batch(&body, project_is_ssh, &body.project) {
        Ok(preflight_warnings) => {
            // Request is allowed; preflight_warnings may contain non-blocking hints
            let start = Instant::now();
            let mut result_metadata = Vec::new();
            let mut cache_hits = 0usize;
            let (mut results, ssh_calls) = if project_is_agent {
                let mut results = Vec::with_capacity(body.requests.len());
                for (idx, item) in body.requests.iter().enumerate() {
                    if let Some((resp, metadata)) =
                        agent_read_file_cache_hit_response(depot, proj, &body.project, item, idx)
                            .await
                    {
                        cache_hits += 1;
                        result_metadata.push(metadata);
                        results.push(resp);
                        continue;
                    }
                    let resp = execute_agent_context_item(depot, proj, &body.project, item).await;
                    if resp.success && matches!(item.mode, ContextMode::ReadFile) {
                        if let Some(rel_path) = item.path.as_deref() {
                            if let Ok(metadata) =
                                agent_read_file_metadata(depot, proj, rel_path, idx, false).await
                            {
                                result_metadata.push(metadata);
                            }
                        }
                    }
                    results.push(resp);
                }
                (results, body.requests.len() as u64)
            } else {
                let mut results = Vec::with_capacity(body.requests.len());
                for (idx, item) in body.requests.iter().enumerate() {
                    if let Some((resp, metadata)) =
                        local_read_file_cache_hit_response(&proj.root(), &body.project, item, idx)
                    {
                        cache_hits += 1;
                        result_metadata.push(metadata);
                        results.push(resp);
                        continue;
                    }
                    let (resp, _) = execute_context_item(proj, &body.project, item);
                    if matches!(item.mode, ContextMode::ReadFile) {
                        if let Some(rel_path) = item.path.as_deref() {
                            if let Ok(metadata) =
                                local_read_file_metadata(&proj.root(), rel_path, idx, false)
                            {
                                result_metadata.push(metadata);
                            }
                        }
                    }
                    results.push(resp);
                }
                (results, 0)
            };
            enforce_context_batch_total_limit(&mut results, body.max_total_chars);
            let success = results.iter().all(|r| r.success);
            let duration_ms = start.elapsed().as_millis() as u64;
            let recommended_next_action =
                context_batch_recommended_next_action(success, &results, false, cache_hits);
            tracing::info!(
                target: "codex.metrics",
                operation = "getProjectContextBatch",
                project = %body.project,
                executor = if project_is_agent { "agent" } else if project_is_ssh { "ssh" } else { "local" },
                success = success,
                request_count = results.len(),
                duration_ms = duration_ms,
                ssh_calls = ssh_calls,
                control_master = projects.ssh.as_ref().map(|s| s.control_master).unwrap_or(false),
                "codex_context_batch_completed"
            );
            let response = ContextBatchResponse {
                success,
                project: body.project,
                results,
                duration_ms,
                ssh_calls,
                error: None,
                preflight_rejected: None,
                estimated_chars: None,
                max_allowed_chars: None,
                max_allowed_items: None,
                project_is_ssh: None,
                suggestion: None,
                warnings: preflight_warnings,
                result_metadata,
                cache_hits: (cache_hits > 0).then_some(cache_hits),
                recommended_next_action: Some(recommended_next_action),
                action_budget_hint: Some(context_batch_action_budget_hint(cache_hits)),
            };
            if let Some(db) = audit_db.as_ref() {
                let ended_at = chrono::Utc::now().timestamp();
                let modes = body
                    .requests
                    .iter()
                    .map(|item| format!("{:?}", item.mode).to_ascii_lowercase())
                    .collect::<Vec<_>>();
                record_action_event(
                    db,
                    ActionAuditEventInput {
                        explicit_session_id: explicit_session_id.clone(),
                        session_title: None,
                        endpoint: "/api/codex/context_batch".to_string(),
                        action_name: "getProjectContextBatch".to_string(),
                        operation: Some(modes.join(",")),
                        project: Some(response.project.clone()),
                        status: if response.success {
                            "success".to_string()
                        } else {
                            "failed".to_string()
                        },
                        http_status: Some(200),
                        started_at: audit_started_at,
                        ended_at,
                        duration_ms: duration_ms as i64,
                        error_summary: response.error.clone(),
                        warning_summary: if response.warnings.is_empty() {
                            None
                        } else {
                            Some(response.warnings.join(" | "))
                        },
                        changed_files: Vec::new(),
                        ids: json!({}),
                        summary: json!({
                            "modes": modes,
                            "count": body.requests.len(),
                            "max_total_chars": body.max_total_chars,
                            "estimated_chars": response.estimated_chars,
                            "preflight_rejected": false,
                            "ssh_calls": response.ssh_calls,
                            "project_is_ssh": project_is_ssh,
                            "project_is_agent": project_is_agent,
                            "cache_hits": response.cache_hits.unwrap_or(0),
                            "metadata_count": response.result_metadata.len(),
                        }),
                        request_bytes: None,
                        response_bytes: None,
                    },
                );
            }
            res.render(Json(response));
        }
        Err(rejection_response) => {
            // Preflight rejected the request — return structured error
            tracing::warn!(
                target: "codex.metrics",
                operation = "getProjectContextBatch",
                project = %body.project,
                executor = if project_is_agent { "agent" } else if project_is_ssh { "ssh" } else { "local" },
                estimated_chars = ?rejection_response.estimated_chars,
                "preflight_rejected"
            );
            if let Some(db) = audit_db.as_ref() {
                let ended_at = chrono::Utc::now().timestamp();
                let modes = body
                    .requests
                    .iter()
                    .map(|item| format!("{:?}", item.mode).to_ascii_lowercase())
                    .collect::<Vec<_>>();
                record_action_event(
                    db,
                    ActionAuditEventInput {
                        explicit_session_id,
                        session_title: None,
                        endpoint: "/api/codex/context_batch".to_string(),
                        action_name: "getProjectContextBatch".to_string(),
                        operation: Some(modes.join(",")),
                        project: Some(rejection_response.project.clone()),
                        status: "rejected".to_string(),
                        http_status: Some(200),
                        started_at: audit_started_at,
                        ended_at,
                        duration_ms: audit_clock.elapsed().as_millis() as i64,
                        error_summary: rejection_response.error.clone(),
                        warning_summary: if rejection_response.warnings.is_empty() {
                            None
                        } else {
                            Some(rejection_response.warnings.join(" | "))
                        },
                        changed_files: Vec::new(),
                        ids: json!({}),
                        summary: json!({
                            "modes": modes,
                            "count": body.requests.len(),
                            "max_total_chars": body.max_total_chars,
                            "estimated_chars": rejection_response.estimated_chars,
                            "preflight_rejected": true,
                            "project_is_ssh": project_is_ssh,
                            "project_is_agent": project_is_agent,
                        }),
                        request_bytes: None,
                        response_bytes: None,
                    },
                );
            }
            res.render(Json(rejection_response));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_read_file_cache_hit_omits_unchanged_content() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("a.txt");
        std::fs::write(&file, "hello\nworld\n").unwrap();

        let metadata = local_read_file_metadata(tmp.path(), "a.txt", 0, false).unwrap();
        let item = ContextBatchItem {
            mode: ContextMode::ReadFile,
            path: Some("a.txt".to_string()),
            query: None,
            if_fingerprint: metadata.fingerprint.clone(),
            start_line: 1,
            limit: 10,
            max_depth: 4,
        };

        let (response, hit_metadata) =
            local_read_file_cache_hit_response(tmp.path(), "proj", &item, 0).unwrap();
        assert!(response.success);
        assert_eq!(response.mode, "read_file");
        assert!(response.content.is_none());
        assert!(hit_metadata.unchanged);
        assert_eq!(hit_metadata.fingerprint, metadata.fingerprint);

        std::fs::write(&file, "hello\nworld\nchanged\n").unwrap();
        assert!(local_read_file_cache_hit_response(tmp.path(), "proj", &item, 0).is_none());
    }
}
