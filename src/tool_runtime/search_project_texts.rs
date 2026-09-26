//! Bounded independent text searches built from the canonical single-search core.

use super::files::{SearchOptions, SearchRequest};
use super::project_resolution::ResolvedProject;
use super::{SearchProjectTextsQuery, SuggestedToolCall, ToolResult, ToolRuntime};
use crate::json_measurement::serialized_json_len;
use futures_util::{stream, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::Instant;
use webcodex_core::runtime_contract::MODEL_INSPECTION_MAX_RESULT_BYTES as MAX_SERIALIZED_OUTPUT_BYTES;
use webcodex_workspace::file_read_normalize::MODEL_RESULT_ENVELOPE_RESERVE_BYTES;

pub(crate) const MAX_SEARCH_PROJECT_TEXTS_QUERIES: usize = 8;
// Keep search fanout below the query cap: each rg process can independently
// consume CPU, filesystem bandwidth, and page cache, including while a heavy
// validation Job is running. Two is the cross-device baseline after current
// 2-vs-4 narrow/broad workload review; results still preserve input order.
pub(crate) const MAX_SEARCH_PROJECT_TEXTS_CONCURRENCY: usize = 2;
pub(crate) const DEFAULT_SEARCH_PROJECT_TEXTS_DEADLINE: Duration = Duration::from_secs(30);
pub(crate) use webcodex_core::runtime_contract::{
    DEFAULT_SEARCH_PROJECT_TEXTS_RESULT_BYTES, MIN_SEARCH_PROJECT_TEXTS_RESULT_BYTES,
};

fn search_request_and_pattern_mode(
    query: SearchProjectTextsQuery,
) -> (SearchRequest, Option<super::SearchPatternMode>) {
    let SearchProjectTextsQuery {
        pattern,
        pattern_mode,
        path,
        limit,
        context_before,
        context_after,
        include_globs,
        exclude_globs,
        result_mode,
        timeout_secs,
    } = query;
    (
        SearchRequest {
            pattern,
            path,
            limit,
            context_before,
            context_after,
            include_globs,
            exclude_globs,
            result_mode,
            timeout_secs,
        },
        pattern_mode,
    )
}

fn include_glob_diagnostic_query(
    query: &SearchProjectTextsQuery,
) -> Option<SearchProjectTextsQuery> {
    if query.include_globs.as_ref().is_none_or(Vec::is_empty) {
        return None;
    }
    let mut diagnostic = query.clone();
    diagnostic.include_globs = None;
    diagnostic.result_mode = Some(super::SearchResultMode::Matches);
    diagnostic.context_before = Some(0);
    diagnostic.context_after = Some(0);
    diagnostic.limit = Some(1);
    Some(diagnostic)
}

fn successful_search_is_complete_and_empty(result: &ToolResult) -> bool {
    if !result.success || result.output["truncated"].as_bool() != Some(false) {
        return false;
    }
    match result.output["result_mode"].as_str() {
        Some("matches") => result.output["matches"]
            .as_array()
            .is_some_and(Vec::is_empty),
        Some("files_with_matches") => result.output["files"].as_array().is_some_and(Vec::is_empty),
        Some("count") => {
            result.output["count_complete"].as_bool() == Some(true)
                && result.output["total_matches"].as_u64() == Some(0)
        }
        _ => false,
    }
}

fn successful_diagnostic_has_match(result: &ToolResult) -> bool {
    result.success
        && result.output["matches"]
            .as_array()
            .is_some_and(|matches| !matches.is_empty())
}

pub(crate) fn normalized_result_budget(max_result_bytes: Option<usize>) -> usize {
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
    });
    if let Some(next_index) = next_index {
        output["next_index"] = json!(next_index);
    }
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
    serialized_json_len(value).unwrap_or(usize::MAX)
}

fn projected_batch_serialized_len(output: &Value, default_timeouts: &[bool]) -> usize {
    let mut projected = ToolResult::ok(output.clone());
    super::dispatch::sparsify_search_batch_success_for_model(default_timeouts, &mut projected);
    serialized_json_len(&projected).unwrap_or(usize::MAX)
}

fn projected_batch_serialized_len_with_continuation(
    output: &Value,
    default_timeouts: &[bool],
    project: &str,
    original_queries: &[SearchProjectTextsQuery],
    session_id: Option<&str>,
    max_result_bytes: Option<usize>,
) -> usize {
    let mut projected = ToolResult::ok(output.clone());
    super::dispatch::sparsify_search_batch_success_for_model(default_timeouts, &mut projected);
    add_actionable_search_continuation(
        &mut projected,
        project,
        original_queries,
        session_id,
        max_result_bytes,
    );
    serialized_json_len(&projected).unwrap_or(usize::MAX)
}

fn projected_search_item_len(item: &Value, default_timeout: bool) -> usize {
    let mut projected = item.clone();
    if projected["success"].as_bool() == Some(true) {
        if let Some(output) = projected.get_mut("output").and_then(Value::as_object_mut) {
            super::dispatch::sparsify_search_output_for_model(output, default_timeout, true);
        }
    }
    serialized_value_len(&projected)
}

fn projected_batch_len(base_len: usize, item_bytes: usize, item_count: usize) -> usize {
    base_len
        .saturating_add(item_bytes)
        .saturating_add(item_count.saturating_sub(1))
}

fn retryable_runner_request_failure(result: &ToolResult) -> bool {
    !result.success
        && result.output.get("code").and_then(Value::as_str) == Some("search_request_dropped")
}

