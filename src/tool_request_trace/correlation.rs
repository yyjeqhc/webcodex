//! Correlation responsibility of the existing trace subsystem.
use super::*;

pub(super) const TRACE_CORRELATION_TTL_SECS: i64 = 24 * 60 * 60;

pub(super) const MAX_TRACE_CORRELATIONS: usize = 8_192;

pub(super) static TRACE_CORRELATIONS: OnceLock<Mutex<TraceCorrelations>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TraceCorrelation {
    pub(super) trace_id: String,
    pub(super) request_id: String,
    pub(super) job_id: Option<String>,
    pub(super) runner_kind: String,
    pub(super) created_at: i64,
}

#[derive(Default)]
pub(super) struct TraceCorrelations {
    pub(super) requests: HashMap<String, TraceCorrelation>,
    pub(super) jobs: HashMap<String, TraceCorrelation>,
}

pub(super) fn correlations() -> &'static Mutex<TraceCorrelations> {
    TRACE_CORRELATIONS.get_or_init(|| Mutex::new(TraceCorrelations::default()))
}

pub(super) fn prune_correlations(correlations: &mut TraceCorrelations) {
    let cutoff = now_ts().saturating_sub(TRACE_CORRELATION_TTL_SECS);
    correlations
        .requests
        .retain(|_, correlation| correlation.created_at >= cutoff);
    correlations
        .jobs
        .retain(|_, correlation| correlation.created_at >= cutoff);
    while correlations.requests.len() > MAX_TRACE_CORRELATIONS {
        let Some(oldest) = correlations
            .requests
            .iter()
            .min_by_key(|(_, correlation)| correlation.created_at)
            .map(|(request_id, _)| request_id.clone())
        else {
            break;
        };
        correlations.requests.remove(&oldest);
    }
    while correlations.jobs.len() > MAX_TRACE_CORRELATIONS {
        let Some(oldest) = correlations
            .jobs
            .iter()
            .min_by_key(|(_, correlation)| correlation.created_at)
            .map(|(job_id, _)| job_id.clone())
        else {
            break;
        };
        correlations.jobs.remove(&oldest);
    }
}

/// Record the exact Server→Runner request identity selected for the current
/// model-facing tool call. This keeps only a bounded in-memory correlation index;
/// full request bodies remain in the file-backed trace store.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_runner_request_enqueued<T: Serialize>(
    request_payload: &T,
    request_id: &str,
    client_id: &str,
    kind: &str,
    job_id: Option<&str>,
    runner_instance_id: Option<&str>,
    runner_transport: Option<&str>,
    runner_version: Option<&str>,
    runner_git_commit: Option<&str>,
) {
    if !tool_request_trace_enabled() {
        return;
    }
    let Some(trace_id) = current_active_trace_id() else {
        return;
    };
    if full_trace_enabled() {
        match serde_json::to_value(request_payload) {
            Ok(value) => capture_owned_payload_for_trace(&trace_id, "runner_request", value),
            Err(error) => tracing::warn!(
                event = "tool_trace_capture_failed",
                server_trace_id = %trace_id,
                phase = "runner_request",
                error = %error,
                "tool_trace_capture_failed"
            ),
        }
    }
    tracing::info!(
        event = "tool_runner_request_enqueued",
        server_trace_id = %trace_id,
        runner_request_id = request_id,
        runner_client_id = client_id,
        runner_request_kind = kind,
        runner_job_id = job_id.unwrap_or("-"),
        runner_agent_instance_id = runner_instance_id.unwrap_or("-"),
        runner_transport = runner_transport.unwrap_or("-"),
        runner_version = runner_version.unwrap_or("-"),
        runner_git_commit = runner_git_commit.unwrap_or("-"),
        "tool_runner_request_enqueued"
    );

    if let Ok(mut correlations) = correlations().lock() {
        prune_correlations(&mut correlations);
        let correlation = TraceCorrelation {
            trace_id: trace_id.clone(),
            request_id: request_id.to_string(),
            job_id: job_id.map(str::to_string),
            runner_kind: kind.to_string(),
            created_at: now_ts(),
        };
        correlations
            .requests
            .insert(request_id.to_string(), correlation.clone());
        if let Some(job_id) = job_id {
            correlations.jobs.insert(job_id.to_string(), correlation);
        }
    }

    {
        let mut event = base_event(&trace_id, "tool_runner_request_enqueued");
        merge_event_fields(
            &mut event,
            json!({
                "runner_request_id": request_id,
                "runner_client_id": client_id,
                "runner_request_kind": kind,
                "runner_job_id": job_id,
                "runner_agent_instance_id": runner_instance_id,
                "runner_transport": runner_transport,
                "runner_version": runner_version,
                "runner_git_commit": runner_git_commit,
            }),
        );
        enqueue_metadata_event(&trace_id, "runner_request_enqueued", event);
    }
}

pub(super) fn lookup_request_correlation(request_id: &str) -> Option<TraceCorrelation> {
    let mut correlations = correlations().lock().ok()?;
    prune_correlations(&mut correlations);
    correlations.requests.get(request_id).cloned()
}

