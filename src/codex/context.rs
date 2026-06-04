use super::get_projects;
use super::shell::{run_command, shell_escape};
use super::types::{
    ContextBatchItem, ContextBatchRequest, ContextBatchResponse, ContextMode, ContextRequest,
    ContextResponse,
};
use super::{
    agent_context_shell_fragment, run_project_cmd, ssh_grep_context, ssh_overview, ssh_read_file,
    ssh_search, ssh_tree, truncate_string, try_ssh_context_batch_once, MAX_OUTPUT_LEN,
};
use crate::projects::{canonicalize_and_verify, ProjectConfig, SshConfig};
use salvo::prelude::*;
use std::path::Path;
use std::time::Instant;

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
const PREFLIGHT_MAX_TOTAL_CHARS: usize = 120_000;
/// Recommended max_total_chars for compact/GPT usage.
const PREFLIGHT_RECOMMENDED_MAX_CHARS: usize = 80_000;
/// Max batch items for local projects before warning.
const PREFLIGHT_LOCAL_MAX_ITEMS: usize = 12;
/// Hard max batch items for SSH projects — requests above this are rejected.
const PREFLIGHT_SSH_MAX_ITEMS: usize = 8;
/// Max read_file limit before preflight rejection.
const PREFLIGHT_MAX_READ_FILE_LIMIT: usize = 400;

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
    /// Whether any item has a very large limit (e.g., read_file limit > 400).
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
        });
    }

    // Check 2: estimated output exceeds max_total_chars significantly (2x margin)
    // This catches requests where the sum of per-item estimates far exceeds what the client
    // asked for — even if they set max_total_chars under the server limit.
    if estimate.estimated_chars > req.max_total_chars * 3 {
        return Err(ContextBatchResponse {
            success: false,
            project: project_name.to_string(),
            results: Vec::new(),
            duration_ms: 0,
            ssh_calls: 0,
            error: Some(format!(
                "context batch too large: estimated {} chars exceeds 3x budget of {}",
                estimate.estimated_chars, req.max_total_chars
            )),
            preflight_rejected: Some(true),
            estimated_chars: Some(estimate.estimated_chars),
            max_allowed_chars: Some(PREFLIGHT_MAX_TOTAL_CHARS),
            max_allowed_items: None,
            project_is_ssh: Some(project_is_ssh),
            suggestion: Some(
                "Split this request into smaller batches by file or section. \
                 Reduce limit per item or request fewer items per batch."
                    .to_string(),
            ),
            warnings: Vec::new(),
        });
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
        });
    }

    // Check 4: oversized read_file limit (limit > 400) on SSH
    if project_is_ssh && estimate.has_oversized_item {
        return Err(ContextBatchResponse {
            success: false,
            project: project_name.to_string(),
            results: Vec::new(),
            duration_ms: 0,
            ssh_calls: 0,
            error: Some(
                "context batch too large for SSH project: read_file limit exceeds 400 lines"
                    .to_string(),
            ),
            preflight_rejected: Some(true),
            estimated_chars: Some(estimate.estimated_chars),
            max_allowed_chars: Some(PREFLIGHT_MAX_TOTAL_CHARS),
            max_allowed_items: Some(PREFLIGHT_SSH_MAX_ITEMS),
            project_is_ssh: Some(project_is_ssh),
            suggestion: Some(
                "Reduce read_file limit to 400 or below for SSH projects, \
                 or split into multiple smaller reads."
                    .to_string(),
            ),
            warnings: Vec::new(),
        });
    }

    Ok(estimate.warnings)
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

