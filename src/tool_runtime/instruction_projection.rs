//! Deterministic projection of already-observed instruction snapshots.
//! Startup and explicit Context share this leaf; authorization, loading and
//! aggregate-envelope priority remain with their respective callers.
use super::project_instructions::{
    ProjectInstructionFile, ProjectInstructionsSnapshot, ProjectInstructionsSummarySnapshot,
    MAX_LINES_PER_FILE,
};
use super::projection_text::{bounded_json_string, json_string_payload_len, serialized_len};
use serde_json::{json, Value};

const INSTRUCTION_CONTENT_JSON_BUDGET: usize = 10 * 1024;
const MAX_RULE_HEADINGS: usize = 6;
const MAX_RULE_HEADING_JSON_BYTES: usize = 160;

pub(super) fn project_instructions_context_projection(
    current: &ProjectInstructionsSnapshot,
    max_bytes: usize,
) -> Value {
    // Sidecar requests observe current sources without Session retention. Its
    // shared envelope is smaller than startup, especially with 16 global files.
    let mut projection = instructions_projection(current, None, true, true, false);
    if serialized_len(&projection) <= max_bytes {
        return projection;
    }
    // Headings duplicate the body; remove this optional index before losing
    // actual guidance or source identities.
    if let Some(sources) = projection["sources"].as_array_mut() {
        for source in sources {
            source["headings"] = json!([]);
        }
    }
    while serialized_len(&projection) > max_bytes {
        let largest = projection["sources"].as_array().and_then(|sources| {
            sources
                .iter()
                .enumerate()
                .filter_map(|(index, source)| {
                    source["content"]
                        .as_str()
                        .filter(|body| !body.is_empty())
                        .map(|body| (index, json_string_payload_len(body)))
                })
                .max_by_key(|(_, bytes)| *bytes)
        });
        let Some((index, bytes)) = largest else {
            // Essential source metadata itself does not fit. The owning
            // sidecar envelope will return its existing explicit budget error.
            break;
        };
        trim_instruction_source(&mut projection["sources"][index], bytes / 2);
        projection["truncated"] = json!(true);
    }
    projection
}

pub(super) fn instructions_projection(
    current: &ProjectInstructionsSnapshot,
    previous: Option<&ProjectInstructionsSummarySnapshot>,
    force_load: bool,
    allow_content: bool,
    minimal: bool,
) -> Value {
    let status = instruction_status(current, previous, force_load);
    let include_content = allow_content
        && (matches!(status, "loaded" | "changed")
            || (status == "unavailable" && !current.files.is_empty()));
    let changed_sources = if matches!(status, "changed" | "unavailable") {
        changed_instruction_sources(current, previous)
    } else {
        Vec::new()
    };
    let mut remaining_content_budget = INSTRUCTION_CONTENT_JSON_BUDGET;
    let mut projection_truncated = false;
    let source_count = current.files.len();
    let sources: Vec<Value> = current
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            instruction_source_projection(
                file,
                include_content,
                minimal,
                source_count.saturating_sub(index),
                &mut remaining_content_budget,
                &mut projection_truncated,
            )
        })
        .collect();
    json!({
        "status": status,
        "sources": sources,
        "changed_sources": changed_sources,
        "content_included": include_content,
        "truncated": current.truncated || projection_truncated,
        "total_chars": current.total_chars,
    })
}

fn instruction_status(
    current: &ProjectInstructionsSnapshot,
    previous: Option<&ProjectInstructionsSummarySnapshot>,
    force_load: bool,
) -> &'static str {
    if !current.scan_complete {
        return "unavailable";
    }
    if !current.loaded {
        return if previous.is_some_and(|snapshot| snapshot.loaded) {
            "changed"
        } else {
            "not_found"
        };
    }
    let Some(previous) = previous else {
        return "loaded";
    };
    if force_load {
        return "loaded";
    }
    if instruction_snapshots_match(current, previous) {
        "reused"
    } else {
        "changed"
    }
}

fn instruction_snapshots_match(
    current: &ProjectInstructionsSnapshot,
    previous: &ProjectInstructionsSummarySnapshot,
) -> bool {
    current.loaded == previous.loaded
        && current.truncated == previous.truncated
        && current.total_chars == previous.total_chars
        && current.files.len() == previous.files.len()
        && current
            .files
            .iter()
            .zip(&previous.files)
            .all(|(left, right)| {
                left.source_scope == right.source_scope
                    && left.path == right.path
                    && left.fingerprint == right.fingerprint
                    && left.truncated == right.truncated
            })
}

