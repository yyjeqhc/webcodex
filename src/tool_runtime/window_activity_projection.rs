//! Canonical sanitized Window activity and observed timing projection shared by the Runtime Console and model tool.
use super::ToolRuntime;
use crate::auth::AuthContext;
#[cfg(feature = "experimental-code-mode")]
use serde::Deserialize;
use serde::Serialize;
#[cfg(feature = "experimental-code-mode")]
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RuntimeConsoleWindowActivity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_observed_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) response_handed_at_ms: Option<i64>,
    pub(crate) started_at_ms: i64,
    pub(crate) ended_at_ms: i64,
    pub(crate) duration_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) service_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) next_call_gap_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cycle_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) window_transition_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) response_streaming: Option<bool>,
    pub(crate) method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) activity_presentation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) activity_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project: Option<String>,
    pub(crate) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) http_status: Option<i64>,
    pub(crate) meaningful: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) async_job_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) observed_job_ids: Vec<String>,
    #[cfg(feature = "experimental-code-mode")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) code_mode_composition: Option<RuntimeConsoleCodeModeComposition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) recorder_gap_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) server_trace_id: Option<String>,
    pub(crate) workflow_sessions: Vec<RuntimeConsoleWindowActivitySession>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct WindowActivityTimingProjection {
    pub(crate) service_ms: Option<i64>,
    pub(crate) next_call_gap_ms: Option<i64>,
    pub(crate) cycle_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RuntimeConsoleWindowActivitySession {
    workflow_session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
    relation: String,
}

#[cfg(feature = "experimental-code-mode")]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuntimeConsoleCodeModeComposition {
    pub(crate) nested_calls: usize,
    pub(crate) nested_successes: usize,
    pub(crate) nested_failures: usize,
    pub(crate) max_in_flight: usize,
    pub(crate) duration_ms: u64,
    pub(crate) slot_wait_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) input_bytes: Option<usize>,
    pub(crate) returned_bytes: usize,
    pub(crate) nested_raw_result_bytes_total: usize,
    pub(crate) nested_tool_counts: BTreeMap<String, usize>,
    pub(crate) consequential_calls: usize,
    pub(crate) known_results: usize,
    pub(crate) job_handoffs: usize,
    pub(crate) outcome_unknown: usize,
}

#[cfg(feature = "experimental-code-mode")]
pub(crate) fn project_code_mode_composition(
    value: &Value,
) -> Option<RuntimeConsoleCodeModeComposition> {
    let projection =
        serde_json::from_value::<RuntimeConsoleCodeModeComposition>(value.clone()).ok()?;
    let counted = projection
        .nested_tool_counts
        .values()
        .try_fold(0usize, |total, value| total.checked_add(*value))?;
    let consequential_counted = projection
        .known_results
        .checked_add(projection.job_handoffs)
        .and_then(|total| total.checked_add(projection.outcome_unknown))?;
    if projection.nested_calls > 32
        || projection.max_in_flight > 8
        || projection
            .nested_successes
            .saturating_add(projection.nested_failures)
            != projection.nested_calls
        || counted != projection.nested_calls
        || consequential_counted != projection.consequential_calls
        || projection.consequential_calls > projection.nested_calls
        || projection.nested_tool_counts.len() > 32
        || projection
            .nested_tool_counts
            .keys()
            .any(|tool| !crate::tool_runtime::code_mode_nested_tool_is_admitted(tool))
    {
        return None;
    }
    Some(projection)
}

