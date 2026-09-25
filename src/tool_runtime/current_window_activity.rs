//! Bounded current-Host-Window diagnostic observation over persisted ActionAudit.
use super::window_activity::{active_window_request_visible_cached, window_event_visible_cached};
use super::window_activity_projection::{
    project_visible_window_activity, project_window_loop_timings,
};
use super::{ToolResult, ToolRuntime};
use crate::auth::{AuthContext, SCOPE_RUNTIME_READ};
use crate::client_window::ClientWindow;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;

const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 200;
const SCAN_LIMIT: usize = 2_000;
const MAX_OUTPUT_BYTES: usize = 96 * 1024;
const MAX_ACTIVE_REQUESTS: usize = 8;

#[derive(Default, Serialize)]
struct ActivitySummary {
    events_scanned: usize,
    meaningful_call_count: usize,
    observe_jobs_count: usize,
    observe_jobs_ratio_denominator: usize,
    observe_jobs_ratio: Option<f64>,
    handler_returned_count: usize,
    missing_handoff_count: usize,
    overlapping_call_count: usize,
    serial_call_count: usize,
    observed_next_call_gap_count: usize,
    gaps_lt_1s: usize,
    gaps_lt_2s: usize,
    gaps_lt_5s: usize,
    gaps_ge_5s: usize,
    gaps_ge_10s: usize,
    gaps_ge_30s: usize,
    gaps_ge_120s: usize,
    total_service_ms: i64,
    total_positive_observed_next_call_gap_ms: i64,
    max_service_ms: Option<i64>,
    max_observed_next_call_gap_ms: Option<i64>,
    returned_nested_code_mode_child_count: usize,
}

