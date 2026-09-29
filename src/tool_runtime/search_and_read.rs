use super::{
    ReadFilesItem, ResolvedProject, SearchProjectTextsQuery, SearchResultMode, ToolResult,
    ToolRuntime,
};
use serde_json::{json, Value};

const DEFAULT_READ_CONTEXT: usize = 40;
const MAX_READ_CONTEXT: usize = 100;
const DEFAULT_MAX_READS: usize = 8;
const MAX_READS: usize = 8;

fn match_read_items(
    search_output: &Value,
    read_before: usize,
    read_after: usize,
    max_reads: usize,
) -> Vec<ReadFilesItem> {
    search_output
        .get("matches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|matched| {
            let path = matched.get("path")?.as_str()?;
            let line = usize::try_from(matched.get("line")?.as_u64()?).ok()?;
            let start_line = line.saturating_sub(read_before).max(1);
            let end_line = line.saturating_add(read_after);
            Some(ReadFilesItem {
                path: path.to_string(),
                start_line: Some(start_line),
                limit: Some(end_line.saturating_sub(start_line).saturating_add(1)),
                expected_read_revision: None,
            })
        })
        .take(max_reads)
        .collect()
}

fn sanitize_batch_search_and_collect_successes(batch: &mut Value) -> Vec<Value> {
    let mut successful = Vec::new();
    let Some(items) = batch.get_mut("items").and_then(Value::as_array_mut) else {
        return successful;
    };
    for item in items {
        if item.get("success").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let Some(output) = item.get_mut("output") else {
            continue;
        };
        if let Some(matches) = output.get_mut("matches").and_then(Value::as_array_mut) {
            for matched in matches {
                if let Some(object) = matched.as_object_mut() {
                    object.remove("read_hint");
                }
            }
        }
        successful.push(output.clone());
    }
    successful
}

/// Reuse the ordinary search projection only after match-derived read planning has consumed
/// canonical results. Query indexes, failures, truncation and nonempty source context stay intact.
pub(crate) fn compact_compound_search(batch: Value, default_timeouts: &[bool]) -> Value {
    let mut result = ToolResult::ok(batch);
    super::result_projection::sparsify_search_batch_success_for_model(
        default_timeouts,
        &mut result,
    );
    result.output
}

#[derive(Debug)]
struct MaterializedReadRange<'a> {
    path: &'a str,
    start_line: Option<u64>,
    end_line: Option<u64>,
    complete_file: bool,
}

fn materialized_read_ranges(reads: &Value) -> Vec<MaterializedReadRange<'_>> {
    reads
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            if item.get("success").and_then(Value::as_bool) != Some(true) {
                return None;
            }
            let path = item.get("path")?.as_str()?;
            let output = item.get("output")?.as_object()?;
            if output.get("text").and_then(Value::as_str).is_none() {
                return None;
            }
            let start_line = output.get("start_line").and_then(Value::as_u64);
            let end_line = output.get("end_line").and_then(Value::as_u64);
            let complete_file = start_line.is_none()
                && end_line.is_none()
                && output
                    .get("read_revision")
                    .and_then(Value::as_u64)
                    .is_some()
                && output.get("total_lines").and_then(Value::as_u64).is_some();
            Some(MaterializedReadRange {
                path,
                start_line,
                end_line,
                complete_file,
            })
        })
        .collect()
}