pub(super) fn changed_instruction_sources(
    current: &ProjectInstructionsSnapshot,
    previous: Option<&ProjectInstructionsSummarySnapshot>,
) -> Vec<String> {
    let mut identities: Vec<_> = current
        .files
        .iter()
        .map(|file| (file.source_scope, file.path.as_str()))
        .collect();
    if let Some(previous) = previous {
        for file in &previous.files {
            let identity = (file.source_scope, file.path.as_str());
            if !identities.contains(&identity) {
                identities.push(identity);
            }
        }
    }

    identities
        .into_iter()
        .filter_map(|(scope, path)| {
            if !current.scope_complete(scope) {
                return None;
            }
            let current_file = current
                .files
                .iter()
                .find(|file| file.source_scope == scope && file.path == path);
            let previous_file = previous.and_then(|snapshot| {
                snapshot
                    .files
                    .iter()
                    .find(|file| file.source_scope == scope && file.path == path)
            });
            let differs = match (current_file, previous_file) {
                (Some(left), Some(right)) => {
                    left.fingerprint != right.fingerprint || left.truncated != right.truncated
                }
                (None, None) => false,
                _ => true,
            };
            differs.then(|| path.to_string())
        })
        .collect()
}

fn instruction_source_projection(
    file: &ProjectInstructionFile,
    include_content: bool,
    minimal: bool,
    remaining_sources: usize,
    remaining_content_budget: &mut usize,
    projection_truncated: &mut bool,
) -> Value {
    let headings = if minimal {
        Vec::new()
    } else {
        file.content
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with('#'))
            .take(MAX_RULE_HEADINGS)
            .map(|line| bounded_json_string(line, MAX_RULE_HEADING_JSON_BYTES).0)
            .collect()
    };
    let (content, content_truncated) = if include_content {
        // Divide the remaining aggregate budget across the remaining sources.
        // Short earlier files leave their unused share available, while a long
        // earlier file cannot starve a later changed rule of all content.
        let source_budget = *remaining_content_budget / remaining_sources.max(1);
        let (content, truncated) = bounded_json_string(&file.content, source_budget);
        *remaining_content_budget =
            remaining_content_budget.saturating_sub(json_string_payload_len(&content));
        (Some(content), truncated)
    } else {
        (None, false)
    };
    *projection_truncated |= content_truncated;
    let read_more = if content_truncated
        && file.source_scope == super::project_instructions::InstructionSourceScope::Project
    {
        let returned = content.as_deref().unwrap_or_default();
        projected_read_more(&file.path, returned)
    } else if content_truncated {
        Value::Null
    } else {
        serde_json::to_value(&file.read_more).unwrap_or(Value::Null)
    };
    json!({
        "source_scope": file.source_scope,
        "path": file.path,
        "fingerprint": file.fingerprint,
        "truncated": file.truncated || content_truncated,
        "headings": headings,
        "content": content,
        "read_more": read_more,
    })
}

fn projected_read_more(path: &str, returned: &str) -> Value {
    let observed_lines = returned.lines().count().max(1);
    let start_line = if returned.ends_with('\n') {
        observed_lines.saturating_add(1)
    } else {
        // The byte-bound projection may end midway through a line. Re-reading
        // that line is conservative and avoids losing its unseen suffix.
        observed_lines
    };
    json!({
        "path": path,
        "start_line": start_line,
        "limit": MAX_LINES_PER_FILE,
    })
}

/// A single source-shortening rule shared by the Context budget and both startup
/// hard-budget passes. The caller owns priority and aggregate truncation flags.
pub(super) fn trim_instruction_source(source: &mut Value, budget: usize) {
    let body = source["content"].as_str().unwrap_or_default();
    let (bounded, _) = bounded_json_string(body, budget);
    source["read_more"] = if source["source_scope"] == "project" {
        projected_read_more(source["path"].as_str().unwrap_or_default(), &bounded)
    } else {
        Value::Null
    };
    source["content"] = json!(bounded);
    source["truncated"] = json!(true);
}

#[cfg(test)]
mod tests;
