//! Git display owns relative-path filtering, diff budgets and its registrations.
//! This module only projects observed evidence; it never reads Git or grants authority.
use super::registry::PresentationRenderer;
use super::{
    bounded_text, copy_bounded_text, copy_scalar, MAX_MCP_PRESENTATION_DIFF_CHARS,
    MAX_MCP_PRESENTATION_DIFF_HUNKS, MAX_MCP_PRESENTATION_DIFF_LINES, MAX_MCP_PRESENTATION_ITEMS,
    MCP_PRESENTATION_VERSION,
};
use serde_json::{json, Map, Value};

pub(super) const CHANGES: PresentationRenderer = PresentationRenderer {
    tools: &["read_workspace_changes"],
    project: |_, output| show_changes_presentation(output),
};
pub(super) const REVIEW: PresentationRenderer = PresentationRenderer {
    tools: &["read_git_review_summary"],
    project: |_, output| git_review_presentation(output),
};
fn safe_label(value: &Value) -> Option<String> {
    let value = value.as_str()?;
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-'))
    {
        return None;
    }
    bounded_text(&Value::String(value.to_string()))
}

fn validated_repo_relative_path(value: &Value) -> Option<&str> {
    let path = value.as_str()?;
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains("://")
        || path.chars().any(char::is_control)
        || path
            .split(|ch| ch == '/' || ch == '\\')
            .any(|component| component == "..")
    {
        return None;
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
    {
        return None;
    }
    Some(path)
}

fn safe_repo_relative_path(value: &Value) -> Option<String> {
    validated_repo_relative_path(value)?;
    bounded_text(value)
}

fn short_git_commit(value: &Value) -> Option<String> {
    let value = value.as_str()?;
    if !(4..=40).contains(&value.len()) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(value[..value.len().min(8)].to_ascii_lowercase())
}

fn bounded_safe_labels(source: &Value) -> (Vec<Value>, bool) {
    let Some(source) = source.as_array() else {
        return (Vec::new(), false);
    };
    let mut values = Vec::new();
    let mut truncated = false;
    for value in source {
        if values.len() == MAX_MCP_PRESENTATION_ITEMS {
            truncated = true;
            break;
        }
        if let Some(value) = safe_label(value) {
            values.push(Value::String(value));
        } else {
            truncated = true;
        }
    }
    (
        values,
        truncated || source.len() > MAX_MCP_PRESENTATION_ITEMS,
    )
}

fn bounded_diff_text(value: &Value) -> Option<(String, bool)> {
    let value = value.as_str()?;
    if value
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    {
        return None;
    }
    let source_lines = value.lines().collect::<Vec<_>>();
    let mut truncated = source_lines.len() > MAX_MCP_PRESENTATION_DIFF_LINES;
    let line_bounded = source_lines
        .into_iter()
        .take(MAX_MCP_PRESENTATION_DIFF_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    let mut chars = line_bounded.chars();
    let bounded = chars
        .by_ref()
        .take(MAX_MCP_PRESENTATION_DIFF_CHARS)
        .collect::<String>();
    if chars.next().is_some() {
        truncated = true;
    }
    Some((bounded, truncated))
}

fn show_changes_diff_hunks_for_path(
    output: &Value,
    raw_path: &str,
    max_hunks: usize,
) -> (Vec<Value>, bool) {
    let Some(files) = output.get("hunks").and_then(Value::as_array) else {
        return (Vec::new(), false);
    };
    let mut result = Vec::new();
    let mut truncated = false;
    for file in files {
        let Some(file_path) = file.get("path").and_then(validated_repo_relative_path) else {
            continue;
        };
        if file_path != raw_path {
            continue;
        }
        let Some(hunks) = file.get("hunks").and_then(Value::as_array) else {
            continue;
        };
        for hunk in hunks {
            if result.len() == max_hunks {
                truncated = true;
                break;
            }
            let Some((diff, diff_truncated)) = hunk.get("diff").and_then(bounded_diff_text) else {
                truncated = true;
                continue;
            };
            let hunk_truncated =
                diff_truncated || hunk.get("truncated").and_then(Value::as_bool) == Some(true);
            if hunk_truncated {
                truncated = true;
            }
            let mut projected = Map::new();
            projected.insert("diff".to_string(), Value::String(diff));
            projected.insert("truncated".to_string(), Value::Bool(hunk_truncated));
            result.push(Value::Object(projected));
        }
        break;
    }
    (result, truncated)
}

fn show_changes_file_presentation(
    file: &Value,
    output: &Value,
    max_diff_hunks: usize,
) -> Option<Value> {
    file.as_object()?;
    let path_value = file.get("path")?;
    let raw_path = validated_repo_relative_path(path_value)?;
    let path = bounded_text(path_value)?;
    let mut item = Map::new();
    item.insert("path".to_string(), Value::String(path.clone()));
    if let Some(status) = file.get("status").and_then(safe_label) {
        item.insert("status".to_string(), Value::String(status));
    }
    if let Some(kind) = file.get("kind").and_then(safe_label) {
        item.insert("kind".to_string(), Value::String(kind));
    }
    for key in ["staged", "unstaged", "additions", "deletions"] {
        copy_scalar(file, &mut item, key);
    }
    if let Some(old_path) = file.get("old_path").and_then(safe_repo_relative_path) {
        item.insert("old_path".to_string(), Value::String(old_path));
    }
    let (diff_hunks, diff_truncated) =
        show_changes_diff_hunks_for_path(output, raw_path, max_diff_hunks);
    if !diff_hunks.is_empty() {
        item.insert("diff_hunks".to_string(), Value::Array(diff_hunks));
    }
    if diff_truncated {
        item.insert("diff_truncated".to_string(), Value::Bool(true));
    }
    Some(Value::Object(item))
}

fn show_changes_status_observation(output: &Value) -> Option<Value> {
    let observation = output.get("status_observation")?;
    observation.as_object()?;
    let mut result = Map::new();
    for key in ["status", "reason_code"] {
        if let Some(value) = observation.get(key).and_then(safe_label) {
            result.insert(key.to_string(), Value::String(value));
        }
    }
    copy_scalar(observation, &mut result, "exit_code");
    (!result.is_empty()).then_some(Value::Object(result))
}

fn show_changes_presentation(output: &Value) -> Option<Value> {
    output.as_object()?;
    let mut presentation = Map::new();
    presentation.insert("version".to_string(), Value::from(MCP_PRESENTATION_VERSION));
    presentation.insert("kind".to_string(), Value::String("git_changes".to_string()));
    for key in ["branch"] {
        copy_bounded_text(output, &mut presentation, key);
    }
    for key in ["upstream_status", "upstream_reason_code"] {
        if let Some(value) = output.get(key).and_then(safe_label) {
            presentation.insert(key.to_string(), Value::String(value));
        }
    }
    for key in [
        "git_available",
        "non_git_project",
        "ahead",
        "behind",
        "clean",
        "files_total",
        "files_returned",
        "files_truncated",
        "files_limit",
        "transport_safe",
        "output_truncated",
    ] {
        copy_scalar(output, &mut presentation, key);
    }
    if let Some(observation) = show_changes_status_observation(output) {
        presentation.insert("status_observation".to_string(), observation);
    }
    if let Some(short) = output.pointer("/head/short").and_then(short_git_commit) {
        presentation.insert("head".to_string(), json!({"short": short}));
    }
    if let Some(counts) = output.get("counts") {
        if counts.is_object() {
            let mut bounded_counts = Map::new();
            for key in [
                "modified",
                "added",
                "deleted",
                "renamed",
                "copied",
                "untracked",
                "conflicted",
                "staged",
                "unstaged",
            ] {
                copy_scalar(counts, &mut bounded_counts, key);
            }
            presentation.insert("counts".to_string(), Value::Object(bounded_counts));
        }
    }
    let (truncation_reasons, reasons_truncated) = output
        .get("truncation_reasons")
        .map(bounded_safe_labels)
        .unwrap_or_default();
    if !truncation_reasons.is_empty() {
        presentation.insert(
            "truncation_reasons".to_string(),
            Value::Array(truncation_reasons),
        );
    }
    if reasons_truncated {
        presentation.insert(
            "truncation_reasons_truncated".to_string(),
            Value::Bool(true),
        );
    }

    if let Some(source_files) = output.get("files").and_then(Value::as_array) {
        let clean = output.get("clean").and_then(Value::as_bool) == Some(true);
        let mut additions = 0u64;
        let mut deletions = 0u64;
        let mut line_stats_observed = false;
        let mut line_stats_partial = output
            .get("files_truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if clean {
            presentation.insert("additions".to_string(), Value::from(0));
            presentation.insert("deletions".to_string(), Value::from(0));
            presentation.insert("line_stats_partial".to_string(), Value::Bool(false));
        } else if !source_files.is_empty() {
            for file in source_files {
                match (
                    file.get("additions").and_then(Value::as_u64),
                    file.get("deletions").and_then(Value::as_u64),
                ) {
                    (Some(file_additions), Some(file_deletions)) => {
                        line_stats_observed = true;
                        additions = additions.saturating_add(file_additions);
                        deletions = deletions.saturating_add(file_deletions);
                    }
                    _ => line_stats_partial = true,
                }
            }
            if line_stats_observed {
                presentation.insert("additions".to_string(), Value::from(additions));
                presentation.insert("deletions".to_string(), Value::from(deletions));
                presentation.insert(
                    "line_stats_partial".to_string(),
                    Value::Bool(line_stats_partial),
                );
            }
        }
        let presented_source_count = source_files.len().min(MAX_MCP_PRESENTATION_ITEMS).max(1);
        let max_hunks_per_file = (MAX_MCP_PRESENTATION_DIFF_HUNKS / presented_source_count).max(1);
        let mut files = Vec::new();
        let mut items_truncated = false;
        for file in source_files {
            if files.len() == MAX_MCP_PRESENTATION_ITEMS {
                items_truncated = true;
                break;
            }
            if let Some(file) = show_changes_file_presentation(file, output, max_hunks_per_file) {
                files.push(file);
            } else {
                items_truncated = true;
            }
        }
        let presentation_diff_truncated = files
            .iter()
            .any(|file| file.get("diff_truncated").and_then(Value::as_bool) == Some(true));
        if output.get("hunks_truncated").and_then(Value::as_bool) == Some(true)
            || presentation_diff_truncated
        {
            presentation.insert("diff_truncated".to_string(), Value::Bool(true));
        }
        presentation.insert("files".to_string(), Value::Array(files));
        presentation.insert(
            "items_truncated".to_string(),
            Value::Bool(items_truncated || source_files.len() > MAX_MCP_PRESENTATION_ITEMS),
        );
    }
    Some(Value::Object(presentation))
}

fn review_file_presentation(file: &Value) -> Option<Value> {
    file.as_object()?;
    let path_omitted = file
        .get("path_omitted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let path = if path_omitted {
        None
    } else {
        match file.get("path") {
            Some(value) if value.is_string() => Some(safe_repo_relative_path(value)?),
            _ => return None,
        }
    };
    let mut item = Map::new();
    if let Some(path) = path {
        item.insert("path".to_string(), Value::String(path));
    }
    item.insert("path_omitted".to_string(), Value::Bool(path_omitted));
    if let Some(previous_path) = file.get("previous_path").and_then(safe_repo_relative_path) {
        item.insert("previous_path".to_string(), Value::String(previous_path));
    }
    if let Some(status) = file.get("status").and_then(safe_label) {
        item.insert("status".to_string(), Value::String(status));
    }
    for key in ["additions", "deletions", "binary", "gitlink"] {
        copy_scalar(file, &mut item, key);
    }
    let (classes, classes_truncated) = file
        .get("classes")
        .map(bounded_safe_labels)
        .unwrap_or_default();
    if !classes.is_empty() {
        item.insert("classes".to_string(), Value::Array(classes));
    }
    if classes_truncated {
        item.insert("classes_truncated".to_string(), Value::Bool(true));
    }
    Some(Value::Object(item))
}

fn git_review_file_classes_presentation(output: &Value) -> Option<Value> {
    let classes = output.get("file_classes")?;
    classes.as_object()?;
    let mut result = Map::new();
    copy_scalar(classes, &mut result, "partial");
    if let Some(counts) = classes.get("counts_observed").and_then(Value::as_object) {
        let mut bounded_counts = Map::new();
        let mut truncated = false;
        for (key, value) in counts {
            if bounded_counts.len() == MAX_MCP_PRESENTATION_ITEMS {
                truncated = true;
                break;
            }
            let label = safe_label(&Value::String(key.clone()));
            if let Some(label) = label.filter(|_| value.is_number()) {
                bounded_counts.insert(label, value.clone());
            } else {
                truncated = true;
            }
        }
        result.insert("counts_observed".to_string(), Value::Object(bounded_counts));
        result.insert(
            "counts_truncated".to_string(),
            Value::Bool(truncated || counts.len() > MAX_MCP_PRESENTATION_ITEMS),
        );
    }
    Some(Value::Object(result))
}

fn git_review_presentation(output: &Value) -> Option<Value> {
    output.as_object()?;
    let mut presentation = Map::new();
    presentation.insert("version".to_string(), Value::from(MCP_PRESENTATION_VERSION));
    presentation.insert("kind".to_string(), Value::String("git_review".to_string()));
    for key in ["deterministic", "truncated"] {
        copy_scalar(output, &mut presentation, key);
    }
    if let Some(reason_code) = output.get("reason_code").and_then(safe_label) {
        presentation.insert("reason_code".to_string(), Value::String(reason_code));
    }
    if let Some(scope) = output.get("scope") {
        if scope.is_object() {
            let mut bounded_scope = Map::new();
            for (source, target) in [
                ("requested_base", "base"),
                ("requested_head", "head"),
                ("merge_base", "merge_base"),
            ] {
                if let Some(value) = scope.get(source).and_then(short_git_commit) {
                    bounded_scope.insert(target.to_string(), Value::String(value));
                }
            }
            for key in ["base_is_ancestor", "commit_count"] {
                copy_scalar(scope, &mut bounded_scope, key);
            }
            presentation.insert("scope".to_string(), Value::Object(bounded_scope));
        }
    }
    for (source_key, target_key, fields) in [
        (
            "stats",
            "stats",
            &["files_changed", "insertions", "deletions", "binary_files"][..],
        ),
        (
            "coverage",
            "coverage",
            &[
                "production_changed",
                "tests_changed",
                "docs_changed",
                "partial",
            ][..],
        ),
        (
            "truncation",
            "truncation",
            &[
                "files_total",
                "files_returned",
                "files_truncated",
                "classification_partial",
                "file_stats_partial",
                "file_modes_partial",
                "symbols_partial",
                "subsystems_partial",
                "signals_partial",
            ][..],
        ),
    ] {
        if let Some(source) = output.get(source_key).filter(|value| value.is_object()) {
            let mut target = Map::new();
            for key in fields {
                copy_scalar(source, &mut target, key);
            }
            presentation.insert(target_key.to_string(), Value::Object(target));
        }
    }
    if let Some(classes) = git_review_file_classes_presentation(output) {
        presentation.insert("file_classes".to_string(), classes);
    }
    if let Some(source_files) = output.get("files").and_then(Value::as_array) {
        let mut files = Vec::new();
        let mut items_truncated = false;
        for file in source_files {
            if files.len() == MAX_MCP_PRESENTATION_ITEMS {
                items_truncated = true;
                break;
            }
            if let Some(file) = review_file_presentation(file) {
                files.push(file);
            } else {
                items_truncated = true;
            }
        }
        presentation.insert("files".to_string(), Value::Array(files));
        presentation.insert(
            "items_truncated".to_string(),
            Value::Bool(items_truncated || source_files.len() > MAX_MCP_PRESENTATION_ITEMS),
        );
    }
    Some(Value::Object(presentation))
}

#[cfg(test)]
mod tests;