fn apply_output_budget(
    project: &str,
    requested_count: usize,
    completed: Vec<Value>,
    default_timeouts: &[bool],
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
    // First evaluate the exact sparse model projection. Canonical search
    // metadata stays intact unless that final applicable projection itself
    // exceeds the response budget.
    if projected_batch_serialized_len(&complete, default_timeouts) <= payload_budget {
        return complete;
    }

    let truncation_reason = if result_budget == MAX_SERIALIZED_OUTPUT_BYTES {
        "hard_result_cap"
    } else {
        "batch_response_budget"
    };
    let base_len = projected_batch_serialized_len(
        &batch_output(
            project,
            requested_count,
            Vec::new(),
            true,
            Some(0),
            Some(truncation_reason),
        ),
        default_timeouts,
    );
    let mut returned = Vec::with_capacity(completed.len());
    let mut returned_item_bytes = 0usize;
    let mut next_index = None;

    for item in completed {
        let index = item["index"].as_u64().unwrap_or(returned.len() as u64) as usize;
        let item_len =
            projected_search_item_len(&item, default_timeouts.get(index).copied().unwrap_or(false));
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

        // Search batch continuation is query-granular. Backend match order is
        // intentionally unstable, so exposing a partial query and resuming by
        // match position would permit duplicates and gaps across reruns.
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
    while projected_batch_serialized_len(&output, default_timeouts) > payload_budget {
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
        Some("search_path_not_found") => "not_found",
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
        Some("path_resolution") => "path_resolution",
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
            "not_found" => "path_resolution",
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
        Some("not_found") => "not_found",
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

fn omitted_query_summary(item: &Value) -> Option<Value> {
    let index = item.get("index")?.as_u64()?;
    let success = item.get("success")?.as_bool()?;
    let output = item.get("output")?.as_object()?;
    if !success {
        return Some(json!({
            "index": index,
            "success": false,
            "reason_code": output.get("reason_code")?.as_str()?,
            "failure_stage": output.get("failure_stage")?.as_str()?,
            "detail_code": output.get("detail_code")?.as_str()?,
        }));
    }

    let result_mode = output.get("result_mode")?.as_str()?;
    let truncated = output.get("truncated")?.as_bool()?;
    let mut summary = json!({
        "index": index,
        "success": true,
        "result_mode": result_mode,
        "truncated": truncated,
    });
    match result_mode {
        "matches" => {
            if let Some(count) = output.get("count").and_then(Value::as_u64).or_else(|| {
                output
                    .get("matches")
                    .and_then(Value::as_array)
                    .map(|matches| matches.len() as u64)
            }) {
                summary["returned_match_count"] = json!(count);
            }
        }
        "files_with_matches" => {
            if let Some(count) = output
                .get("returned_file_count")
                .and_then(Value::as_u64)
                .or_else(|| {
                    output
                        .get("files")
                        .and_then(Value::as_array)
                        .map(|files| files.len() as u64)
                })
            {
                summary["returned_file_count"] = json!(count);
            }
        }
        "count" => {
            if output.get("count_complete").and_then(Value::as_bool) == Some(true) {
                if let Some(total_matches) = output.get("total_matches").and_then(Value::as_u64) {
                    summary["total_matches"] = json!(total_matches);
                }
            }
        }
        _ => return None,
    }
    Some(summary)
}

fn remaining_query_summaries(completed: &[Value], next_index: usize) -> Vec<Value> {
    completed
        .iter()
        .filter(|item| {
            item.get("index")
                .and_then(Value::as_u64)
                .is_some_and(|index| index as usize >= next_index)
        })
        .filter_map(omitted_query_summary)
        .collect()
}

fn sort_dedup_remaining_summaries(summaries: &mut Vec<Value>) {
    summaries.sort_by_key(|summary| summary["index"].as_u64().unwrap_or(u64::MAX));
    summaries.dedup_by_key(|summary| summary["index"].as_u64().unwrap_or(u64::MAX));
}

fn attach_bounded_remaining_summaries(
    output: &mut Value,
    summaries: &[Value],
    max_len: usize,
    measure: impl Fn(&Value) -> usize,
) {
    if summaries.is_empty() {
        return;
    }
    if let Some(root) = output.as_object_mut() {
        root.remove("remaining_summaries");
    }
    let mut accepted = Vec::with_capacity(summaries.len());
    for summary in summaries {
        accepted.push(summary.clone());
        let mut candidate = output.clone();
        candidate["remaining_summaries"] = json!(accepted);
        if measure(&candidate) <= max_len {
            *output = candidate;
        } else {
            break;
        }
    }
}

fn search_query_argument_value(query: &SearchProjectTextsQuery) -> Value {
    let mut value =
        serde_json::to_value(query).expect("SearchProjectTextsQuery serialization is infallible");
    if let Some(object) = value.as_object_mut() {
        object.retain(|_, value| !value.is_null());
    }
    value
}

fn search_suggested_arguments(
    project: &str,
    queries: &[SearchProjectTextsQuery],
    session_id: Option<&str>,
    max_result_bytes: Option<usize>,
) -> Value {
    let mut arguments = json!({
        "project": project,
        "queries": queries.iter().map(search_query_argument_value).collect::<Vec<_>>(),
    });
    if let Some(session_id) = session_id {
        arguments["session_id"] = json!(session_id);
    }
    if let Some(max_result_bytes) = max_result_bytes {
        arguments["max_result_bytes"] = json!(max_result_bytes);
    }
    arguments
}

/// Compile producer-only whole-query next_index bookkeeping into one directly
/// reusable suffix rerun. Individual query match positions remain deliberately
/// non-resumable because backend order is not a stable cursor.
pub(crate) fn add_actionable_search_continuation(
    result: &mut ToolResult,
    project: &str,
    original_queries: &[SearchProjectTextsQuery],
    session_id: Option<&str>,
    max_result_bytes: Option<usize>,
) {
    if !result.success {
        return;
    }
    let Some(output) = result.output.as_object_mut() else {
        return;
    };
    let truncated = output.get("output_truncated").and_then(Value::as_bool) == Some(true);
    let next_index = output
        .get("next_index")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    if !truncated {
        output.remove("next_index");
        return;
    }
    let Some(next_index) = next_index else {
        return;
    };
    let returned_count = output
        .get("returned_count")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let mut next_budget = max_result_bytes;
    if returned_count == 0 && next_index == 0 {
        let reason = output.get("truncation_reason").and_then(Value::as_str);
        let current_budget = normalized_result_budget(max_result_bytes);
        if reason == Some("hard_result_cap") || current_budget >= MAX_SERIALIZED_OUTPUT_BYTES {
            // No bounded parameter change can prove progress. Preserve the
            // truncation reason, but never manufacture a looping next call.
            output.remove("next_index");
            return;
        }
        next_budget = Some(MAX_SERIALIZED_OUTPUT_BYTES);
    }
    if let Some(remaining) = original_queries
        .get(next_index..)
        .filter(|queries| !queries.is_empty())
    {
        output.insert(
            "suggested_call".to_string(),
            SuggestedToolCall::new(
                "search_project_texts",
                search_suggested_arguments(project, remaining, session_id, next_budget),
            )
            .to_value(),
        );
    }
    output.remove("next_index");
}

pub(crate) fn apply_model_facing_output_budget(
    result: &mut ToolResult,
    default_timeouts: &[bool],
    max_result_bytes: Option<usize>,
    project: &str,
    original_queries: &[SearchProjectTextsQuery],
    session_id: Option<&str>,
) {
    if !result.success {
        return;
    }
    let Some(output) = result.output.as_object() else {
        return;
    };
    let Some(output_project) = output
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

    let mut budgeted = apply_output_budget(
        &output_project,
        requested_count,
        completed.clone(),
        default_timeouts,
        max_result_bytes,
    );

    // The parser-ready suffix call is part of the primary model-facing search
    // projection. Measure it before the optional omitted-query summaries. The
    // summaries describe already-completed suffix queries but never consume the
    // canonical next_index; the exact follow-up still begins at that same query.
    let payload_budget = normalized_result_budget(max_result_bytes)
        .saturating_sub(MODEL_RESULT_ENVELOPE_RESERVE_BYTES);
    let next_index = budgeted
        .get("next_index")
        .and_then(Value::as_u64)
        .map(|index| index as usize);
    let summaries = next_index
        .map(|index| remaining_query_summaries(&completed, index))
        .unwrap_or_default();
    let continuation_fits = projected_batch_serialized_len_with_continuation(
        &budgeted,
        default_timeouts,
        project,
        original_queries,
        session_id,
        max_result_bytes,
    ) <= payload_budget;
    if continuation_fits {
        attach_bounded_remaining_summaries(
            &mut budgeted,
            &summaries,
            payload_budget,
            |candidate| {
                projected_batch_serialized_len_with_continuation(
                    candidate,
                    default_timeouts,
                    project,
                    original_queries,
                    session_id,
                    max_result_bytes,
                )
            },
        );
    } else {
        if let Some(root) = budgeted.as_object_mut() {
            root.remove("next_index");
        }
        attach_bounded_remaining_summaries(
            &mut budgeted,
            &summaries,
            payload_budget,
            |candidate| projected_batch_serialized_len(candidate, default_timeouts),
        );
    }

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
        "remaining_summaries",
    ] {
        root.remove(key);
    }
    if let Some(budgeted) = budgeted.as_object() {
        for (key, value) in budgeted {
            root.insert(key.clone(), value.clone());
        }
    }
}

fn final_model_result_len(
    output: &Value,
    default_timeouts: &[bool],
    project: &str,
    original_queries: &[SearchProjectTextsQuery],
    session_id: Option<&str>,
    max_result_bytes: Option<usize>,
) -> usize {
    projected_batch_serialized_len_with_continuation(
        output,
        default_timeouts,
        project,
        original_queries,
        session_id,
        max_result_bytes,
    )
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

/// Enforce the explicit 512 KiB model-inspection ceiling against the actual final
/// serialized ToolResult, including Session/continuity overlays. Search
/// continuation remains query-granular: whole query items are removed from the
/// end until the fully decorated result fits, and next_index points at the first
/// omitted query.
pub(crate) fn enforce_final_model_facing_hard_cap(
    result: &mut ToolResult,
    default_timeouts: &[bool],
    project: &str,
    original_queries: &[SearchProjectTextsQuery],
    session_id: Option<&str>,
    max_result_bytes: Option<usize>,
) {
    if !result.success
        || final_model_result_len(
            &result.output,
            default_timeouts,
            project,
            original_queries,
            session_id,
            max_result_bytes,
        ) <= MAX_SERIALIZED_OUTPUT_BYTES
    {
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

    let mut summary_candidates = result
        .output
        .as_object_mut()
        .and_then(|root| root.remove("remaining_summaries"))
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    if final_model_result_len(
        &result.output,
        default_timeouts,
        project,
        original_queries,
        session_id,
        max_result_bytes,
    ) <= MAX_SERIALIZED_OUTPUT_BYTES
    {
        attach_bounded_remaining_summaries(
            &mut result.output,
            &summary_candidates,
            MAX_SERIALIZED_OUTPUT_BYTES,
            |candidate| {
                final_model_result_len(
                    candidate,
                    default_timeouts,
                    project,
                    original_queries,
                    session_id,
                    max_result_bytes,
                )
            },
        );
        return;
    }

    loop {
        let removed = {
            let Some(items) = result.output.get_mut("items").and_then(Value::as_array_mut) else {
                return;
            };
            let Some(removed) = items.pop() else {
                // No business item remains to trim. If the exact parser-ready
                // suffix call itself cannot fit the hard model ceiling, expose
                // truthful truncation without a fake/raw continuation cursor.
                if let Some(root) = result.output.as_object_mut() {
                    root.remove("next_index");
                }
                attach_bounded_remaining_summaries(
                    &mut result.output,
                    &summary_candidates,
                    MAX_SERIALIZED_OUTPUT_BYTES,
                    |candidate| {
                        final_model_result_len(
                            candidate,
                            default_timeouts,
                            project,
                            original_queries,
                            session_id,
                            max_result_bytes,
                        )
                    },
                );
                return;
            };
            removed
        };
        let removed_index = removed["index"].as_u64().unwrap_or(0) as usize;
        if let Some(summary) = omitted_query_summary(&removed) {
            summary_candidates.push(summary);
            sort_dedup_remaining_summaries(&mut summary_candidates);
        }
        mark_final_hard_cap_truncation(&mut result.output, removed_index);
        if final_model_result_len(
            &result.output,
            default_timeouts,
            project,
            original_queries,
            session_id,
            max_result_bytes,
        ) <= MAX_SERIALIZED_OUTPUT_BYTES
        {
            attach_bounded_remaining_summaries(
                &mut result.output,
                &summary_candidates,
                MAX_SERIALIZED_OUTPUT_BYTES,
                |candidate| {
                    final_model_result_len(
                        candidate,
                        default_timeouts,
                        project,
                        original_queries,
                        session_id,
                        max_result_bytes,
                    )
                },
            );
            return;
        }
    }
}

impl ToolRuntime {
    pub(crate) async fn search_project_texts(
        &self,
        project: String,
        queries: Vec<SearchProjectTextsQuery>,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(project) => project,
            Err(error) => return error.into_tool_result(),
        };
        self.search_project_texts_resolved(&resolved, queries).await
    }

    pub(crate) async fn search_project_texts_resolved(
        &self,
        resolved: &ResolvedProject,
        queries: Vec<SearchProjectTextsQuery>,
    ) -> ToolResult {
        if !(1..=MAX_SEARCH_PROJECT_TEXTS_QUERIES).contains(&queries.len()) {
            return ToolResult::err("search_project_texts requires 1 to 8 queries");
        }

        let runtime_project_id = resolved.resolved_id.clone();
        let requested_count = queries.len();
        let deadline = Instant::now() + self.search_project_texts_deadline;
        // One absolute Server-owned latency deadline covers the whole batch.
        // Queueing, retries, and include-glob diagnostics all consume this same
        // deadline; each query's execution ceiling is further clamped to the
        // remaining batch budget and the deadline is never reset.
        // Validation, Runner enqueue, and response waiting all happen inside
        // the concurrency slot. A third query cannot enter the Runner queue
        // while two earlier queries still hold their slots.
        let mut completed: Vec<Value> =
            stream::iter(queries.into_iter().enumerate().map(|(index, query)| {
                let project = &resolved.config;
                let output_project = runtime_project_id.as_str();
                async move {
                    let diagnostic_query = include_glob_diagnostic_query(&query);
                    let (request, pattern_mode) = search_request_and_pattern_mode(query);
                    let mut result =
                        match SearchOptions::normalize_with_pattern_mode(request, pattern_mode) {
                            Ok(options) => {
                                let first = self
                                    .search_one_resolved_project_text(
                                        project,
                                        output_project,
                                        options.clone(),
                                        Some(deadline),
                                    )
                                    .await;
                                if retryable_runner_request_failure(&first)
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
                            Err(error) => error.into_tool_result(),
                        };
                    if successful_search_is_complete_and_empty(&result) {
                        if let Some(diagnostic_query) = diagnostic_query {
                            let (request, pattern_mode) =
                                search_request_and_pattern_mode(diagnostic_query);
                            if let Ok(options) =
                                SearchOptions::normalize_with_pattern_mode(request, pattern_mode)
                            {
                                let diagnostic = self
                                    .search_one_resolved_project_text(
                                        project,
                                        output_project,
                                        options,
                                        Some(deadline),
                                    )
                                    .await;
                                if successful_diagnostic_has_match(&diagnostic) {
                                    result.output["zero_match_hint"] =
                                        json!("include_globs_excluded_matches");
                                }
                            }
                        }
                    }
                    batch_item(index, result)
                }
            }))
            .buffer_unordered(MAX_SEARCH_PROJECT_TEXTS_CONCURRENCY)
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

    fn matches_item(index: usize, count: usize, preview_bytes: usize) -> Value {
        let matches = (0..count)
            .map(|match_index| {
                json!({
                    "path": format!("src/{index}-{match_index:03}.rs"),
                    "line": match_index + 1,
                    "preview": format!("m{match_index:03}-{}", "界".repeat(preview_bytes / 3)),
                    "context_before": [],
                    "context_after": [],
                    "read_hint": {
                        "path": format!("src/{index}-{match_index:03}.rs"),
                        "start_line": 1,
                        "limit": 80
                    }
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

    fn default_matches_item(index: usize, count: usize, preview_bytes: usize) -> Value {
        let mut item = matches_item(index, count, preview_bytes);
        let output = item["output"].as_object_mut().unwrap();
        output.insert("path".to_string(), json!("."));
        output.insert("effective_timeout_secs".to_string(), json!(30));
        output.insert("exit_code".to_string(), json!(0));
        output.insert("context_before".to_string(), json!(0));
        output.insert("context_after".to_string(), json!(0));
        item
    }

    fn test_query(
        index: usize,
        result_mode: Option<super::super::SearchResultMode>,
    ) -> SearchProjectTextsQuery {
        SearchProjectTextsQuery {
            pattern: format!("needle-{index}"),
            pattern_mode: None,
            path: None,
            limit: None,
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode,
            timeout_secs: None,
        }
    }

    fn files_item(index: usize, count: usize) -> Value {
        json!({
            "index": index,
            "success": true,
            "output": {
                "backend": "rg",
                "result_mode": "files_with_matches",
                "pattern_mode": "regex",
                "path": ".",
                "effective_timeout_secs": 30,
                "exit_code": 0,
                "context_before": 0,
                "context_after": 0,
                "files": (0..count).map(|file_index| json!({"path": format!("src/{index}-{file_index}.rs")})).collect::<Vec<_>>(),
                "returned_file_count": count,
                "truncated": false,
                "truncation_reason": null
            },
            "error": null
        })
    }

    fn count_item(index: usize, total_matches: usize) -> Value {
        json!({
            "index": index,
            "success": true,
            "output": {
                "backend": "rg",
                "result_mode": "count",
                "pattern_mode": "regex",
                "path": ".",
                "effective_timeout_secs": 30,
                "exit_code": 0,
                "context_before": 0,
                "context_after": 0,
                "files": [],
                "returned_file_count": 0,
                "returned_match_count": total_matches,
                "count_complete": true,
                "total_matches": total_matches,
                "truncated": false,
                "truncation_reason": null
            },
            "error": null
        })
    }

    #[test]
    fn batch_projection_preserves_incomplete_count_truth_after_path_filtering() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(10),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: Some(crate::tool_runtime::SearchResultMode::Count),
            timeout_secs: None,
        })
        .unwrap();
        let marker = "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n";
        let stdout = format!("{marker}/private/absolute/secret.rs\u{0}2\n");
        let single = crate::tool_runtime::files::search_project_text_output(
            "agent:special:demo",
            &options,
            &stdout,
            Some(0),
            "",
        );
        assert!(single.success, "{:?}", single.error);

        let item = batch_item(0, single);
        let mut batch = ToolResult::ok(batch_output(
            "agent:special:demo",
            1,
            vec![item],
            false,
            None,
            None,
        ));
        super::super::dispatch::sparsify_search_batch_success_for_model(&[true], &mut batch);
        let output = &batch.output["items"][0]["output"];
        assert_eq!(output["count_complete"], false);
        assert_eq!(output["total_matches"], Value::Null);
        assert_eq!(output["files"], json!([]));
        assert!(!serde_json::to_string(&batch)
            .unwrap()
            .contains("/private/absolute/secret.rs"));
    }

    #[test]
    fn complete_default_sparse_fit_is_not_preemptively_budget_truncated() {
        let payload_budget =
            DEFAULT_SEARCH_PROJECT_TEXTS_RESULT_BYTES - MODEL_RESULT_ENVELOPE_RESERVE_BYTES;
        let mut selected = None;
        for preview_bytes in 6_000..=9_000 {
            let completed = (0..8)
                .map(|index| default_matches_item(index, 1, preview_bytes))
                .collect::<Vec<_>>();
            let canonical = batch_output("agent:oe:demo", 8, completed.clone(), false, None, None);
            let canonical_bytes = serialized_batch_len(&canonical);
            let sparse_bytes = projected_batch_serialized_len(&canonical, &[true; 8]);
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
        let queries = (0..8)
            .map(|index| SearchProjectTextsQuery {
                pattern: format!("needle-{index}"),
                pattern_mode: None,
                path: None,
                limit: None,
                context_before: None,
                context_after: None,
                include_globs: None,
                exclude_globs: None,
                result_mode: None,
                timeout_secs: None,
            })
            .collect::<Vec<_>>();
        apply_model_facing_output_budget(
            &mut result,
            &[true; 8],
            None,
            "agent:oe:demo",
            &queries,
            None,
        );
        assert_eq!(result.output["output_truncated"], false);
        assert!(result.output["next_index"].is_null());
        assert_eq!(result.output["items"].as_array().unwrap().len(), 8);
        assert!(result.output.get("remaining_summaries").is_none());
        for (actual, expected) in result.output["items"]
            .as_array()
            .unwrap()
            .iter()
            .zip(completed.iter())
        {
            assert_eq!(actual["output"]["matches"], expected["output"]["matches"]);
        }

        super::super::dispatch::sparsify_search_batch_success_for_model(&[true; 8], &mut result);
        assert!(result.output.get("output_truncated").is_none());
        assert!(result.output.get("next_index").is_none());
        assert_eq!(result.output["items"].as_array().unwrap().len(), 8);
    }

    #[test]
    fn omitted_query_summaries_cover_result_modes_without_content() {
        let completed = vec![matches_item(0, 0, 12), files_item(1, 4), count_item(2, 17)];
        let summaries = remaining_query_summaries(&completed, 0);
        assert_eq!(summaries.len(), 3);
        assert_eq!(summaries[0]["result_mode"], "matches");
        assert_eq!(summaries[0]["returned_match_count"], 0);
        assert_eq!(summaries[0]["truncated"], false);
        assert_eq!(summaries[1]["result_mode"], "files_with_matches");
        assert_eq!(summaries[1]["returned_file_count"], 4);
        assert_eq!(summaries[2]["result_mode"], "count");
        assert_eq!(summaries[2]["total_matches"], 17);
        let rendered = serde_json::to_string(&summaries).unwrap();
        assert!(!rendered.contains("src/"));
        assert!(!rendered.contains("preview"));
        assert!(!rendered.contains("read_hint"));
    }

    #[test]
    fn omitted_query_failure_summary_uses_only_stable_classification() {
        let mut failed = ToolResult::err("RAW_BACKEND_BODY_NEVER_RETURN");
        failed.output = json!({
            "code": "search_timeout",
            "reason_code": "timeout",
            "failure_stage": "agent_transport",
            "stderr": "RAW_BACKEND_BODY_NEVER_RETURN",
            "path": "/private/never-return.rs"
        });
        let item = batch_item(5, failed);
        let summary = omitted_query_summary(&item).unwrap();
        assert_eq!(summary["index"], 5);
        assert_eq!(summary["success"], false);
        assert_eq!(summary["reason_code"], "timeout");
        assert_eq!(summary["failure_stage"], "agent_transport");
        assert_eq!(summary["detail_code"], "timeout");
        let rendered = serde_json::to_string(&summary).unwrap();
        assert!(!rendered.contains("RAW_BACKEND_BODY_NEVER_RETURN"));
        assert!(!rendered.contains("/private/"));
        assert!(!rendered.contains("stderr"));
    }

    #[test]
    fn batch_budget_summarizes_omitted_suffix_without_consuming_continuation() {
        let mut first = default_matches_item(0, 1, 8_000);
        first["output"]["pattern_mode"] = json!("regex");
        let completed = vec![
            first.clone(),
            default_matches_item(1, 120, 900),
            default_matches_item(2, 0, 16),
            files_item(3, 4),
        ];
        let queries = vec![
            test_query(0, Some(super::super::SearchResultMode::Matches)),
            test_query(1, Some(super::super::SearchResultMode::Matches)),
            test_query(2, Some(super::super::SearchResultMode::Matches)),
            test_query(3, Some(super::super::SearchResultMode::FilesWithMatches)),
        ];
        let mut result = ToolResult::ok(batch_output(
            "agent:oe:demo",
            4,
            completed,
            false,
            None,
            None,
        ));
        apply_model_facing_output_budget(
            &mut result,
            &[true; 4],
            None,
            "agent:oe:demo",
            &queries,
            None,
        );
        assert_eq!(result.output["returned_count"], 1);
        assert_eq!(result.output["items"][0], first);
        assert_eq!(result.output["next_index"], 1);
        let summaries = result.output["remaining_summaries"].as_array().unwrap();
        assert_eq!(
            summaries
                .iter()
                .map(|summary| summary["index"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(summaries[0]["returned_match_count"], 120);
        assert_eq!(summaries[1]["returned_match_count"], 0);
        assert_eq!(summaries[2]["returned_file_count"], 4);
        assert!(!serde_json::to_string(summaries).unwrap().contains("src/3-"));

        super::super::dispatch::sparsify_search_batch_success_for_model(&[true; 4], &mut result);
        add_actionable_search_continuation(&mut result, "agent:oe:demo", &queries, None, None);
        let suggested_queries = result.output["suggested_call"]["arguments"]["queries"]
            .as_array()
            .unwrap();
        assert_eq!(suggested_queries.len(), 3);
        assert_eq!(suggested_queries[0]["pattern"], "needle-1");
        assert_eq!(suggested_queries[1]["pattern"], "needle-2");
        assert_eq!(suggested_queries[2]["pattern"], "needle-3");
        assert!(result.output.get("next_index").is_none());
        assert!(serialized_json_len(&result).unwrap() <= DEFAULT_SEARCH_PROJECT_TEXTS_RESULT_BYTES);
    }

    #[test]
    fn continuation_budget_has_priority_over_remaining_summaries() {
        let queries = (0..4)
            .map(|index| test_query(index, None))
            .collect::<Vec<_>>();
        let completed = vec![
            default_matches_item(0, 1, 12_000),
            default_matches_item(1, 1, 12),
            default_matches_item(2, 0, 12),
            files_item(3, 4),
        ];
        let mut output = batch_output(
            "agent:oe:demo",
            4,
            vec![completed[0].clone()],
            true,
            Some(1),
            Some("batch_response_budget"),
        );
        let summaries = remaining_query_summaries(&completed, 1);
        let base_len = projected_batch_serialized_len_with_continuation(
            &output,
            &[true; 4],
            "agent:oe:demo",
            &queries,
            None,
            None,
        );
        let mut all = output.clone();
        all["remaining_summaries"] = json!(summaries);
        let all_len = projected_batch_serialized_len_with_continuation(
            &all,
            &[true; 4],
            "agent:oe:demo",
            &queries,
            None,
            None,
        );
        assert!(all_len > base_len);
        let budget = base_len + (all_len - base_len) / 2;
        attach_bounded_remaining_summaries(&mut output, &summaries, budget, |candidate| {
            projected_batch_serialized_len_with_continuation(
                candidate,
                &[true; 4],
                "agent:oe:demo",
                &queries,
                None,
                None,
            )
        });
        assert_eq!(output["next_index"], 1);
        assert!(
            output
                .get("remaining_summaries")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0)
                < summaries.len()
        );
        assert!(
            projected_batch_serialized_len_with_continuation(
                &output,
                &[true; 4],
                "agent:oe:demo",
                &queries,
                None,
                None,
            ) <= budget
        );
        let mut result = ToolResult::ok(output);
        super::super::dispatch::sparsify_search_batch_success_for_model(&[true; 4], &mut result);
        add_actionable_search_continuation(&mut result, "agent:oe:demo", &queries, None, None);
        assert_eq!(
            result.output["suggested_call"]["tool"],
            "search_project_texts"
        );
    }

    #[test]
    fn tiny_budget_summary_fallback_is_deterministic_and_bounded() {
        let query = test_query(0, Some(super::super::SearchResultMode::Matches));
        let canonical = batch_output(
            "agent:oe:demo",
            1,
            vec![default_matches_item(0, 120, 900)],
            false,
            None,
            None,
        );
        let project_once = || {
            let mut result = ToolResult::ok(canonical.clone());
            apply_model_facing_output_budget(
                &mut result,
                &[true],
                Some(0),
                "agent:oe:demo",
                std::slice::from_ref(&query),
                None,
            );
            super::super::dispatch::sparsify_search_batch_success_for_model(&[true], &mut result);
            add_actionable_search_continuation(
                &mut result,
                "agent:oe:demo",
                std::slice::from_ref(&query),
                None,
                Some(0),
            );
            result
        };
        let first = project_once();
        let second = project_once();
        assert_eq!(first.output, second.output);
        assert!(first.output["output_truncated"].as_bool().unwrap());
        assert!(serialized_json_len(&first).unwrap() <= normalized_result_budget(Some(0)));
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
                        "context_after": [],
                        "read_hint": {
                            "path": format!("src/{index}.rs"),
                            "start_line": 1,
                            "limit": 80
                        }
                    }],
                    "truncated": false,
                    "truncation_reason": null
                },
                "error": null
            })
        };
        let completed = vec![
            item(0, "x".repeat(120 * 1024)),
            item(1, "y".repeat(120 * 1024)),
            item(2, "z".repeat(120 * 1024)),
        ];
        let output = apply_output_budget(
            "agent:oe:demo",
            3,
            completed.clone(),
            &[false, false, false],
            Some(256 * 1024),
        );
        assert_eq!(output["returned_count"], 2);
        assert_eq!(output["next_index"], 2);
        assert_eq!(output["output_truncated"], true);

        let mut result = ToolResult::ok(output);
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

        let expanded = apply_output_budget(
            "agent:oe:demo",
            3,
            completed,
            &[false, false, false],
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
        );
        assert_eq!(expanded["returned_count"], 3);
        assert_eq!(expanded["output_truncated"], false);
    }

    #[test]
    fn single_query_soft_budget_omits_whole_item_then_larger_budget_returns_it() {
        let completed = vec![default_matches_item(0, 120, 900)];
        let default = apply_output_budget("agent:oe:demo", 1, completed.clone(), &[true], None);
        assert_eq!(default["returned_count"], 0);
        assert!(default["items"].as_array().unwrap().is_empty());
        assert_eq!(default["next_index"], 0);
        assert_eq!(default["output_truncated"], true);
        assert_eq!(default["truncation_reason"], "batch_response_budget");

        let large = apply_output_budget(
            "agent:oe:demo",
            1,
            completed,
            &[true],
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
        );
        assert_eq!(large["output_truncated"], false);
        assert!(large["next_index"].is_null());
        assert_eq!(large["returned_count"], 1);
        assert_eq!(
            large["items"][0]["output"]["matches"]
                .as_array()
                .unwrap()
                .len(),
            120
        );
    }

    #[test]
    fn whole_query_continuation_is_independent_of_backend_match_order() {
        let first_query = default_matches_item(0, 1, 100);
        let omitted_query = default_matches_item(1, 90, 900);
        let first = apply_output_budget(
            "agent:oe:demo",
            2,
            vec![first_query.clone(), omitted_query],
            &[true, true],
            None,
        );
        assert_eq!(first["returned_count"], 1);
        assert_eq!(first["items"][0], first_query);
        assert_eq!(first["next_index"], 1);
        assert_eq!(first["output_truncated"], true);

        // A suffix rerun is a fresh query execution. Deliberately reverse its
        // match order to prove continuation does not stitch by match position.
        let mut rerun = default_matches_item(0, 90, 900);
        rerun["output"]["matches"].as_array_mut().unwrap().reverse();
        let rerun_matches = rerun["output"]["matches"].clone();
        let continuation = apply_output_budget(
            "agent:oe:demo",
            1,
            vec![rerun],
            &[true],
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
        );
        assert_eq!(continuation["output_truncated"], false);
        assert_eq!(continuation["returned_count"], 1);
        assert_eq!(continuation["items"][0]["output"]["matches"], rerun_matches);
    }

    #[test]
    fn fitting_producer_truncation_remains_independent_of_batch_budget() {
        for reason in ["limit", "output_bytes"] {
            let mut original = matches_item(0, 2, 100);
            original["output"]["truncated"] = json!(true);
            original["output"]["truncation_reason"] = json!(reason);
            let expected = original.clone();
            let output = apply_output_budget("agent:oe:demo", 1, vec![original], &[false], None);
            assert_eq!(output["output_truncated"], false);
            assert!(output["next_index"].is_null());
            assert_eq!(output["items"][0], expected);
        }
    }

    #[test]
    fn actionable_search_continuation_is_parser_ready_and_hard_cap_fails_closed() {
        let query = SearchProjectTextsQuery {
            pattern: "needle".to_string(),
            pattern_mode: None,
            path: None,
            limit: Some(120),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        };
        let mut soft = ToolResult::ok(apply_output_budget(
            "agent:resolved:demo",
            1,
            vec![default_matches_item(0, 120, 900)],
            &[true],
            None,
        ));
        assert_eq!(soft.output["next_index"], 0);
        add_actionable_search_continuation(
            &mut soft,
            "agent:resolved:demo",
            std::slice::from_ref(&query),
            Some("wc_sess_demo"),
            None,
        );
        assert!(soft.output.get("next_index").is_none());
        let suggested = &soft.output["suggested_call"];
        assert_eq!(suggested["tool"], "search_project_texts");
        assert_eq!(suggested["arguments"]["project"], "agent:resolved:demo");
        assert_eq!(suggested["arguments"]["session_id"], "wc_sess_demo");
        assert_eq!(
            suggested["arguments"]["max_result_bytes"],
            MAX_SERIALIZED_OUTPUT_BYTES
        );
        assert_eq!(suggested["arguments"]["queries"][0]["pattern"], "needle");
        assert!(suggested["arguments"]["queries"][0]
            .get("pattern_mode")
            .is_none());
        crate::tool_runtime::ToolCall::from_tool_name(
            suggested["tool"].as_str().unwrap(),
            suggested["arguments"].clone(),
        )
        .expect("whole-query search follow-up must be parser-ready");

        let mut hard = ToolResult::ok(apply_output_budget(
            "agent:resolved:demo",
            1,
            vec![matches_item(0, 199, 3_000)],
            &[false],
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
        ));
        assert_eq!(hard.output["next_index"], 0);
        assert_eq!(hard.output["truncation_reason"], "hard_result_cap");
        add_actionable_search_continuation(
            &mut hard,
            "agent:resolved:demo",
            &[query],
            None,
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
        );
        assert!(hard.output.get("next_index").is_none());
        assert!(hard.output.get("suggested_call").is_none());
    }

    #[test]
    fn hard_cap_pressure_omits_oversized_query_without_partial_matches() {
        let output = apply_output_budget(
            "agent:oe:demo",
            1,
            vec![matches_item(0, 199, 3_000)],
            &[false],
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
        );
        assert_eq!(output["returned_count"], 0);
        assert!(output["items"].as_array().unwrap().is_empty());
        assert_eq!(output["next_index"], 0);
        assert_eq!(output["output_truncated"], true);
        assert_eq!(output["truncation_reason"], "hard_result_cap");
    }

    #[test]
    fn final_hard_cap_accounts_for_outer_session_overlay_bytes() {
        let queries = (0..3)
            .map(|index| SearchProjectTextsQuery {
                pattern: format!("needle-{index}"),
                pattern_mode: None,
                path: None,
                limit: None,
                context_before: None,
                context_after: None,
                include_globs: None,
                exclude_globs: None,
                result_mode: None,
                timeout_secs: None,
            })
            .collect::<Vec<_>>();
        let completed = (0..3)
            .map(|index| default_matches_item(index, 1, 120 * 1024))
            .collect::<Vec<_>>();
        let mut result = ToolResult::ok(batch_output(
            "agent:oe:demo",
            3,
            completed,
            false,
            None,
            None,
        ));
        result.output["context_projection"] = json!({
            "materials": ["o".repeat(220 * 1024)]
        });
        assert!(
            final_model_result_len(
                &result.output,
                &[false; 3],
                "agent:oe:demo",
                &queries,
                None,
                Some(MAX_SERIALIZED_OUTPUT_BYTES),
            ) > MAX_SERIALIZED_OUTPUT_BYTES
        );

        enforce_final_model_facing_hard_cap(
            &mut result,
            &[false; 3],
            "agent:oe:demo",
            &queries,
            None,
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
        );

        assert_eq!(result.output["output_truncated"], true);
        assert_eq!(result.output["truncation_reason"], "hard_result_cap");
        let returned_count = result.output["returned_count"].as_u64().unwrap();
        let next_index = result.output["next_index"].as_u64().unwrap();
        assert!(returned_count < 3);
        assert_eq!(next_index, returned_count);
        let summaries = result.output["remaining_summaries"].as_array().unwrap();
        assert!(!summaries.is_empty());
        assert_eq!(summaries[0]["index"].as_u64().unwrap(), next_index);
        assert!(summaries
            .iter()
            .all(|summary| summary["index"].as_u64().unwrap() >= next_index));
        assert_eq!(
            result.output["context_projection"]["materials"][0]
                .as_str()
                .unwrap()
                .len(),
            220 * 1024
        );
        assert!(
            final_model_result_len(
                &result.output,
                &[false; 3],
                "agent:oe:demo",
                &queries,
                None,
                Some(MAX_SERIALIZED_OUTPUT_BYTES),
            ) <= MAX_SERIALIZED_OUTPUT_BYTES
        );
    }

    #[test]
    fn oversized_parser_ready_suffix_is_suppressed_before_soft_budget_overflow() {
        let glob = format!("src/{}", "g".repeat(252));
        assert_eq!(glob.len(), 256);
        let queries = (0..8)
            .map(|index| SearchProjectTextsQuery {
                pattern: format!("needle-{index}"),
                pattern_mode: Some(crate::tool_runtime::SearchPatternMode::Literal),
                path: Some("src".to_string()),
                limit: Some(50),
                context_before: None,
                context_after: None,
                include_globs: Some(vec![glob.clone(); 32]),
                exclude_globs: Some(vec![glob.clone(); 32]),
                result_mode: None,
                timeout_secs: None,
            })
            .collect::<Vec<_>>();
        let completed = (0..8)
            .map(|index| default_matches_item(index, 1, 9_000))
            .collect::<Vec<_>>();
        let mut result = ToolResult::ok(batch_output(
            "agent:oe:demo",
            8,
            completed,
            false,
            None,
            None,
        ));

        apply_model_facing_output_budget(
            &mut result,
            &[true; 8],
            None,
            "agent:oe:demo",
            &queries,
            None,
        );

        assert_eq!(result.output["output_truncated"], true);
        assert!(result.output["returned_count"].as_u64().unwrap() > 0);
        assert!(
            result.output.get("next_index").is_none(),
            "producer-only cursor must not survive when its parser-ready call exceeds the model budget"
        );
        super::super::dispatch::sparsify_search_batch_success_for_model(&[true; 8], &mut result);
        add_actionable_search_continuation(&mut result, "agent:oe:demo", &queries, None, None);
        assert!(result.output.get("suggested_call").is_none());
        let bytes = serialized_json_len(&result).unwrap();
        assert!(
            bytes <= DEFAULT_SEARCH_PROJECT_TEXTS_RESULT_BYTES,
            "final search projection exceeded its soft result budget after continuation handling: {bytes}"
        );
    }

    #[test]
    fn search_result_budget_clamps_to_existing_hard_bounds() {
        assert_eq!(
            normalized_result_budget(Some(MIN_SEARCH_PROJECT_TEXTS_RESULT_BYTES / 2)),
            MIN_SEARCH_PROJECT_TEXTS_RESULT_BYTES
        );
        assert_eq!(
            normalized_result_budget(Some(MAX_SERIALIZED_OUTPUT_BYTES * 2)),
            MAX_SERIALIZED_OUTPUT_BYTES
        );
    }
}