pub(super) fn resolve_job_correlation(
    correlations: &TraceCorrelations,
    request_id: Option<&str>,
    job_id: &str,
) -> Option<TraceCorrelation> {
    let job_correlation = correlations.jobs.get(job_id).cloned()?;
    let Some(request_id) = request_id else {
        return Some(job_correlation);
    };
    let request_correlation = correlations.requests.get(request_id).cloned()?;
    (request_correlation == job_correlation).then_some(request_correlation)
}

pub(super) fn lookup_job_correlation(
    request_id: Option<&str>,
    job_id: &str,
) -> Option<TraceCorrelation> {
    let mut correlations = correlations().lock().ok()?;
    prune_correlations(&mut correlations);
    resolve_job_correlation(&correlations, request_id, job_id)
}

pub(super) fn remove_correlation(correlation: &TraceCorrelation) {
    if let Ok(mut correlations) = correlations().lock() {
        correlations.requests.remove(&correlation.request_id);
        if let Some(job_id) = correlation.job_id.as_deref() {
            correlations.jobs.remove(job_id);
        }
    }
}

/// Capture only after the shell client accepted exact Runner/request ownership.
/// Finalizing the correlation remains separate from observational capture.
pub(crate) fn capture_runner_result<T: Serialize>(request_id: &str, payload: &T) {
    if !tool_request_trace_enabled() {
        return;
    }
    let Some(correlation) = lookup_request_correlation(request_id) else {
        return;
    };
    if !full_trace_enabled() {
        let mut event = base_event(&correlation.trace_id, "tool_runner_result_accepted");
        merge_event_fields(
            &mut event,
            json!({"runner_request_id": request_id, "runner_job_id": correlation.job_id}),
        );
        enqueue_metadata_event(&correlation.trace_id, "runner_result_accepted", event);
        return;
    }
    match serde_json::to_value(payload) {
        Ok(value) => {
            let value = runner_result_trace_payload(&correlation.runner_kind, value);
            capture_owned_payload_for_trace(&correlation.trace_id, "runner_result", value)
        }
        Err(error) => tracing::warn!(
            event = "tool_trace_capture_failed",
            server_trace_id = %correlation.trace_id,
            phase = "runner_result",
            error = %error,
            "tool_trace_capture_failed"
        ),
    }
}

pub(super) fn runner_result_trace_payload(kind: &str, payload: Value) -> Value {
    if kind != "ssh_resource" {
        return payload;
    }
    let result = payload.get("result");
    json!({
        "kind": "ssh_resource",
        "exit_code": result
            .and_then(|value| value.get("exit_code"))
            .cloned()
            .unwrap_or(Value::Null),
        "stdout_present": result
            .and_then(|value| value.get("stdout"))
            .is_some_and(|value| !value.is_null()),
        "stderr_present": result
            .and_then(|value| value.get("stderr"))
            .is_some_and(|value| !value.is_null()),
        "error_present": result
            .and_then(|value| value.get("error"))
            .is_some_and(|value| !value.is_null()),
        "command_execution_state": payload
            .get("command_execution_state")
            .cloned()
            .unwrap_or(Value::Null),
    })
}

/// Finalize a non-Job Runner result correlation only after authoritative result
/// acceptance. Job-backed requests stay correlated until authoritative Job
/// terminal state is accepted.
pub(crate) fn finalize_runner_result_correlation(request_id: &str) {
    let Some(correlation) = lookup_request_correlation(request_id) else {
        return;
    };
    if correlation.job_id.is_none() {
        remove_correlation(&correlation);
    }
}

/// Capture one authoritative Runner Job update into the trace that originally
/// admitted the Job. Updates without request_id may use job_id correlation; when
/// both identities are present they must resolve to the same TraceCorrelation.
/// Capture never consumes correlation.
pub(crate) fn capture_runner_job_update<T: Serialize>(
    request_id: Option<&str>,
    job_id: &str,
    payload: &T,
) {
    let Some(correlation) = lookup_job_correlation(request_id, job_id) else {
        return;
    };
    if !full_trace_enabled() {
        return;
    }
    match serde_json::to_value(payload) {
        Ok(value) => {
            capture_owned_payload_for_trace(&correlation.trace_id, "runner_job_update", value)
        }
        Err(error) => tracing::warn!(
            event = "tool_trace_capture_failed",
            server_trace_id = %correlation.trace_id,
            phase = "runner_job_update",
            error = %error,
            "tool_trace_capture_failed"
        ),
    }
}

/// Consume Job correlation only after the shell-client layer has accepted a
/// terminal Server-authoritative Job state. A mismatched request_id + job_id
/// pair never resolves and therefore cannot consume either trace.
pub(crate) fn finalize_runner_job_correlation(request_id: Option<&str>, job_id: &str) {
    let Some(correlation) = lookup_job_correlation(request_id, job_id) else {
        return;
    };
    if tool_request_trace_enabled() {
        let mut event = base_event(&correlation.trace_id, "tool_runner_job_terminal_accepted");
        merge_event_fields(
            &mut event,
            json!({"runner_request_id":correlation.request_id, "runner_job_id":job_id}),
        );
        enqueue_metadata_event(&correlation.trace_id, "runner_job_terminal_accepted", event);
    }
    remove_correlation(&correlation);
}