impl ToolRuntime {
    pub(super) async fn current_window_activity(
        &self,
        window: Option<&ClientWindow>,
        auth: Option<&AuthContext>,
        limit: Option<usize>,
        include_nonmeaningful: bool,
    ) -> ToolResult {
        let Some(window) = window else {
            return ToolResult::ok(
                json!({"status": "unavailable", "reason_code": "window_identity_unavailable"}),
            );
        };
        let Some(auth) = auth.filter(|auth| !auth.is_open_anonymous()) else {
            return ToolResult::ok(
                json!({"status": "unavailable", "reason_code": "principal_identity_unavailable"}),
            );
        };
        if !auth.has_scope(SCOPE_RUNTIME_READ) {
            return ToolResult::ok(
                json!({"status": "unavailable", "reason_code": "runtime_read_unavailable"}),
            );
        }
        let Ok((principal_kind, principal_id)) =
            super::session_context::runtime_observation_principal(Some(auth))
        else {
            return ToolResult::ok(
                json!({"status": "unavailable", "reason_code": "principal_identity_unavailable"}),
            );
        };
        let Some(db) = self.window_activity_db.as_ref() else {
            return ToolResult::ok(
                json!({"status": "unavailable", "reason_code": "activity_store_unavailable"}),
            );
        };
        let principal = Some((principal_kind.as_str(), principal_id.as_str()));
        #[cfg(feature = "experimental-code-mode")]
        let raw = db.list_window_activity_events_with_code_mode_composition(
            window.key(),
            principal,
            SCAN_LIMIT,
        );
        #[cfg(not(feature = "experimental-code-mode"))]
        let raw = db.list_window_activity_events(window.key(), principal, SCAN_LIMIT);
        let Ok(raw) = raw else {
            return ToolResult::ok(
                json!({"status": "unavailable", "reason_code": "activity_query_unavailable"}),
            );
        };
        let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let mut visibility_cache = HashMap::new();
        let mut visible = Vec::with_capacity(raw.len());
        for event in &raw {
            visible
                .push(window_event_visible_cached(self, auth, &mut visibility_cache, event).await);
        }
        let timing = project_window_loop_timings(&raw, &visible);
        let mut summary = ActivitySummary::default();
        let mut events = Vec::<Value>::new();
        let mut eligible_count = 0usize;
        for ((event, visible), timing) in raw.into_iter().zip(visible).zip(timing) {
            if !visible {
                continue;
            }
            summary.events_scanned += 1;
            if event.meaningful {
                summary.meaningful_call_count += 1;
            }
            if event.operation.as_deref() == Some("observe_jobs") {
                summary.observe_jobs_count += 1;
            }
            if event.response_handed_at_ms.is_some() {
                summary.handler_returned_count += 1;
            }
            if event.response_streaming == Some(false) && event.response_handed_at_ms.is_none() {
                summary.missing_handoff_count += 1;
            }
            if event.meaningful {
                match event.window_transition_kind.as_deref() {
                    Some("overlap") => summary.overlapping_call_count += 1,
                    Some("serial") => summary.serial_call_count += 1,
                    _ => {}
                }
            }
            if let Some(gap) = timing.next_call_gap_ms {
                summary.observed_next_call_gap_count += 1;
                if gap < 1_000 {
                    summary.gaps_lt_1s += 1;
                }
                if gap < 2_000 {
                    summary.gaps_lt_2s += 1;
                }
                if gap < 5_000 {
                    summary.gaps_lt_5s += 1;
                }
                if gap >= 5_000 {
                    summary.gaps_ge_5s += 1;
                }
                if gap >= 10_000 {
                    summary.gaps_ge_10s += 1;
                }
                if gap >= 30_000 {
                    summary.gaps_ge_30s += 1;
                }
                if gap >= 120_000 {
                    summary.gaps_ge_120s += 1;
                }
                if gap > 0 {
                    summary.total_positive_observed_next_call_gap_ms = summary
                        .total_positive_observed_next_call_gap_ms
                        .saturating_add(gap);
                }
                summary.max_observed_next_call_gap_ms =
                    Some(summary.max_observed_next_call_gap_ms.unwrap_or(0).max(gap));
            }
            if let Some(service) = timing.service_ms {
                summary.total_service_ms = summary.total_service_ms.saturating_add(service);
                summary.max_service_ms = Some(summary.max_service_ms.unwrap_or(0).max(service));
            }
            if !include_nonmeaningful && !event.meaningful {
                continue;
            }
            eligible_count += 1;
            if events.len() >= limit {
                continue;
            }
            let projected =
                project_visible_window_activity(self, auth, &mut visibility_cache, event, timing)
                    .await;
            if let Ok(mut value) = serde_json::to_value(projected) {
                if let Some(object) = value.as_object_mut() {
                    object.remove("recorder_gap_session_id");
                    // Explicit null reports that no later canonical serial gap
                    // was observed; elapsed wall time never fills this field.
                    object.entry("next_call_gap_ms").or_insert(Value::Null);
                }
                events.push(value);
            }
        }
        summary.observe_jobs_ratio_denominator = summary.meaningful_call_count;
        summary.observe_jobs_ratio = (summary.meaningful_call_count > 0)
            .then(|| summary.observe_jobs_count as f64 / summary.meaningful_call_count as f64);
        let self_trace = crate::tool_request_trace::current_active_trace_id();
        let mut active_requests = Vec::new();
        for request in self
            .window_activity
            .list_for_window(window.key(), principal)
        {
            if self_trace.as_deref() == Some(request.server_trace_id.as_str())
                || !active_window_request_visible_cached(
                    self,
                    auth,
                    &mut visibility_cache,
                    &request,
                )
                .await
            {
                continue;
            }
            active_requests.push(json!({
                "server_trace_id": request.server_trace_id,
                "tool_name": request.tool_name,
                "started_at_ms": request.started_at_ms,
            }));
            if active_requests.len() == MAX_ACTIVE_REQUESTS {
                break;
            }
        }
        let mut output = json!({
            "status": "available",
            "events": events,
            "summary": summary,
            "active_requests": active_requests,
            // This describes the visible scanned page only. Never expose the
            // count or cap state of hidden/revoked Project events.
            "truncated": eligible_count > limit,
        });
        loop {
            output["summary"]["returned_nested_code_mode_child_count"] = json!(output["events"]
                .as_array()
                .map(|events| events
                    .iter()
                    .map(|event| event["code_mode_composition"]["nested_calls"]
                        .as_u64()
                        .unwrap_or(0) as usize)
                    .sum::<usize>())
                .unwrap_or(0));
            if crate::json_measurement::serialized_json_len(&output)
                .is_ok_and(|size| size <= MAX_OUTPUT_BYTES)
            {
                break;
            }
            let Some(events) = output["events"].as_array_mut() else {
                break;
            };
            if events.pop().is_none() {
                break;
            }
            output["truncated"] = json!(true);
        }
        ToolResult::ok(output)
    }
}