pub(super) fn execute_context_item(
    proj: &ProjectConfig,
    project_name: &str,
    item: &ContextBatchItem,
    ssh_config: Option<&SshConfig>,
) -> (ContextResponse, u64) {
    if proj.is_ssh() {
        let resp = match item.mode {
            ContextMode::Overview => ssh_overview(proj, project_name, ssh_config),
            ContextMode::Tree => ssh_tree(
                proj,
                project_name,
                item.path.as_deref(),
                item.limit,
                item.max_depth,
                ssh_config,
            ),
            ContextMode::Search => match &item.query {
                Some(query) => ssh_search(proj, project_name, query, ssh_config),
                None => context_error(
                    project_name,
                    &item.mode,
                    "query parameter is required for search mode".to_string(),
                ),
            },
            ContextMode::GrepContext => match &item.query {
                Some(query) => ssh_grep_context(
                    proj,
                    project_name,
                    item.path.as_deref(),
                    query,
                    item.limit,
                    ssh_config,
                ),
                None => context_error(
                    project_name,
                    &item.mode,
                    "query parameter is required for grep_context mode".to_string(),
                ),
            },
            ContextMode::ReadFile => match &item.path {
                Some(path) => ssh_read_file(
                    proj,
                    project_name,
                    path,
                    item.start_line,
                    item.limit,
                    ssh_config,
                ),
                None => context_error(
                    project_name,
                    &item.mode,
                    "path parameter is required for read_file mode".to_string(),
                ),
            },
            ContextMode::MarkdownOutline => match &item.path {
                Some(path) => {
                    let cmd = markdown_outline_shell_fragment(path, item.limit);
                    let (code, stdout, stderr, _) = run_project_cmd(proj, &cmd, 10, ssh_config);
                    if code == 0 {
                        if let Some(err) = stdout.trim().strip_prefix("__PDCTX_ERROR__:") {
                            context_error(project_name, &item.mode, err.trim().to_string())
                        } else {
                            mode_content_response(
                                project_name,
                                "markdown_outline",
                                stdout,
                                MAX_OUTPUT_LEN,
                            )
                        }
                    } else {
                        context_error(
                            project_name,
                            &item.mode,
                            format!("markdown_outline failed: {}", stderr.trim()),
                        )
                    }
                }
                None => context_error(
                    project_name,
                    &item.mode,
                    "path parameter is required for markdown_outline mode".to_string(),
                ),
            },
            ContextMode::ReadSection => match (&item.path, &item.query) {
                (Some(path), Some(query)) => {
                    let cmd = markdown_section_shell_fragment(path, query, item.limit);
                    let (code, stdout, stderr, _) = run_project_cmd(proj, &cmd, 10, ssh_config);
                    if code == 0 {
                        if let Some(err) = stdout.trim().strip_prefix("__PDCTX_ERROR__:") {
                            context_error(project_name, &item.mode, err.trim().to_string())
                        } else {
                            mode_content_response(
                                project_name,
                                "read_section",
                                stdout,
                                MAX_OUTPUT_LEN,
                            )
                        }
                    } else {
                        context_error(
                            project_name,
                            &item.mode,
                            format!("read_section failed: {}", stderr.trim()),
                        )
                    }
                }
                _ => context_error(
                    project_name,
                    &item.mode,
                    "path and query parameters are required for read_section mode".to_string(),
                ),
            },
            ContextMode::AgentContext => {
                let (code, stdout, stderr, _) =
                    run_project_cmd(proj, &agent_context_shell_fragment(), 10, ssh_config);
                if code == 0 {
                    mode_content_response(project_name, "agent_context", stdout, MAX_OUTPUT_LEN)
                } else {
                    context_error(
                        project_name,
                        &item.mode,
                        format!("agent_context failed: {}", stderr.trim()),
                    )
                }
            }
            ContextMode::GitStatus => {
                let (code, stdout, stderr, _) =
                    run_project_cmd(proj, git_status_command(), 10, ssh_config);
                if code == 0 {
                    let (content, truncated) = truncate_string(stdout, MAX_OUTPUT_LEN);
                    ContextResponse {
                        success: true,
                        project: project_name.to_string(),
                        mode: "git_status".to_string(),
                        content: Some(content),
                        items: None,
                        truncated,
                        error: None,
                    }
                } else {
                    context_error(
                        project_name,
                        &item.mode,
                        format!("git status failed: {}", stderr.trim()),
                    )
                }
            }
            ContextMode::GitDiff => {
                let (code, stdout, stderr, _) = run_project_cmd(proj, "git diff", 30, ssh_config);
                if code == 0 {
                    let (content, truncated) = truncate_string(stdout, MAX_OUTPUT_LEN);
                    ContextResponse {
                        success: true,
                        project: project_name.to_string(),
                        mode: "git_diff".to_string(),
                        content: Some(content),
                        items: None,
                        truncated,
                        error: None,
                    }
                } else {
                    context_error(
                        project_name,
                        &item.mode,
                        format!("git diff failed: {}", stderr.trim()),
                    )
                }
            }
            ContextMode::ExperimentOutputs => {
                // SSH: run the experiment_outputs shell script remotely
                let cmd = experiment_outputs_shell_fragment(
                    item.limit.max(1).min(500),
                    item.query.as_deref(),
                );
                let (code, stdout, stderr, _) = run_project_cmd(proj, &cmd, 30, ssh_config);
                if code == 0 {
                    let (out, truncated) = truncate_string(stdout, MAX_OUTPUT_LEN);
                    ContextResponse {
                        success: true,
                        project: project_name.to_string(),
                        mode: "experiment_outputs".to_string(),
                        content: Some(out),
                        items: None,
                        truncated,
                        error: None,
                    }
                } else {
                    context_error(
                        project_name,
                        &item.mode,
                        format!("experiment_outputs failed: {}", stderr.trim()),
                    )
                }
            }
        };
        return (resp, 1);
    }

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
            let mut content = format!("Project: {}\nRoot: {}\nBranch: {}\n\nGit Status:\n{}\n\nAllowed Checks: {}\n\nImportant Files:", project_name, root.display(), branch, status, proj.allowed_checks.join(", "));
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
    let request_start = Instant::now();
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
        let ssh_config = projects.ssh.as_ref();
        let resp = match body.mode {
            ContextMode::Overview => ssh_overview(proj, &body.project, ssh_config),
            ContextMode::Tree => ssh_tree(
                proj,
                &body.project,
                body.path.as_deref(),
                body.limit,
                body.max_depth,
                ssh_config,
            ),
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
                ssh_search(proj, &body.project, query, ssh_config)
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
                        error: Some(
                            "query parameter is required for grep_context mode".to_string(),
                        ),
                    }));
                    return;
                };
                ssh_grep_context(
                    proj,
                    &body.project,
                    body.path.as_deref(),
                    query,
                    body.limit,
                    ssh_config,
                )
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
                ssh_read_file(
                    proj,
                    &body.project,
                    rel_path,
                    body.start_line,
                    body.limit,
                    ssh_config,
                )
            }
            ContextMode::MarkdownOutline => match &body.path {
                Some(path) => {
                    let cmd = markdown_outline_shell_fragment(path, body.limit);
                    let (code, stdout, stderr, _) = run_project_cmd(proj, &cmd, 10, ssh_config);
                    if code != 0 {
                        ContextResponse {
                            success: false,
                            project: body.project.clone(),
                            mode: "markdown_outline".to_string(),
                            content: None,
                            items: None,
                            truncated: false,
                            error: Some(format!("markdown_outline failed: {}", stderr.trim())),
                        }
                    } else if let Some(err) = stdout.trim().strip_prefix("__PDCTX_ERROR__:") {
                        ContextResponse {
                            success: false,
                            project: body.project.clone(),
                            mode: "markdown_outline".to_string(),
                            content: None,
                            items: None,
                            truncated: false,
                            error: Some(err.trim().to_string()),
                        }
                    } else {
                        mode_content_response(
                            &body.project,
                            "markdown_outline",
                            stdout,
                            MAX_OUTPUT_LEN,
                        )
                    }
                }
                None => ContextResponse {
                    success: false,
                    project: body.project.clone(),
                    mode: "markdown_outline".to_string(),
                    content: None,
                    items: None,
                    truncated: false,
                    error: Some("path parameter is required for markdown_outline mode".to_string()),
                },
            },
            ContextMode::ReadSection => match (&body.path, &body.query) {
                (Some(path), Some(query)) => {
                    let cmd = markdown_section_shell_fragment(path, query, body.limit);
                    let (code, stdout, stderr, _) = run_project_cmd(proj, &cmd, 10, ssh_config);
                    if code != 0 {
                        ContextResponse {
                            success: false,
                            project: body.project.clone(),
                            mode: "read_section".to_string(),
                            content: None,
                            items: None,
                            truncated: false,
                            error: Some(format!("read_section failed: {}", stderr.trim())),
                        }
                    } else if let Some(err) = stdout.trim().strip_prefix("__PDCTX_ERROR__:") {
                        ContextResponse {
                            success: false,
                            project: body.project.clone(),
                            mode: "read_section".to_string(),
                            content: None,
                            items: None,
                            truncated: false,
                            error: Some(err.trim().to_string()),
                        }
                    } else {
                        mode_content_response(&body.project, "read_section", stdout, MAX_OUTPUT_LEN)
                    }
                }
                _ => ContextResponse {
                    success: false,
                    project: body.project.clone(),
                    mode: "read_section".to_string(),
                    content: None,
                    items: None,
                    truncated: false,
                    error: Some(
                        "path and query parameters are required for read_section mode".to_string(),
                    ),
                },
            },
            ContextMode::AgentContext => {
                let (code, stdout, stderr, _) =
                    run_project_cmd(proj, &agent_context_shell_fragment(), 10, ssh_config);
                if code != 0 {
                    ContextResponse {
                        success: false,
                        project: body.project.clone(),
                        mode: "agent_context".to_string(),
                        content: None,
                        items: None,
                        truncated: false,
                        error: Some(format!("agent_context failed: {}", stderr.trim())),
                    }
                } else {
                    mode_content_response(&body.project, "agent_context", stdout, MAX_OUTPUT_LEN)
                }
            }
            ContextMode::GitStatus => {
                let (code, stdout, stderr, _) =
                    run_project_cmd(proj, git_status_command(), 10, ssh_config);
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
                let (code, stdout, stderr, _) = run_project_cmd(proj, "git diff", 30, ssh_config);
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
            ContextMode::ExperimentOutputs => {
                let cmd = experiment_outputs_shell_fragment(
                    body.limit.max(1).min(500),
                    body.query.as_deref(),
                );
                let (code, stdout, stderr, _) = run_project_cmd(proj, &cmd, 30, ssh_config);
                if code != 0 {
                    ContextResponse {
                        success: false,
                        project: body.project.clone(),
                        mode: "experiment_outputs".to_string(),
                        content: None,
                        items: None,
                        truncated: false,
                        error: Some(format!("experiment_outputs failed: {}", stderr.trim())),
                    }
                } else {
                    let (out, truncated) = truncate_string(stdout, MAX_OUTPUT_LEN);
                    ContextResponse {
                        success: true,
                        project: body.project.clone(),
                        mode: "experiment_outputs".to_string(),
                        content: Some(out),
                        items: None,
                        truncated,
                        error: None,
                    }
                }
            }
        };
        let ssh_calls = match resp.mode.as_str() {
            "overview" | "tree" | "search" | "grep_context" | "read_file" | "markdown_outline"
            | "read_section" | "git_status" | "git_diff" => 1,
            _ => 0,
        };
        tracing::info!(
            target: "codex.metrics",
            operation = "getProjectContext",
            project = %resp.project,
            mode = %resp.mode,
            executor = "ssh",
            success = resp.success,
            duration_ms = request_start.elapsed().as_millis() as u64,
            ssh_calls = ssh_calls,
            control_master = projects.ssh.as_ref().map(|s| s.control_master).unwrap_or(false),
            "codex_context_completed"
        );
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
            }));
            return;
        }
    };
    if body.requests.is_empty() || body.requests.len() > 20 {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ContextBatchResponse {
            success: false,
            project: body.project,
            results: Vec::new(),
            duration_ms: 0,
            ssh_calls: 0,
            error: Some("requests must contain 1-20 items".to_string()),
            preflight_rejected: None,
            estimated_chars: None,
            max_allowed_chars: None,
            max_allowed_items: None,
            project_is_ssh: None,
            suggestion: None,
            warnings: Vec::new(),
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
            }));
            return;
        }
    };

    // Preflight check: reject obviously oversized requests before execution
    let project_is_ssh = proj.is_ssh();
    match preflight_context_batch(&body, project_is_ssh, &body.project) {
        Ok(preflight_warnings) => {
            // Request is allowed; preflight_warnings may contain non-blocking hints
            let start = Instant::now();
            let (mut results, ssh_calls) = if project_is_ssh {
                match try_ssh_context_batch_once(
                    proj,
                    &body.project,
                    &body.requests,
                    projects.ssh.as_ref(),
                ) {
                    Some((results, ssh_calls)) => (results, ssh_calls),
                    None => {
                        let mut ssh_calls = 0;
                        let mut results = Vec::with_capacity(body.requests.len());
                        for item in &body.requests {
                            let (resp, calls) = execute_context_item(
                                proj,
                                &body.project,
                                item,
                                projects.ssh.as_ref(),
                            );
                            ssh_calls += calls;
                            results.push(resp);
                        }
                        (results, ssh_calls)
                    }
                }
            } else {
                let mut results = Vec::with_capacity(body.requests.len());
                for item in &body.requests {
                    let (resp, _) = execute_context_item(proj, &body.project, item, None);
                    results.push(resp);
                }
                (results, 0)
            };
            enforce_context_batch_total_limit(&mut results, body.max_total_chars);
            let success = results.iter().all(|r| r.success);
            let duration_ms = start.elapsed().as_millis() as u64;
            tracing::info!(
                target: "codex.metrics",
                operation = "getProjectContextBatch",
                project = %body.project,
                executor = if project_is_ssh { "ssh" } else { "local" },
                success = success,
                request_count = results.len(),
                duration_ms = duration_ms,
                ssh_calls = ssh_calls,
                control_master = projects.ssh.as_ref().map(|s| s.control_master).unwrap_or(false),
                "codex_context_batch_completed"
            );
            res.render(Json(ContextBatchResponse {
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
            }));
        }
        Err(rejection_response) => {
            // Preflight rejected the request — return structured error
            tracing::warn!(
                target: "codex.metrics",
                operation = "getProjectContextBatch",
                project = %body.project,
                executor = if project_is_ssh { "ssh" } else { "local" },
                estimated_chars = ?rejection_response.estimated_chars,
                "preflight_rejected"
            );
            res.render(Json(rejection_response));
        }
    }
}
