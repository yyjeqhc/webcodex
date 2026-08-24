//! Bounded independent text searches built from the canonical single-search core.

use super::files::{SearchOptions, SearchRequest};
use super::project_resolution::ResolvedProject;
use super::{SearchProjectTextsQuery, ToolResult, ToolRuntime};
use futures_util::{stream, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::Instant;
use webcodex_workspace::file_read_normalize::MODEL_RESULT_ENVELOPE_RESERVE_BYTES;
use webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES;

pub(crate) const MAX_SEARCH_PROJECT_TEXTS_QUERIES: usize = 8;
pub(crate) const MAX_SEARCH_PROJECT_TEXTS_CONCURRENCY: usize = 2;
pub(crate) const DEFAULT_SEARCH_PROJECT_TEXTS_DEADLINE: Duration = Duration::from_secs(30);
pub(crate) const DEFAULT_SEARCH_PROJECT_TEXTS_RESULT_BYTES: usize = 64 * 1024;
pub(crate) const MIN_SEARCH_PROJECT_TEXTS_RESULT_BYTES: usize = 8 * 1024;

impl From<SearchProjectTextsQuery> for SearchRequest {
    fn from(query: SearchProjectTextsQuery) -> Self {
        Self {
            pattern: query.pattern,
            path: query.path,
            limit: query.limit,
            context_before: query.context_before,
            context_after: query.context_after,
            include_globs: query.include_globs,
            exclude_globs: query.exclude_globs,
            result_mode: query.result_mode,
            timeout_secs: query.timeout_secs,
        }
    }
}

fn normalized_result_budget(max_result_bytes: Option<usize>) -> usize {
    max_result_bytes
        .unwrap_or(DEFAULT_SEARCH_PROJECT_TEXTS_RESULT_BYTES)
        .clamp(
            MIN_SEARCH_PROJECT_TEXTS_RESULT_BYTES,
            MAX_SERIALIZED_OUTPUT_BYTES,
        )
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

fn projected_batch_len(base_len: usize, item_bytes: usize, item_count: usize) -> usize {
    base_len
        .saturating_add(item_bytes)
        .saturating_add(item_count.saturating_sub(1))
}

fn apply_match_offset(mut item: Value, match_offset: usize) -> Value {
    if match_offset == 0 || item["success"].as_bool() != Some(true) {
        return item;
    }
    let Some(output) = item.get_mut("output").and_then(Value::as_object_mut) else {
        return item;
    };
    if output.get("result_mode").and_then(Value::as_str) != Some("matches") {
        return item;
    }
    let Some(matches) = output.get_mut("matches").and_then(Value::as_array_mut) else {
        return item;
    };
    let drain = match_offset.min(matches.len());
    matches.drain(..drain);
    let remaining = matches.len();
    output.insert("count".to_string(), json!(remaining));
    item
}

fn truncate_matches_item(item: &Value, keep_matches: usize) -> Option<Value> {
    if item["success"].as_bool() != Some(true) || keep_matches == 0 {
        return None;
    }
    let output = item.get("output")?.as_object()?;
    if output.get("result_mode")?.as_str()? != "matches" {
        return None;
    }
    let matches = output.get("matches")?.as_array()?;
    if keep_matches >= matches.len() || matches.len() <= 1 {
        return None;
    }

    let mut projected = item.clone();
    let projected_output = projected.get_mut("output")?.as_object_mut()?;
    projected_output.insert("matches".to_string(), json!(matches[..keep_matches]));
    projected_output.insert("count".to_string(), json!(keep_matches));
    projected_output.insert("budget_truncated".to_string(), json!(true));
    projected_output.insert("truncated".to_string(), json!(true));
    if projected_output
        .get("truncation_reason")
        .is_some_and(Value::is_null)
    {
        projected_output.insert(
            "truncation_reason".to_string(),
            json!("batch_response_budget"),
        );
    }
    Some(projected)
}

fn truncate_matches_item_to_fit(
    item: &Value,
    max_item_bytes: usize,
    match_offset: usize,
) -> Option<Value> {
    let match_count = item.get("output")?.get("matches")?.as_array()?.len();
    if match_count <= 1 {
        return None;
    }

    let mut low = 1usize;
    let mut high = match_count - 1;
    let mut best = None;
    while low <= high {
        let keep = low + (high - low) / 2;
        let Some(mut candidate) = truncate_matches_item(item, keep) else {
            break;
        };
        candidate["output"]["next_match_offset"] = json!(match_offset.saturating_add(keep));
        if serialized_value_len(&candidate) <= max_item_bytes {
            best = Some(candidate);
            low = keep.saturating_add(1);
        } else {
            high = keep.saturating_sub(1);
        }
    }
    best
}

fn retryable_agent_request_failure(result: &ToolResult) -> bool {
    !result.success
        && result.output.get("code").and_then(Value::as_str) == Some("search_request_dropped")
}

fn apply_output_budget(
    project: &str,
    requested_count: usize,
    completed: Vec<Value>,
    match_offsets: &[usize],
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
    if serialized_batch_len(&complete) <= payload_budget {
        return complete;
    }

    let truncation_reason = if result_budget == MAX_SERIALIZED_OUTPUT_BYTES {
        "hard_result_cap"
    } else {
        "batch_response_budget"
    };
    let base_len = serialized_batch_len(&batch_output(
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
        let item_len = serialized_value_len(&item);
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
        if let Some(partial) = truncate_matches_item_to_fit(
            &item,
            max_item_bytes,
            match_offsets.get(index).copied().unwrap_or(0),
        ) {
            returned.push(partial);
            // Continue this same query with its item-local next_match_offset.
            next_index = Some(index);
        } else {
            next_index = Some(index);
        }
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
    while serialized_batch_len(&output) > payload_budget {
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

fn failure_reason_code(result: &ToolResult) -> &'static str {
    match result.output.get("code").and_then(Value::as_str) {
        Some("invalid_search_request") => {
            match result.output.get("field").and_then(Value::as_str) {
                Some("pattern") => "invalid_pattern",
                Some("path") => "invalid_path",
                Some("include_globs" | "exclude_globs") => "invalid_glob",
                _ => "invalid_search_request",
            }
        }
        Some("search_timeout") => "timeout",
        Some("search_backend_feature_unavailable") => "search_backend_feature_unavailable",
        Some("search_execution_failed") => "search_execution_failed",
        Some("search_request_dropped") => "search_request_dropped",
        _ if result.output.get("format").and_then(Value::as_str)
            == Some("webcodex.external_provider_error.v1") =>
        {
            "external_provider_error"
        }
        _ => "agent_unavailable",
    }
}

fn batch_failure_stage(result: &ToolResult, broad_reason: &str) -> &'static str {
    match result.output.get("failure_stage").and_then(Value::as_str) {
        Some("request_validation") => "request_validation",
        Some("backend_selection") => "backend_selection",
        Some("backend_protocol") => "backend_protocol",
        Some("backend_execution") => "backend_execution",
        Some("agent_request") => "agent_request",
        Some("agent_execution") => "agent_execution",
        Some("agent_transport") => "agent_transport",
        Some("provider") => "provider",
        Some("local_execution") => "local_execution",
        Some("batch_deadline") => "batch_deadline",
        _ => match broad_reason {
            "invalid_pattern" | "invalid_path" | "invalid_glob" | "invalid_search_request" => {
                "request_validation"
            }
            "search_backend_feature_unavailable" => "backend_selection",
            "search_request_dropped" => "agent_transport",
            "external_provider_error" => "provider",
            "agent_unavailable" => "agent_request",
            "timeout" => "agent_transport",
            _ => "backend_execution",
        },
    }
}

fn batch_failure_detail_code(result: &ToolResult, broad_reason: &'static str) -> &'static str {
    match result.output.get("reason_code").and_then(Value::as_str) {
        Some("invalid_pattern") => "invalid_pattern",
        Some("invalid_path") => "invalid_path",
        Some("invalid_glob") => "invalid_glob",
        Some("invalid_search_request") => "invalid_search_request",
        Some("backend_feature_unavailable") => "backend_feature_unavailable",
        Some("backend_identity_missing") => "backend_identity_missing",
        Some("backend_identity_invalid") => "backend_identity_invalid",
        Some("backend_status_unavailable") => "backend_status_unavailable",
        Some("backend_output_inconsistent") => "backend_output_inconsistent",
        Some("backend_process_failed") => "backend_process_failed",
        Some("agent_request_failed") => "agent_request_failed",
        Some("agent_execution_failed") => "agent_execution_failed",
        Some("search_request_dropped") => "search_request_dropped",
        Some("timeout") => "timeout",
        Some("provider_execution_failed") => "provider_execution_failed",
        Some("provider_protocol_invalid") => "provider_protocol_invalid",
        Some("local_execution_failed") => "local_execution_failed",
        _ => broad_reason,
    }
}

fn copy_safe_batch_failure_metadata(source: &Value, target: &mut Value) {
    if let Some(backend @ ("rg" | "grep" | "native" | "claude_code")) =
        source.get("backend").and_then(Value::as_str)
    {
        target["backend"] = json!(backend);
    }
    if let Some(exit_code) = source.get("exit_code").and_then(Value::as_i64) {
        target["exit_code"] = json!(exit_code);
    }
    if let Some(result_mode @ ("matches" | "files_with_matches" | "count")) =
        source.get("result_mode").and_then(Value::as_str)
    {
        target["result_mode"] = json!(result_mode);
    }
    if let Some(timeout) = source
        .get("effective_timeout_secs")
        .and_then(Value::as_u64)
        .filter(|timeout| (1..=120).contains(timeout))
    {
        target["effective_timeout_secs"] = json!(timeout);
    }
    if let Some(provider_code) =
        source
            .get("provider_code")
            .and_then(Value::as_str)
            .filter(|code| {
                !code.is_empty()
                    && code.len() <= 64
                    && code.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    })
            })
    {
        target["provider_code"] = json!(provider_code);
    }
}

fn batch_item(index: usize, mut result: ToolResult) -> Value {
    if result.success {
        if let Some(output) = result.output.as_object_mut() {
            // Project identity and Session/permission metadata are outer-batch
            // concerns. The input index identifies the original pattern.
            for key in [
                "project",
                "pattern",
                "session_recorded",
                "session_id",
                "session_event_id",
                "session_hint",
                "permission",
            ] {
                output.remove(key);
            }
        }
        return json!({
            "index": index,
            "success": true,
            "output": result.output,
            "error": null,
        });
    }

    let reason_code = failure_reason_code(&result);
    let failure_stage = batch_failure_stage(&result, reason_code);
    let detail_code = batch_failure_detail_code(&result, reason_code);
    let mut output = json!({
        "error_kind": "search_project_text_failed",
        // Preserve the established broad batch reason while adding the
        // single-search provenance that explains where and why it failed.
        "reason_code": reason_code,
        "failure_stage": failure_stage,
        "detail_code": detail_code,
        "state_changed": false,
    });
    copy_safe_batch_failure_metadata(&result.output, &mut output);
    json!({
        "index": index,
        "success": false,
        "output": output,
        "error": format!("search_project_text failed: {reason_code}"),
    })
}

impl ToolRuntime {
    #[cfg(test)]
    pub(crate) async fn search_project_texts(
        &self,
        project: String,
        queries: Vec<SearchProjectTextsQuery>,
    ) -> ToolResult {
        self.search_project_texts_with_budget(project, queries, None)
            .await
    }

    pub(crate) async fn search_project_texts_with_budget(
        &self,
        project: String,
        queries: Vec<SearchProjectTextsQuery>,
        max_result_bytes: Option<usize>,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(project) => project,
            Err(error) => return error.into_tool_result(),
        };
        self.search_project_texts_resolved_with_budget(&resolved, queries, max_result_bytes)
            .await
    }

    pub(crate) async fn search_project_texts_resolved_with_budget(
        &self,
        resolved: &ResolvedProject,
        queries: Vec<SearchProjectTextsQuery>,
        max_result_bytes: Option<usize>,
    ) -> ToolResult {
        if !(1..=MAX_SEARCH_PROJECT_TEXTS_QUERIES).contains(&queries.len()) {
            return ToolResult::err("search_project_texts requires 1 to 8 queries");
        }

        let runtime_project_id = resolved.resolved_id.clone();
        let requested_count = queries.len();
        let deadline = Instant::now() + self.search_project_texts_deadline;
        let match_offsets = queries
            .iter()
            .map(|query| query.match_offset.unwrap_or(0))
            .collect::<Vec<_>>();

        // Validation, Runner enqueue, and response waiting all happen inside
        // the concurrency slot. A third query cannot enter the Runner queue
        // while two earlier queries still hold their slots.
        let mut completed: Vec<Value> =
            stream::iter(queries.into_iter().enumerate().map(|(index, query)| {
                let project = &resolved.config;
                let output_project = runtime_project_id.as_str();
                let match_offset = query.match_offset.unwrap_or(0);
                async move {
                    let offset_out_of_range = match_offset >= 200;
                    let offset_wrong_mode = match_offset > 0
                        && !matches!(
                            query.result_mode,
                            None | Some(super::SearchResultMode::Matches)
                        );
                    let result = if offset_out_of_range || offset_wrong_mode {
                        let reason = if offset_out_of_range {
                            "out_of_range"
                        } else {
                            "matches_mode_only"
                        };
                        ToolResult::err_with_output(
                            "invalid match_offset for search_project_texts",
                            json!({
                                "code": "invalid_search_request",
                                "failure_stage": "request_validation",
                                "reason_code": "invalid_search_request",
                                "field": "match_offset",
                                "reason": reason,
                            }),
                        )
                    } else {
                        match SearchOptions::normalize(query.into()) {
                            Ok(options) if project.is_agent() => {
                                let first = self
                                    .search_one_resolved_project_text(
                                        project,
                                        output_project,
                                        options.clone(),
                                        Some(deadline),
                                    )
                                    .await;
                                if retryable_agent_request_failure(&first)
                                    && Instant::now() < deadline
                                {
                                    self.search_one_resolved_project_text(
                                        project,
                                        output_project,
                                        options,
                                        Some(deadline),
                                    )
                                    .await
                                } else {
                                    first
                                }
                            }
                            Ok(options) => {
                                self.search_one_resolved_project_text(
                                    project,
                                    output_project,
                                    options,
                                    Some(deadline),
                                )
                                .await
                            }
                            Err(error) => error.into_tool_result(),
                        }
                    };
                    apply_match_offset(batch_item(index, result), match_offset)
                }
            }))
            .buffer_unordered(MAX_SEARCH_PROJECT_TEXTS_CONCURRENCY)
            .collect()
            .await;
        completed.sort_by_key(|item| item["index"].as_u64().unwrap_or(u64::MAX));

        ToolResult::ok(apply_output_budget(
            &runtime_project_id,
            requested_count,
            completed,
            &match_offsets,
            max_result_bytes,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches_item(index: usize, count: usize, preview_bytes: usize) -> Value {
        let matches = (0..count)
            .map(|match_index| {
                json!({
                    "path": format!("src/{index}-{match_index:03}.rs"),
                    "line": match_index + 1,
                    "preview": format!("m{match_index:03}-{}", "界".repeat(preview_bytes / 3)),
                    "context_before": [],
                    "context_after": []
                })
            })
            .collect::<Vec<_>>();
        json!({
            "index": index,
            "success": true,
            "output": {
                "backend": "rg",
                "result_mode": "matches",
                "count": count,
                "matches": matches,
                "truncated": false,
                "truncation_reason": null
            },
            "error": null
        })
    }

    #[test]
    fn output_budget_keeps_whole_items_and_reserves_outer_metadata_space() {
        let item = |index, text: String| {
            json!({
                "index": index,
                "success": true,
                "output": {
                    "backend": "rg",
                    "result_mode": "matches",
                    "count": 1,
                    "matches": [{
                        "path": format!("src/{index}.rs"),
                        "line": 1,
                        "preview": text,
                        "context_before": [],
                        "context_after": []
                    }],
                    "truncated": false,
                    "truncation_reason": null
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
            &[0, 0, 0],
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
        );
        assert_eq!(output["returned_count"], 2);
        assert_eq!(output["next_index"], 2);
        assert_eq!(output["output_truncated"], true);

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
    fn default_search_budget_partials_matches_and_explicit_large_returns_more() {
        let completed = vec![matches_item(0, 120, 900)];
        let default = apply_output_budget("agent:oe:demo", 1, completed.clone(), &[0], None);
        let large = apply_output_budget(
            "agent:oe:demo",
            1,
            completed,
            &[0],
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
        );
        let partial = &default["items"][0]["output"];
        let kept = partial["matches"].as_array().unwrap().len();
        assert!(kept > 0 && kept < 120);
        assert_eq!(default["next_index"], 0);
        assert_eq!(default["truncation_reason"], "batch_response_budget");
        assert_eq!(partial["budget_truncated"], true);
        assert_eq!(partial["next_match_offset"], kept);
        assert_eq!(partial["truncated"], true);
        assert_eq!(partial["truncation_reason"], "batch_response_budget");
        assert!(
            serde_json::to_vec(&ToolResult::ok(default)).unwrap().len()
                <= DEFAULT_SEARCH_PROJECT_TEXTS_RESULT_BYTES
        );
        assert_eq!(large["output_truncated"], false);
        assert_eq!(
            large["items"][0]["output"]["matches"]
                .as_array()
                .unwrap()
                .len(),
            120
        );
    }

    #[test]
    fn search_match_offset_continuation_has_no_duplicates_or_gaps() {
        let original = matches_item(0, 100, 1000);
        let first = apply_output_budget("agent:oe:demo", 1, vec![original.clone()], &[0], None);
        let first_matches = first["items"][0]["output"]["matches"]
            .as_array()
            .unwrap()
            .clone();
        let offset = first["items"][0]["output"]["next_match_offset"]
            .as_u64()
            .unwrap() as usize;
        let remaining = apply_match_offset(original.clone(), offset);
        let second = apply_output_budget(
            "agent:oe:demo",
            1,
            vec![remaining],
            &[offset],
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
        );
        let mut joined = first_matches;
        joined.extend(
            second["items"][0]["output"]["matches"]
                .as_array()
                .unwrap()
                .iter()
                .cloned(),
        );
        assert_eq!(
            joined,
            original["output"]["matches"].as_array().unwrap().clone()
        );
    }

    #[test]
    fn budget_truncation_preserves_existing_producer_reason() {
        let mut original = matches_item(0, 100, 1000);
        original["output"]["truncated"] = json!(true);
        original["output"]["truncation_reason"] = json!("limit");
        let output = apply_output_budget("agent:oe:demo", 1, vec![original], &[0], None);
        assert_eq!(output["items"][0]["output"]["budget_truncated"], true);
        assert_eq!(output["items"][0]["output"]["truncation_reason"], "limit");
    }

    #[test]
    fn search_result_budget_clamps_to_existing_hard_cap() {
        assert_eq!(
            normalized_result_budget(Some(MAX_SERIALIZED_OUTPUT_BYTES * 2)),
            MAX_SERIALIZED_OUTPUT_BYTES
        );
    }
}
