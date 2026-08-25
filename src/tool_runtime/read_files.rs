//! Bounded multi-file reads built from the canonical single-file read core.

use super::project_resolution::ResolvedProject;
use super::{ReadFilesItem, ToolResult, ToolRuntime};
use futures_util::{stream, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::Instant;
use webcodex_workspace::file_read_normalize::MODEL_RESULT_ENVELOPE_RESERVE_BYTES;
use webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES;

pub(crate) const MAX_READ_FILES_ITEMS: usize = 8;
pub(crate) const MAX_READ_FILES_CONCURRENCY: usize = 4;
pub(crate) const DEFAULT_READ_FILES_DEADLINE: Duration = Duration::from_secs(30);
pub(crate) const DEFAULT_READ_FILES_RESULT_BYTES: usize = 64 * 1024;
pub(crate) const MIN_READ_FILES_RESULT_BYTES: usize = 8 * 1024;

fn normalized_result_budget(max_result_bytes: Option<usize>) -> usize {
    max_result_bytes
        .unwrap_or(DEFAULT_READ_FILES_RESULT_BYTES)
        .clamp(MIN_READ_FILES_RESULT_BYTES, MAX_SERIALIZED_OUTPUT_BYTES)
}

fn batch_output(
    project: &str,
    requested_count: usize,
    items: Vec<Value>,
    output_truncated: bool,
    next_index: Option<usize>,
    truncation_reason: Option<&str>,
) -> Value {
    let succeeded_count = items
        .iter()
        .filter(|item| item["success"].as_bool() == Some(true))
        .count();
    let returned_count = items.len();
    let mut output = json!({
        "project": project,
        "requested_count": requested_count,
        "returned_count": returned_count,
        "succeeded_count": succeeded_count,
        "failed_count": returned_count - succeeded_count,
        "items": items,
        "output_truncated": output_truncated,
        "next_index": next_index,
    });
    if let Some(reason) = truncation_reason {
        output["truncation_reason"] = json!(reason);
    }
    output
}

#[cfg(test)]
fn serialized_batch_len(output: &Value) -> usize {
    serde_json::to_vec(&ToolResult::ok(output.clone()))
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

fn serialized_value_len(value: &Value) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

fn projected_batch_serialized_len(output: &Value) -> usize {
    let mut projected = ToolResult::ok(output.clone());
    super::dispatch::sparsify_complete_read_success("read_files", &mut projected);
    serde_json::to_vec(&projected)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

fn projected_read_item_len(item: &Value) -> usize {
    let mut projected = item.clone();
    if projected["success"].as_bool() == Some(true) {
        let outer_path = projected
            .get("path")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let (Some(outer_path), Some(output)) = (
            outer_path,
            projected.get_mut("output").and_then(Value::as_object_mut),
        ) {
            super::dispatch::sparsify_complete_file_read_output(output, Some(&outer_path));
        }
    }
    serialized_value_len(&projected)
}

fn projected_batch_len(base_len: usize, item_bytes: usize, item_count: usize) -> usize {
    base_len
        .saturating_add(item_bytes)
        .saturating_add(item_count.saturating_sub(1))
}

fn truncate_read_item(item: &Value, keep_lines: usize) -> Option<Value> {
    if item["success"].as_bool() != Some(true) || keep_lines == 0 {
        return None;
    }
    let output = item.get("output")?.as_object()?;
    let returned_lines = output.get("returned_lines")?.as_u64()? as usize;
    if keep_lines >= returned_lines || returned_lines <= 1 {
        return None;
    }
    let start_line = output.get("start_line")?.as_u64()? as usize;
    let text = output.get("text")?.as_str()?;
    let lines = text.split('\n').collect::<Vec<_>>();
    if lines.len() != returned_lines {
        return None;
    }

    let mut projected = item.clone();
    let projected_output = projected.get_mut("output")?.as_object_mut()?;
    projected_output.insert("text".to_string(), json!(lines[..keep_lines].join("\n")));
    projected_output.insert("returned_lines".to_string(), json!(keep_lines));
    projected_output.insert(
        "end_line".to_string(),
        json!(start_line.saturating_add(keep_lines).saturating_sub(1)),
    );
    projected_output.insert("has_more".to_string(), json!(true));
    projected_output.insert(
        "next_start_line".to_string(),
        json!(start_line.saturating_add(keep_lines)),
    );
    projected_output.insert("budget_truncated".to_string(), json!(true));
    let existing_budget_next_limit = output
        .get("budget_next_limit")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    projected_output.insert(
        "budget_next_limit".to_string(),
        json!(returned_lines
            .saturating_sub(keep_lines)
            .saturating_add(existing_budget_next_limit)),
    );
    Some(projected)
}

fn truncate_read_item_to_fit(item: &Value, max_item_bytes: usize) -> Option<Value> {
    let returned_lines = item.get("output")?.get("returned_lines")?.as_u64()? as usize;
    if returned_lines <= 1 {
        return None;
    }

    let mut low = 1usize;
    let mut high = returned_lines - 1;
    let mut best = None;
    while low <= high {
        let keep = low + (high - low) / 2;
        let Some(candidate) = truncate_read_item(item, keep) else {
            break;
        };
        if serialized_value_len(&candidate) <= max_item_bytes {
            best = Some(candidate);
            low = keep.saturating_add(1);
        } else {
            high = keep.saturating_sub(1);
        }
    }
    best
}

fn apply_output_budget(
    project: &str,
    requested_count: usize,
    completed: Vec<Value>,
    max_result_bytes: Option<usize>,
) -> Value {
    let result_budget = normalized_result_budget(max_result_bytes);
    let payload_budget = result_budget.saturating_sub(MODEL_RESULT_ENVELOPE_RESERVE_BYTES);
    let complete = batch_output(
        project,
        requested_count,
        completed.clone(),
        false,
        None,
        None,
    );
    // Budget the shape the model would actually receive after the existing
    // sparse projection. The canonical representation remains available for
    // Session recording and for any partial-item cursor construction below.
    if projected_batch_serialized_len(&complete) <= payload_budget {
        return complete;
    }

    let truncation_reason = if result_budget == MAX_SERIALIZED_OUTPUT_BYTES {
        "hard_result_cap"
    } else {
        "batch_response_budget"
    };
    // The truncated empty shape fixes all outer-field byte costs up front.
    // Counts and indices are single digits for this 1..=8 batch, so each item
    // can then be accounted exactly by its own serialized size plus one comma.
    let base_len = projected_batch_serialized_len(&batch_output(
        project,
        requested_count,
        Vec::new(),
        true,
        Some(0),
        Some(truncation_reason),
    ));
    let mut returned = Vec::with_capacity(completed.len());
    let mut returned_item_bytes = 0usize;
    let mut next_index = None;

    for item in completed {
        let index = item["index"].as_u64().unwrap_or(returned.len() as u64) as usize;
        let item_len = projected_read_item_len(&item);
        let candidate_item_count = returned.len() + 1;
        if projected_batch_len(
            base_len,
            returned_item_bytes.saturating_add(item_len),
            candidate_item_count,
        ) <= payload_budget
        {
            returned_item_bytes = returned_item_bytes.saturating_add(item_len);
            returned.push(item);
            continue;
        }

        let separator_bytes = usize::from(!returned.is_empty());
        let max_item_bytes = payload_budget
            .saturating_sub(base_len)
            .saturating_sub(returned_item_bytes)
            .saturating_sub(separator_bytes);
        if let Some(partial) = truncate_read_item_to_fit(&item, max_item_bytes) {
            returned.push(partial);
        }
        // A partial current item resumes from its next_start_line, while an
        // omitted item resumes from its original range. In both cases the
        // existing next_index can deterministically point at this same item.
        next_index = Some(index);
        break;
    }

    let mut output = batch_output(
        project,
        requested_count,
        returned,
        true,
        next_index,
        Some(truncation_reason),
    );
    // Defensive exact serialization fallback. Normal accounting above is O(n)
    // plus an O(log lines) partial-item search; this loop should never execute
    // unless a future outer-field change invalidates the fixed-size arithmetic.
    while projected_batch_serialized_len(&output) > payload_budget {
        let Some(items) = output.get_mut("items").and_then(Value::as_array_mut) else {
            break;
        };
        let Some(removed) = items.pop() else {
            break;
        };
        next_index = removed["index"].as_u64().map(|index| index as usize);
        output = batch_output(
            project,
            requested_count,
            items.clone(),
            true,
            next_index,
            Some(truncation_reason),
        );
    }
    output
}

pub(crate) fn apply_model_facing_output_budget(
    result: &mut ToolResult,
    max_result_bytes: Option<usize>,
) {
    if !result.success {
        return;
    }
    let Some(output) = result.output.as_object() else {
        return;
    };
    let Some(project) = output
        .get("project")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return;
    };
    let Some(requested_count) = output
        .get("requested_count")
        .and_then(Value::as_u64)
        .map(|count| count as usize)
    else {
        return;
    };
    let Some(completed) = output.get("items").and_then(Value::as_array).cloned() else {
        return;
    };

    let budgeted = apply_output_budget(&project, requested_count, completed, max_result_bytes);
    let Some(root) = result.output.as_object_mut() else {
        return;
    };
    for key in [
        "project",
        "requested_count",
        "returned_count",
        "succeeded_count",
        "failed_count",
        "items",
        "output_truncated",
        "next_index",
        "truncation_reason",
    ] {
        root.remove(key);
    }
    if let Some(budgeted) = budgeted.as_object() {
        for (key, value) in budgeted {
            root.insert(key.clone(), value.clone());
        }
    }
}

fn final_model_result_len(output: &Value) -> usize {
    let mut projected = ToolResult::ok(output.clone());
    super::dispatch::sparsify_complete_read_success("read_files", &mut projected);
    serde_json::to_vec(&projected)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

fn mark_final_hard_cap_truncation(output: &mut Value, next_index: usize) {
    let Some(root) = output.as_object_mut() else {
        return;
    };
    let (returned_count, succeeded_count) = root
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            (
                items.len(),
                items
                    .iter()
                    .filter(|item| item["success"].as_bool() == Some(true))
                    .count(),
            )
        })
        .unwrap_or_default();
    root.insert("returned_count".to_string(), json!(returned_count));
    root.insert("succeeded_count".to_string(), json!(succeeded_count));
    root.insert(
        "failed_count".to_string(),
        json!(returned_count.saturating_sub(succeeded_count)),
    );
    root.insert("output_truncated".to_string(), json!(true));
    root.insert("next_index".to_string(), json!(next_index));
    root.insert("truncation_reason".to_string(), json!("hard_result_cap"));
}

/// Enforce the repository-wide 256 KiB ceiling against the actual final
/// serialized ToolResult, including Session/continuity overlays. The primary
/// batch budget remains independent; this pass only removes/shortens read body
/// content when the fully decorated response would otherwise violate the hard
/// cap.
///
/// This expects the canonical batch envelope (before complete-success sparse
/// projection), so project/count/continuation metadata can remain truthful when
/// final hard-cap pressure turns a previously complete response into a partial
/// one.
pub(crate) fn enforce_final_model_facing_hard_cap(result: &mut ToolResult) {
    if !result.success || final_model_result_len(&result.output) <= MAX_SERIALIZED_OUTPUT_BYTES {
        return;
    }
    let Some(root) = result.output.as_object() else {
        return;
    };
    if root.get("project").and_then(Value::as_str).is_none()
        || root
            .get("requested_count")
            .and_then(Value::as_u64)
            .is_none()
        || root.get("items").and_then(Value::as_array).is_none()
    {
        return;
    }

    loop {
        let Some(items) = result.output.get("items").and_then(Value::as_array) else {
            return;
        };
        let Some(last) = items.last() else {
            return;
        };
        let index = last["index"].as_u64().unwrap_or(0) as usize;

        // Preserve as many whole source lines as possible from the last success
        // item. Measuring the complete decorated ToolResult makes this exact and
        // naturally accounts for recovery/handoff/attention bytes.
        if last["success"].as_bool() == Some(true) {
            let returned_lines = last
                .get("output")
                .and_then(|output| output.get("returned_lines"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            if returned_lines > 1 {
                let mut low = 1usize;
                let mut high = returned_lines - 1;
                let mut best = None;
                while low <= high {
                    let keep = low + (high - low) / 2;
                    let Some(partial) = truncate_read_item(last, keep) else {
                        break;
                    };
                    let mut candidate = result.output.clone();
                    if let Some(candidate_items) =
                        candidate.get_mut("items").and_then(Value::as_array_mut)
                    {
                        let last_index = candidate_items.len() - 1;
                        candidate_items[last_index] = partial;
                    }
                    mark_final_hard_cap_truncation(&mut candidate, index);
                    if final_model_result_len(&candidate) <= MAX_SERIALIZED_OUTPUT_BYTES {
                        best = Some(candidate);
                        low = keep.saturating_add(1);
                    } else {
                        high = keep.saturating_sub(1);
                    }
                }
                if let Some(best) = best {
                    result.output = best;
                    return;
                }
            }
        }

        let removed_index = {
            let Some(items) = result.output.get_mut("items").and_then(Value::as_array_mut) else {
                return;
            };
            items
                .pop()
                .and_then(|removed| removed["index"].as_u64())
                .unwrap_or(index as u64) as usize
        };
        mark_final_hard_cap_truncation(&mut result.output, removed_index);
        if final_model_result_len(&result.output) <= MAX_SERIALIZED_OUTPUT_BYTES {
            return;
        }
    }
}

impl ToolRuntime {
    pub(crate) async fn read_files(
        &self,
        project: String,
        items: Vec<ReadFilesItem>,
        with_line_numbers: Option<bool>,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(project) => project,
            Err(error) => return ToolResult::err(error),
        };
        self.read_files_resolved(&resolved, items, with_line_numbers)
            .await
    }

    pub(crate) async fn read_files_resolved(
        &self,
        resolved: &ResolvedProject,
        items: Vec<ReadFilesItem>,
        with_line_numbers: Option<bool>,
    ) -> ToolResult {
        if !(1..=MAX_READ_FILES_ITEMS).contains(&items.len())
            || items.iter().any(|item| item.path.trim().is_empty())
        {
            return ToolResult::err("read_files requires 1 to 8 items with non-empty paths");
        }

        let runtime_project_id = resolved.resolved_id.clone();
        let requested_count = items.len();
        let with_line_numbers = with_line_numbers.unwrap_or(false);
        let deadline = Instant::now() + self.read_files_deadline;

        // The concurrency slot covers validation, enqueue, and response wait.
        // No request can reach the Runner until its future is polled by
        // `buffer_unordered`, so at most four file reads are actually in flight.
        let mut completed: Vec<Value> =
            stream::iter(items.into_iter().enumerate().map(|(index, item)| {
                let project = &resolved.config;
                async move {
                    let path = item.path;
                    let result = self
                        .read_one_resolved_project_file(
                            project,
                            path.clone(),
                            item.start_line,
                            item.limit,
                            with_line_numbers,
                            deadline,
                        )
                        .await;
                    json!({
                        "index": index,
                        "path": path,
                        "success": result.success,
                        "output": result.output,
                        "error": result.error,
                    })
                }
            }))
            .buffer_unordered(MAX_READ_FILES_CONCURRENCY)
            .collect()
            .await;
        completed.sort_by_key(|item| item["index"].as_u64().unwrap_or(u64::MAX));

        ToolResult::ok(batch_output(
            &runtime_project_id,
            requested_count,
            completed,
            false,
            None,
            None,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranged_item(index: usize, start_line: usize, lines: &[String]) -> Value {
        let returned_lines = lines.len();
        json!({
            "index": index,
            "path": format!("src/{index}.rs"),
            "success": true,
            "output": {
                "text": lines.join("\n"),
                "format": "plain",
                "path": format!("src/{index}.rs"),
                "sha256": "c".repeat(64),
                "start_line": start_line,
                "limit": returned_lines,
                "total_lines": start_line + returned_lines - 1,
                "returned_lines": returned_lines,
                "end_line": start_line + returned_lines - 1,
                "has_more": false,
                "next_start_line": null
            },
            "error": null
        })
    }

    fn default_complete_item(index: usize, lines: &[String]) -> Value {
        let returned_lines = lines.len();
        let default_limit =
            webcodex_workspace::file_read_range::EffectiveRange::new(None, None).limit;
        json!({
            "index": index,
            "path": format!("src/{index}.rs"),
            "success": true,
            "output": {
                "text": lines.join("\n"),
                "format": "plain",
                "path": format!("src/{index}.rs"),
                "sha256": "d".repeat(64),
                "start_line": 1,
                "limit": default_limit,
                "total_lines": returned_lines,
                "returned_lines": returned_lines,
                "end_line": if returned_lines == 0 { Value::Null } else { json!(returned_lines) },
                "has_more": false,
                "next_start_line": null
            },
            "error": null
        })
    }

    #[test]
    fn complete_default_sparse_fit_is_not_preemptively_budget_truncated() {
        let payload_budget = DEFAULT_READ_FILES_RESULT_BYTES - MODEL_RESULT_ENVELOPE_RESERVE_BYTES;
        let mut selected = None;
        for text_bytes in 6_000..=9_000 {
            let completed = (0..8)
                .map(|index| default_complete_item(index, &["x".repeat(text_bytes)]))
                .collect::<Vec<_>>();
            let canonical = batch_output("agent:oe:demo", 8, completed.clone(), false, None, None);
            let canonical_bytes = serialized_batch_len(&canonical);
            let sparse_bytes = projected_batch_serialized_len(&canonical);
            if canonical_bytes > payload_budget && sparse_bytes <= payload_budget {
                selected = Some((completed, canonical_bytes, sparse_bytes));
                break;
            }
        }
        let (completed, canonical_bytes, sparse_bytes) =
            selected.expect("test fixture must straddle canonical/sparse budget boundary");
        assert!(canonical_bytes > payload_budget);
        assert!(sparse_bytes <= payload_budget);

        let mut result = ToolResult::ok(batch_output(
            "agent:oe:demo",
            8,
            completed.clone(),
            false,
            None,
            None,
        ));
        apply_model_facing_output_budget(&mut result, None);
        assert_eq!(result.output["output_truncated"], false);
        assert!(result.output["next_index"].is_null());
        assert_eq!(result.output["items"].as_array().unwrap().len(), 8);
        for (actual, expected) in result.output["items"]
            .as_array()
            .unwrap()
            .iter()
            .zip(completed.iter())
        {
            assert_eq!(actual["output"]["text"], expected["output"]["text"]);
        }

        super::super::dispatch::sparsify_complete_read_success("read_files", &mut result);
        assert!(result.output.get("output_truncated").is_none());
        assert!(result.output.get("next_index").is_none());
        assert_eq!(result.output["items"].as_array().unwrap().len(), 8);
    }

    #[test]
    fn output_budget_keeps_whole_items_and_points_at_first_omitted_index() {
        let item = |index, text: String| {
            json!({
                "index": index,
                "path": format!("src/{index}.rs"),
                "success": true,
                "output": {
                    "text": text,
                    "format": "plain",
                    "path": format!("src/{index}.rs"),
                    "sha256": "a".repeat(64),
                    "start_line": 1,
                    "limit": 1,
                    "total_lines": 1,
                    "returned_lines": 1,
                    "end_line": 1,
                    "has_more": false,
                    "next_start_line": null
                },
                "error": null
            })
        };
        let output = apply_output_budget(
            "agent:oe:demo",
            2,
            vec![
                item(0, "x".repeat(140 * 1024)),
                item(1, "y".repeat(140 * 1024)),
            ],
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
        );
        assert_eq!(output["returned_count"], 1);
        assert_eq!(output["output_truncated"], true);
        assert_eq!(output["next_index"], 1);
        assert_eq!(output["items"].as_array().unwrap().len(), 1);
        let serialized = serde_json::to_vec(&ToolResult::ok(output)).unwrap();
        assert!(serialized.len() <= MAX_SERIALIZED_OUTPUT_BYTES);
    }

    #[test]
    fn output_budget_reserves_space_for_outer_session_metadata() {
        let item = |index, text: String| {
            json!({
                "index": index,
                "path": format!("src/{index}.rs"),
                "success": true,
                "output": {
                    "text": text,
                    "format": "plain",
                    "path": format!("src/{index}.rs"),
                    "sha256": "b".repeat(64),
                    "start_line": 1,
                    "limit": 1,
                    "total_lines": 1,
                    "returned_lines": 1,
                    "end_line": 1,
                    "has_more": false,
                    "next_start_line": null
                },
                "error": null
            })
        };
        let output = apply_output_budget(
            "agent:oe:demo",
            3,
            vec![
                item(0, "x".repeat(120 * 1024)),
                item(1, "y".repeat(120 * 1024)),
                item(2, "z".repeat(120 * 1024)),
            ],
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
        );
        assert_eq!(output["returned_count"], 2);
        assert_eq!(output["next_index"], 2);

        let mut result = ToolResult::ok(output);
        result.output["session_recorded"] = json!(true);
        result.output["session_id"] = json!(format!("wc_sess_{}", "s".repeat(64)));
        result.output["session_event_id"] = json!(format!("evt_{}", "e".repeat(64)));
        result.output["session_hint"] = json!({
            "has_open_messages": true,
            "open_counts": {
                "guidance": u64::MAX,
                "question": u64::MAX,
                "todo": u64::MAX,
                "risk": u64::MAX
            },
            "highest_priority": "high",
            "suggested_next_tool": "session_discussion_summary"
        });
        assert!(serde_json::to_vec(&result).unwrap().len() <= MAX_SERIALIZED_OUTPUT_BYTES);
    }

    #[test]
    fn default_budget_partials_on_line_boundaries_and_explicit_large_returns_more() {
        let lines = (0..900)
            .map(|index| format!("第{index:04}行-{}", "界".repeat(40)))
            .collect::<Vec<_>>();
        let completed = vec![default_complete_item(0, &lines)];

        let default = apply_output_budget("agent:oe:demo", 1, completed.clone(), None);
        let large = apply_output_budget(
            "agent:oe:demo",
            1,
            completed,
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
        );
        let partial = &default["items"][0]["output"];
        let kept = partial["returned_lines"].as_u64().unwrap() as usize;
        assert!(kept > 0 && kept < lines.len());
        assert_eq!(default["next_index"], 0);
        assert_eq!(default["truncation_reason"], "batch_response_budget");
        assert_eq!(partial["budget_truncated"], true);
        assert_eq!(partial["next_start_line"], kept + 1);
        assert_eq!(partial["budget_next_limit"], lines.len() - kept);
        assert_eq!(partial["text"], lines[..kept].join("\n"));
        assert!(std::str::from_utf8(partial["text"].as_str().unwrap().as_bytes()).is_ok());
        assert!(
            serde_json::to_vec(&ToolResult::ok(default)).unwrap().len()
                <= DEFAULT_READ_FILES_RESULT_BYTES
        );
        assert_eq!(large["output_truncated"], false);
        assert_eq!(large["items"][0]["output"]["returned_lines"], lines.len());
    }

    #[test]
    fn read_budget_cursor_reconstructs_original_range_without_gaps() {
        let lines = (0..700)
            .map(|index| format!("line-{index:04}-{}", "x".repeat(90)))
            .collect::<Vec<_>>();
        let first = apply_output_budget("agent:oe:demo", 1, vec![ranged_item(0, 11, &lines)], None);
        let first_output = &first["items"][0]["output"];
        let kept = first_output["returned_lines"].as_u64().unwrap() as usize;
        let next_start = first_output["next_start_line"].as_u64().unwrap() as usize;
        let next_limit = first_output["budget_next_limit"].as_u64().unwrap() as usize;
        assert_eq!(next_start, 11 + kept);
        assert_eq!(next_limit, lines.len() - kept);

        let continuation = apply_output_budget(
            "agent:oe:demo",
            1,
            vec![ranged_item(0, next_start, &lines[kept..])],
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
        );
        let joined = format!(
            "{}\n{}",
            first_output["text"].as_str().unwrap(),
            continuation["items"][0]["output"]["text"].as_str().unwrap()
        );
        assert_eq!(joined, lines.join("\n"));
    }

    #[test]
    fn final_read_trim_preserves_existing_budget_tail_without_gaps() {
        let first_chunk = (0..60)
            .map(|index| format!("line-{index:03}"))
            .collect::<Vec<_>>();
        let mut item = ranged_item(0, 11, &first_chunk);
        item["output"]["limit"] = json!(100);
        item["output"]["total_lines"] = json!(110);
        item["output"]["has_more"] = json!(true);
        item["output"]["next_start_line"] = json!(71);
        item["output"]["budget_truncated"] = json!(true);
        item["output"]["budget_next_limit"] = json!(40);

        let partial = truncate_read_item(&item, 25).expect("second-stage partial read");
        let output = &partial["output"];
        assert_eq!(output["returned_lines"], 25);
        assert_eq!(output["end_line"], 35);
        assert_eq!(output["next_start_line"], 36);
        assert_eq!(output["budget_next_limit"], 75);
        assert_eq!(output["budget_truncated"], true);
    }

    #[test]
    fn result_budget_clamps_to_existing_hard_cap() {
        assert_eq!(
            normalized_result_budget(Some(MAX_SERIALIZED_OUTPUT_BYTES * 2)),
            MAX_SERIALIZED_OUTPUT_BYTES
        );
    }
}
