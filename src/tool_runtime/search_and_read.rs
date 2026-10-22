use super::read_files::{ReadFilesOutput, RetainedReadRanges};
use super::search_project_texts::SearchBatch;
use super::{
    ReadFilesItem, ResolvedProject, SearchProjectTextsQuery, SearchResultMode, ToolResult,
    ToolRuntime,
};
use serde_json::{json, Value};

const DEFAULT_READ_CONTEXT: usize = 40;
const MAX_READ_CONTEXT: usize = 100;
const DEFAULT_MAX_READS: usize = 8;
const MAX_READS: usize = 8;

fn match_read_items<'a>(
    matches: impl IntoIterator<Item = (&'a str, u64)>,
    read_before: usize,
    read_after: usize,
    max_reads: usize,
) -> Vec<ReadFilesItem> {
    matches
        .into_iter()
        .filter_map(|(path, line)| {
            let line = usize::try_from(line).ok()?;
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

/// The mask comes from typed search identities and the final retained read
/// ranges. This function only shapes the public output; it never derives facts
/// from a sparse or budget-truncated JSON representation.
fn project_compound_matches(batch: &mut Value, materialized: &[Option<Vec<bool>>]) {
    let Some(items) = batch.get_mut("items").and_then(Value::as_array_mut) else {
        return;
    };
    for (item, materialized) in items.iter_mut().zip(materialized) {
        let Some(materialized) = materialized else {
            continue;
        };
        let Some(matches) = item
            .get_mut("output")
            .and_then(|output| output.get_mut("matches"))
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        for (matched, covered) in matches.iter_mut().zip(materialized) {
            if let Some(object) = matched.as_object_mut() {
                object.remove("read_hint");
                if *covered {
                    for key in ["preview", "context_before", "context_after"] {
                        object.remove(key);
                    }
                }
            }
        }
    }
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

fn project_compound_search(
    batch: SearchBatch,
    default_timeouts: &[bool],
    retained: &RetainedReadRanges,
) -> Value {
    let query_count = batch.items.len();
    let materialized = batch
        .items
        .iter()
        .map(|item| {
            item.result.as_ref().ok().map(|success| {
                success
                    .matches()
                    .unwrap_or_default()
                    .iter()
                    .map(|matched| retained.contains(&matched.path, matched.line))
                    .collect()
            })
        })
        .collect::<Vec<_>>();
    let mut output = batch.into_tool_result().output;
    project_compound_matches(&mut output, &materialized);
    let mut output = compact_compound_search(output, default_timeouts);
    if query_count == 1 {
        output["items"][0]["output"].take()
    } else {
        output
    }
}

fn fair_match_read_items(
    per_query: Vec<Vec<ReadFilesItem>>,
    max_reads: usize,
) -> Vec<ReadFilesItem> {
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
            return ToolResult::err("search_file_context requires 1..8 queries");
        }
        for query in &mut queries {
            query.result_mode = Some(SearchResultMode::Matches);
            query.context_before = Some(0);
            query.context_after = Some(0);
            query.limit = Some(query.limit.unwrap_or(max_reads).min(max_reads));
        }

        let default_timeouts: Vec<bool> = queries
            .iter()
            .map(|query| {
                query
                    .timeout_secs
                    .unwrap_or(super::files::DEFAULT_SEARCH_TIMEOUT_SECS as i64)
                    == super::files::DEFAULT_SEARCH_TIMEOUT_SECS as i64
            })
            .collect();
        let search = match self
            .search_project_texts_resolved_typed(resolved, queries)
            .await
        {
            Ok(search) => search,
            Err(error) => return error.into_tool_result(),
        };
        if !search.items.iter().any(|item| item.result.is_ok()) {
            return ToolResult::err_with_output(
                "search_file_context could not obtain a successful search result",
                json!({"project": resolved.resolved_id, "search": search.into_tool_result().output, "state_changed": false}),
            );
        }
        let per_query = search
            .items
            .iter()
            .filter_map(|item| {
                let matches = item.result.as_ref().ok()?.matches()?;
                Some(match_read_items(
                    matches
                        .iter()
                        .map(|matched| (matched.path.as_str(), matched.line)),
                    read_before,
                    read_after,
                    max_reads,
                ))
            })
            .collect();
        let items = fair_match_read_items(per_query, max_reads);
        if items.is_empty() {
            let search_output =
                project_compound_search(search, &default_timeouts, &RetainedReadRanges::default());
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
        let ReadFilesOutput {
            result: mut reads,
            items: output_items,
            mut retained,
        } = self
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
        retained.apply_retention(super::read_files::apply_model_facing_output_budget(
            &mut reads,
            Some(super::read_files::DEFAULT_READ_FILES_RESULT_BYTES),
            &projection,
        ));
        retained.apply_retention(super::read_files::enforce_final_model_facing_hard_cap(
            &mut reads,
            &projection,
        ));
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
        let search_output = project_compound_search(search, &default_timeouts, &retained);
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
            "search_file_context",
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
            "search_file_context",
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
        let search = [("src/a.rs", 5), ("src/a.rs", 100)];
        let items = match_read_items(search, 40, 40, 8);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].start_line, Some(1));
        assert_eq!(items[0].limit, Some(45));
        assert_eq!(items[1].start_line, Some(60));
        assert_eq!(items[1].limit, Some(81));
    }

    #[test]
    fn overlapping_match_ranges_are_coalesced_before_compound_read() {
        let search = [
            ("index.vue", 26),
            ("index.vue", 43),
            ("index.vue", 60),
            ("index.vue", 77),
        ];
        let items = match_read_items(search, 20, 59, 8);
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
        project_compound_matches(&mut batch, &[Some(vec![false]), None, Some(vec![false])]);
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
    fn compound_projection_compacts_only_matches_with_retained_text() {
        let mut search = json!({"items": [{"index": 0, "success": true, "output": {
            "matches": [
                {"path": "src/a.rs", "line": 10, "preview": "needle a", "context_before": [],
                 "context_after": [], "read_hint": {"start_line": 1, "limit": 80}},
                {"path": "src/b.rs", "line": 20, "preview": "needle b", "context_before": [],
                 "context_after": [], "read_hint": {"start_line": 1, "limit": 80}}
            ]
        }}]});
        let before = serde_json::to_vec(&search).unwrap().len();
        project_compound_matches(&mut search, &[Some(vec![true, false])]);
        let after = serde_json::to_vec(&search).unwrap().len();
        let matches = &search["items"][0]["output"]["matches"];
        assert_eq!(matches[0]["path"], "src/a.rs");
        assert_eq!(matches[0]["line"], 10);
        assert!(matches[0].get("preview").is_none());
        assert!(matches[0].get("read_hint").is_none());
        assert_eq!(matches[1]["preview"], "needle b");
        assert!(matches[1].get("read_hint").is_none());
        assert!(
            after < before,
            "materialized duplicate evidence should shrink"
        );
    }

    #[test]
    fn multi_query_schema_and_fair_global_budget() {
        let parsed = crate::tool_runtime::ToolCall::from_tool_name(
            "search_file_context",
            json!({"project":"demo","queries":[{"pattern":"one"},{"pattern":"two"}]}),
        )
        .unwrap();
        assert!(
            matches!(parsed, crate::tool_runtime::ToolCall::SearchAndRead { query: None, queries: Some(queries), .. } if queries.len() == 2)
        );
        let searches = vec![
            match_read_items([("a.rs", 10), ("a.rs", 20), ("a.rs", 30)], 1, 1, 4),
            match_read_items([("b.rs", 10), ("b.rs", 20), ("b.rs", 30)], 1, 1, 4),
        ];
        let items = fair_match_read_items(searches, 4);
        assert_eq!(
            items
                .iter()
                .map(|item| item.path.as_str())
                .collect::<Vec<_>>(),
            vec!["a.rs", "b.rs", "a.rs", "b.rs"]
        );
    }
}
