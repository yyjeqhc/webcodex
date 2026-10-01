//! One administrator-only query/read path shared by MCP and the hosted Console.
//! Queries use ActionAudit's existing index; payloads remain in the bounded
//! trace store. Missing Host identity is never reconstructed from a Project.
use super::{ToolCall, ToolResult, ToolRuntime};
use crate::auth::{AuthContext, SCOPE_ADMIN};
use crate::tool_request_trace::TraceReadError;
use serde_json::json;
#[cfg(test)]
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use webcodex_tool_contracts::ToolTraceQuery;

impl ToolRuntime {
    pub(crate) async fn read_tool_trace_diagnostic(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if !auth.is_some_and(|auth| auth.has_scope(SCOPE_ADMIN)) {
            return failure(
                "insufficient_scope",
                "Administrator diagnostic access required",
            );
        }
        let ToolCall::ReadToolTrace {
            trace_ref,
            query,
            offset,
            limit,
            payload_index,
        } = call
        else {
            return failure(
                "invalid_trace_request",
                "Expected read_tool_trace arguments",
            );
        };
        if (trace_ref.is_some() && query.is_some())
            || (trace_ref.is_none() && payload_index.is_some())
        {
            return failure(
                "invalid_trace_request",
                "query and trace_ref are exclusive; payload_index requires trace_ref",
            );
        }
        let database = self.window_activity_db.clone();
        let result = tokio::task::spawn_blocking(move || {
            if let Some(trace_ref) = trace_ref {
                return crate::tool_request_trace::read_trace(&trace_ref, offset, limit, payload_index);
            }
            let query = normalize_query(query.unwrap_or_default())?;
            let offset = offset.unwrap_or(0);
            let limit = limit.unwrap_or(20).clamp(1, 64);
            if offset > 10_000 { return Err(error("invalid_trace_request", "query offset exceeds 10000")); }
            let database = database.ok_or_else(|| error("trace_index_unavailable", "ActionAudit database unavailable"))?;
            let mut calls = database.query_tool_trace_calls(&webcodex_store::ToolTraceQueryFilter {
                window_key: query.window_key.as_deref(), project:query.project.as_deref(), tool_name:query.tool_name.as_deref(),
                since_ms:query.since_ms.expect("normalized"), until_ms:query.until_ms.expect("normalized"),
                include_nonmeaningful:query.include_nonmeaningful, offset, limit:limit + 1,
            }).map_err(|_| error("trace_index_unavailable", "ActionAudit diagnostic query failed"))?;
            let more = calls.len() > limit;
            calls.truncate(limit);
            let mut windows: BTreeMap<&str, (i64, i64, usize, BTreeSet<&str>)> = BTreeMap::new();
            for call in &calls {
                if let Some(key) = call.window_key.as_deref() {
                    let entry = windows.entry(key).or_insert((call.observed_at_ms,call.observed_at_ms,0,BTreeSet::new()));
                    entry.0 = entry.0.min(call.observed_at_ms);
                    entry.1 = entry.1.max(call.observed_at_ms);
                    entry.2 += 1;
                    if let Some(project) = call.project.as_deref() { entry.3.insert(project); }
                }
            }
            let windows = windows.into_iter().map(|(key,(first,last,count,projects))| json!({
                "window_key":key,"first_in_page_ms":first,"last_in_page_ms":last,"calls_in_page":count,"projects":projects
            })).collect::<Vec<_>>();
            Ok(json!({"status":"available","capture_mode":crate::tool_request_trace::capture_mode(),
                "calls":calls,"windows":windows,"window_summary_scope":"returned_calls_only",
                "query":query,"offset":offset,"returned_count":calls.len(),"next_offset":more.then_some(offset+calls.len()),
                "coverage":"retained_action_audit_only"}))
        }).await.map_err(|_| error("trace_store_unavailable", "Diagnostic worker failed")).and_then(|result| result);
        match result {
            Ok(value) => ToolResult::ok(value),
            Err(error) => failure(error.kind, &error.message),
        }
    }
}

fn normalize_query(mut query: ToolTraceQuery) -> Result<ToolTraceQuery, TraceReadError> {
    if query
        .window_key
        .as_deref()
        .is_some_and(|key| key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()))
        || query
            .project
            .as_deref()
            .is_some_and(|s| s.is_empty() || s.len() > 512)
        || query
            .tool_name
            .as_deref()
            .is_some_and(|s| s.is_empty() || s.len() > 128)
    {
        return Err(error(
            "invalid_trace_request",
            "Invalid exact diagnostic selector",
        ));
    }
    query.window_key = query.window_key.map(|s| s.to_ascii_lowercase());
    let until = query
        .until_ms
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let since = query
        .since_ms
        .unwrap_or_else(|| until.saturating_sub(86_400_000).max(0));
    if since < 0 || until < since || until - since > 31 * 86_400_000 {
        return Err(error(
            "invalid_trace_request",
            "Diagnostic range must be nonnegative, ordered, and at most 31 days",
        ));
    }
    query.since_ms = Some(since);
    query.until_ms = Some(until);
    Ok(query)
}
fn error(kind: &'static str, message: &str) -> TraceReadError {
    TraceReadError {
        kind,
        message: message.into(),
    }
}
fn failure(kind: &str, message: &str) -> ToolResult {
    ToolResult::err_with_output(
        message,
        json!({"error_kind":kind,"message":message,"state_changed":false}),
    )
}

#[cfg(test)]
mod tests;