fn match_is_materialized(matched: &Value, materialized: &[MaterializedReadRange<'_>]) -> bool {
    let Some(path) = matched.get("path").and_then(Value::as_str) else {
        return false;
    };
    let Some(line) = matched.get("line").and_then(Value::as_u64) else {
        return false;
    };
    materialized.iter().any(|read| {
        read.path == path
            && (read.complete_file
                || read
                    .start_line
                    .zip(read.end_line)
                    .is_some_and(|(start, end)| (start..=end).contains(&line)))
    })
}

fn compact_materialized_matches(output: &mut Value, materialized: &[MaterializedReadRange<'_>]) {
    let Some(matches) = output.get_mut("matches").and_then(Value::as_array_mut) else {
        return;
    };
    for matched in matches {
        if !match_is_materialized(matched, materialized) {
            continue;
        }
        if let Some(object) = matched.as_object_mut() {
            for key in ["preview", "context_before", "context_after", "read_hint"] {
                object.remove(key);
            }
        }
    }
}

/// Once source text for a match is present in the returned read evidence, keep
/// only its navigation identity in the search phase. Unmaterialized matches and
/// per-query failures retain their ordinary search projection so the compound
/// call never trades away evidence merely to save model-facing bytes.
fn compact_materialized_search_evidence(search: &mut Value, reads: &Value) {
    let materialized = materialized_read_ranges(reads);
    if materialized.is_empty() {
        return;
    }
    if let Some(items) = search.get_mut("items").and_then(Value::as_array_mut) {
        for item in items {
            if item.get("success").and_then(Value::as_bool) != Some(true) {
                continue;
            }
            if let Some(output) = item.get_mut("output") {
                compact_materialized_matches(output, &materialized);
            }
        }
    } else {
        compact_materialized_matches(search, &materialized);
    }
}

fn fair_match_read_items(
    searches: &[Value],
    read_before: usize,
    read_after: usize,
    max_reads: usize,
) -> Vec<ReadFilesItem> {
    let per_query: Vec<Vec<ReadFilesItem>> = searches
        .iter()
        .map(|search| match_read_items(search, read_before, read_after, max_reads))
        .collect();
    let mut result = Vec::with_capacity(max_reads);
    let mut offset = 0;
    while result.len() < max_reads {
        let mut added = false;
        for items in &per_query {
            if let Some(item) = items.get(offset) {
                result.push(item.clone());
                added = true;
                if result.len() == max_reads {
                    break;
                }
            }
        }
        if !added {
            break;
        }
        offset += 1;
    }
    result
}

impl ToolRuntime {
    pub(crate) async fn search_and_read(
        &self,
        project: String,
        queries: Vec<SearchProjectTextsQuery>,
        session_id: Option<String>,
        read_before: Option<usize>,
        read_after: Option<usize>,
        max_reads: Option<usize>,
        with_line_numbers: Option<bool>,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(project) => project,
            Err(error) => return error.into_tool_result(),
        };
        self.search_and_read_resolved(
            &resolved,
            queries,
            session_id,
            read_before,
            read_after,
            max_reads,
            with_line_numbers,
        )
        .await
    }

    pub(crate) async fn search_and_read_resolved(
        &self,
        resolved: &ResolvedProject,
        mut queries: Vec<SearchProjectTextsQuery>,
        session_id: Option<String>,
        read_before: Option<usize>,
        read_after: Option<usize>,
        max_reads: Option<usize>,
        with_line_numbers: Option<bool>,
    ) -> ToolResult {
        let read_before = read_before
            .unwrap_or(DEFAULT_READ_CONTEXT)
            .min(MAX_READ_CONTEXT);
        let read_after = read_after
            .unwrap_or(DEFAULT_READ_CONTEXT)
            .min(MAX_READ_CONTEXT);
        let max_reads = max_reads.unwrap_or(DEFAULT_MAX_READS).clamp(1, MAX_READS);

        if queries.is_empty() || queries.len() > 8 {
            return ToolResult::err("search_and_read requires 1..8 queries");
        }
        for query in &mut queries {
            query.result_mode = Some(SearchResultMode::Matches);
            query.context_before = Some(0);
            query.context_after = Some(0);
            query.limit = Some(query.limit.unwrap_or(max_reads).min(max_reads));
        }

        let query_count = queries.len();
        let default_timeouts: Vec<bool> = queries
            .iter()
            .map(|query| {
                query
                    .timeout_secs
                    .unwrap_or(super::files::DEFAULT_SEARCH_TIMEOUT_SECS as i64)
                    == super::files::DEFAULT_SEARCH_TIMEOUT_SECS as i64
            })
            .collect();
        let search = self.search_project_texts_resolved(resolved, queries).await;
        if !search.success {
            return search;
        }
        let mut batch_search_output = search.output;
        let search_outputs = sanitize_batch_search_and_collect_successes(&mut batch_search_output);
        if search_outputs.is_empty() {
            return ToolResult::err_with_output(
                "search_and_read could not obtain a successful search result",
                json!({"project": resolved.resolved_id, "search": batch_search_output, "state_changed": false}),
            );
        }
        let items = fair_match_read_items(&search_outputs, read_before, read_after, max_reads);
        let projected_search = compact_compound_search(batch_search_output, &default_timeouts);
        let mut search_output = if query_count == 1 {
            projected_search["items"][0]["output"].clone()
        } else {
            // Keep query correspondence and failures; only redundant presentation metadata is removed.
            projected_search
        };
        if items.is_empty() {
            return ToolResult::ok(json!({
                "project": resolved.resolved_id,
                "search": search_output,
                "reads": [],
                "read_request_count": 0,
            }));
        }

        let requested_reads = items.len();
        // Keep original members for canonical byte-ceiling fallback, but return
        // each successful physical union only once. Continuations must follow
        // the actual output ranges, not the optimistic pre-execution plan.
        let (mut reads, output_items) = self
            .read_files_coalesced_resolved(resolved, items, with_line_numbers)
            .await;
        let coalesced_reads = output_items.len();
        let projection = super::read_files::ReadModelProjection::Batch {
            project: resolved.resolved_id.clone(),
            items: output_items,
            session_id,
            with_line_numbers,
            max_result_bytes: Some(super::read_files::DEFAULT_READ_FILES_RESULT_BYTES),
        };
        super::read_files::apply_model_facing_output_budget(
            &mut reads,
            Some(super::read_files::DEFAULT_READ_FILES_RESULT_BYTES),
            &projection,
        );
        super::read_files::enforce_final_model_facing_hard_cap(&mut reads, &projection);
        super::read_files::add_actionable_read_continuations(&projection, &mut reads);
        super::result_projection::sparsify_complete_read_success("read_files", &mut reads);
        // The outer Project already identifies both phases; never remove a distinct identity.
        if reads.output.get("project").and_then(Value::as_str)
            == Some(resolved.resolved_id.as_str())
        {
            if let Some(output) = reads.output.as_object_mut() {
                output.remove("project");
            }
        }
        compact_materialized_search_evidence(&mut search_output, &reads.output);
        ToolResult::ok(json!({
            "project": resolved.resolved_id,
            "search": search_output,
            "reads": reads.output,
            "read_request_count": requested_reads,
            "coalesced_read_count": coalesced_reads,
            "read_success": reads.success,
            "read_error": reads.error,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_call_schema_parses_search_and_read_and_rejects_unknown_fields() {
        let parsed = crate::tool_runtime::ToolCall::from_tool_name(
            "search_and_read",
            json!({
                "project": "demo",
                "query": {"pattern": "needle", "pattern_mode": "literal"},
                "read_before": 20,
                "read_after": 30,
                "max_reads": 4,
                "with_line_numbers": true
            }),
        )
        .unwrap();
        assert!(matches!(
            parsed,
            crate::tool_runtime::ToolCall::SearchAndRead {
                project,
                read_before: Some(20),
                read_after: Some(30),
                max_reads: Some(4),
                with_line_numbers: Some(true),
                ..
            } if project == "demo"
        ));
        assert!(crate::tool_runtime::ToolCall::from_tool_name(
            "search_and_read",
            json!({
                "project": "demo",
                "query": {"pattern": "needle"},
                "unexpected": true
            })
        )
        .is_err());
    }

    #[test]
    fn match_ranges_are_bounded_and_start_at_one() {
        let search = json!({
            "matches": [
                {"path": "src/a.rs", "line": 5},
                {"path": "src/a.rs", "line": 100}
            ]
        });
        let items = match_read_items(&search, 40, 40, 8);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].start_line, Some(1));
        assert_eq!(items[0].limit, Some(45));
        assert_eq!(items[1].start_line, Some(60));
        assert_eq!(items[1].limit, Some(81));
    }

    #[test]
    fn overlapping_match_ranges_are_coalesced_before_compound_read() {
        let search = json!({
            "matches": [
                {"path": "index.vue", "line": 26},
                {"path": "index.vue", "line": 43},
                {"path": "index.vue", "line": 60},
                {"path": "index.vue", "line": 77}
            ]
        });
        let items = match_read_items(&search, 20, 59, 8);
        assert_eq!(items.len(), 4);
        let merged = super::super::read_files::coalesce_read_files_items(items);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].start_line, Some(6));
        assert_eq!(merged[0].limit, Some(131));
    }

    #[test]
    fn batch_search_sanitization_preserves_indexes_and_failures() {
        let mut batch = json!({
            "project": "demo",
            "requested_count": 3,
            "returned_count": 3,
            "succeeded_count": 2,
            "failed_count": 1,
            "items": [
                {"index": 0, "success": true, "output": {"matches": [{"path":"a.rs","line":10,"read_hint":{"path":"a.rs","start_line":1,"limit":20}}]}, "error": null},
                {"index": 1, "success": false, "output": {"reason_code":"empty","failure_stage":"request_validation"}, "error": "search_project_text failed: empty"},
                {"index": 2, "success": true, "output": {"matches": [{"path":"b.rs","line":20,"read_hint":{"path":"b.rs","start_line":1,"limit":20}}]}, "error": null}
            ],
            "output_truncated": false
        });
        let successes = sanitize_batch_search_and_collect_successes(&mut batch);
        assert_eq!(successes.len(), 2);
        assert_eq!(batch["items"].as_array().unwrap().len(), 3);
        assert_eq!(batch["items"][0]["index"], 0);
        assert_eq!(batch["items"][1]["index"], 1);
        assert_eq!(batch["items"][1]["success"], false);
        assert_eq!(batch["items"][1]["output"]["reason_code"], "empty");
        assert_eq!(batch["items"][2]["index"], 2);
        assert!(batch["items"][0]["output"]["matches"][0]
            .get("read_hint")
            .is_none());
        assert!(batch["items"][2]["output"]["matches"][0]
            .get("read_hint")
            .is_none());
    }

    #[test]
    fn compound_projection_compacts_only_matches_covered_by_successful_reads() {
        let mut search = json!({
            "matches": [
                {
                    "path": "src/a.rs",
                    "line": 10,
                    "preview": "needle a",
                    "context_before": [],
                    "context_after": [],
                    "read_hint": {"start_line": 1, "limit": 80}
                },
                {
                    "path": "src/b.rs",
                    "line": 20,
                    "preview": "needle b",
                    "context_before": [],
                    "context_after": [],
                    "read_hint": {"start_line": 1, "limit": 80}
                }
            ]
        });
        let reads = json!({
            "items": [
                {
                    "path": "src/a.rs",
                    "success": true,
                    "output": {"text": "materialized", "start_line": 1, "end_line": 30},
                    "error": null
                },
                {
                    "path": "src/b.rs",
                    "success": false,
                    "output": {"reason_code": "not_found"},
                    "error": "read failed"
                }
            ]
        });
        let before = serde_json::to_vec(&search).unwrap().len();
        compact_materialized_search_evidence(&mut search, &reads);
        let after = serde_json::to_vec(&search).unwrap().len();

        assert_eq!(search["matches"][0]["path"], "src/a.rs");
        assert_eq!(search["matches"][0]["line"], 10);
        assert!(search["matches"][0].get("preview").is_none());
        assert!(search["matches"][0].get("read_hint").is_none());
        assert_eq!(search["matches"][1]["preview"], "needle b");
        assert!(search["matches"][1].get("read_hint").is_some());
        assert!(
            after < before,
            "materialized duplicate evidence should shrink"
        );
    }

    #[test]
    fn multi_query_schema_and_fair_global_budget() {
        let parsed = crate::tool_runtime::ToolCall::from_tool_name(
            "search_and_read",
            json!({"project":"demo","queries":[{"pattern":"one"},{"pattern":"two"}]}),
        )
        .unwrap();
        assert!(
            matches!(parsed, crate::tool_runtime::ToolCall::SearchAndRead { query: None, queries: Some(queries), .. } if queries.len() == 2)
        );
        let searches = vec![
            json!({"matches":[{"path":"a.rs","line":10},{"path":"a.rs","line":20},{"path":"a.rs","line":30}]}),
            json!({"matches":[{"path":"b.rs","line":10},{"path":"b.rs","line":20},{"path":"b.rs","line":30}]}),
        ];
        let items = fair_match_read_items(&searches, 1, 1, 4);
        assert_eq!(
            items
                .iter()
                .map(|item| item.path.as_str())
                .collect::<Vec<_>>(),
            vec!["a.rs", "b.rs", "a.rs", "b.rs"]
        );
    }
}
