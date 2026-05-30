use std::path::Path;

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