pub(crate) fn project_window_loop_timings(
    events: &[webcodex_store::models::WindowActivityEventRecord],
    visible: &[bool],
) -> Vec<WindowActivityTimingProjection> {
    let mut projections = vec![WindowActivityTimingProjection::default(); events.len()];
    let mut previous_meaningful = BTreeMap::<(String, String), usize>::new();

    for index in (0..events.len()).rev() {
        let event = &events[index];
        if event.response_streaming == Some(false) {
            if let (Some(started), Some(handed)) =
                (event.request_observed_at_ms, event.response_handed_at_ms)
            {
                if handed >= started {
                    projections[index].service_ms = Some(handed - started);
                }
            }
        }
        if !event.meaningful {
            continue;
        }
        let Some(principal_kind) = event.principal_correlation_kind.as_ref() else {
            continue;
        };
        let Some(principal_id) = event.principal_correlation_id.as_ref() else {
            continue;
        };
        let key = (principal_kind.clone(), principal_id.clone());
        let previous_index = previous_meaningful.remove(&key);
        if event.window_transition_kind.as_deref() == Some("serial") {
            if let Some(previous_index) = previous_index {
                let previous = &events[previous_index];
                // Do not bridge over an event whose Project is hidden/revoked:
                // exposing a derived timestamp across that boundary would turn
                // Window timing into a Project-existence oracle.
                if visible.get(previous_index) == Some(&true) && visible.get(index) == Some(&true) {
                    if let (Some(current_started), Some(previous_handed), Some(previous_started)) = (
                        event.request_observed_at_ms,
                        previous.response_handed_at_ms,
                        previous.request_observed_at_ms,
                    ) {
                        if current_started >= previous_handed {
                            projections[previous_index].next_call_gap_ms =
                                Some(current_started - previous_handed);
                        }
                        if current_started >= previous_started {
                            projections[previous_index].cycle_ms =
                                Some(current_started - previous_started);
                        }
                    }
                }
            }
        }
        if event.window_continuity_eligible == Some(true) {
            previous_meaningful.insert(key, index);
        }
    }
    projections
}

pub(crate) async fn project_visible_window_activity(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    visibility_cache: &mut HashMap<String, bool>,
    event: webcodex_store::models::WindowActivityEventRecord,
    timing: WindowActivityTimingProjection,
) -> RuntimeConsoleWindowActivity {
    #[cfg(feature = "experimental-code-mode")]
    let code_mode_composition = event
        .code_mode_composition
        .as_ref()
        .and_then(project_code_mode_composition);
    let mut activity_sessions = Vec::new();
    for link in event.workflow_links {
        if !super::window_activity::window_project_visible_cached(
            runtime,
            auth,
            visibility_cache,
            link.project.as_deref(),
        )
        .await
        {
            continue;
        }
        activity_sessions.push(RuntimeConsoleWindowActivitySession {
            workflow_session_id: link.workflow_session_id,
            project: link.project,
            relation: link.relation,
        });
    }
    let activity_semantics = event
        .operation
        .as_deref()
        .map(webcodex_tool_contracts::runtime_tool_activity_semantics);
    RuntimeConsoleWindowActivity {
        request_observed_at_ms: event.request_observed_at_ms,
        response_handed_at_ms: event.response_handed_at_ms,
        started_at_ms: event.started_at_ms,
        ended_at_ms: event.ended_at_ms,
        duration_ms: event.duration_ms,
        service_ms: timing.service_ms,
        next_call_gap_ms: timing.next_call_gap_ms,
        cycle_ms: timing.cycle_ms,
        window_transition_kind: event.window_transition_kind,
        response_streaming: event.response_streaming,
        method: match event.action_name.as_str() {
            "toolsCall" => "tools/call".to_string(),
            "toolsList" => "tools/list".to_string(),
            other => other.to_string(),
        },
        tool_name: event.operation,
        activity_presentation: activity_semantics
            .map(|semantics| semantics.presentation.as_str().to_string()),
        activity_kind: activity_semantics
            .and_then(|semantics| semantics.kind.as_str().map(str::to_string)),
        project: event.project,
        status: event.status,
        http_status: event.http_status,
        // Persisted event-time truth: never recompute historical meaningfulness
        // from the current ToolDefinition activity policy.
        meaningful: event.meaningful,
        async_job_id: event.async_job_id,
        observed_job_ids: event.observed_job_ids,
        #[cfg(feature = "experimental-code-mode")]
        code_mode_composition,
        recorder_gap_session_id: event.recorder_gap_session_id,
        server_trace_id: event.server_trace_id,
        workflow_sessions: activity_sessions,
    }
}

pub(crate) async fn project_window_activity(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    visibility_cache: &mut HashMap<String, bool>,
    event: webcodex_store::models::WindowActivityEventRecord,
) -> Option<RuntimeConsoleWindowActivity> {
    if !super::window_activity::window_event_visible_cached(runtime, auth, visibility_cache, &event)
        .await
    {
        return None;
    }
    let service_ms = if event.response_streaming == Some(false) {
        event
            .request_observed_at_ms
            .zip(event.response_handed_at_ms)
            .and_then(|(started, handed)| handed.checked_sub(started))
            .filter(|elapsed| *elapsed >= 0)
    } else {
        None
    };
    Some(
        project_visible_window_activity(
            runtime,
            auth,
            visibility_cache,
            event,
            WindowActivityTimingProjection {
                service_ms,
                ..WindowActivityTimingProjection::default()
            },
        )
        .await,
    )
}
