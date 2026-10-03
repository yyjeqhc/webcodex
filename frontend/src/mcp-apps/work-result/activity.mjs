// Time and DOM adapters are supplied by the mounted App. This module owns no
// timers, caches, asynchronous requests, Host channel, or teardown lifecycle.
export function renderWindowCurrentActivity({ el, activityIdleElapsed, idleAfterMs, formatDuration, formatAge }, activity) {
  const lastAt = activity.last_activity_at_ms ?? activity.last_meaningful_activity_at_ms;
  if (activity.active && activity.current) {
    el("activityStatus").textContent = activity.current.label;
    el("activityDetail").textContent = "This Window is active";
    el("activityAge").textContent = "Active now";
    return;
  }
  const idleElapsed = activityIdleElapsed(activity);
  if (idleElapsed !== null && idleElapsed >= idleAfterMs) {
    el("activityStatus").textContent = "No WebCodex activity";
    el("activityDetail").textContent = activity.last ? "Last activity: " + activity.last.label
      : activity.coverage_partial ? "Earlier activity is not fully observable" : "";
    el("activityAge").textContent = "Idle for " + formatDuration(idleElapsed);
    return;
  }
  if (activity.last) {
    el("activityStatus").textContent = activity.last.label;
    el("activityDetail").textContent = "Waiting for the next Window activity";
    el("activityAge").textContent = lastAt === null || lastAt === undefined ? "" : "Last active " + formatAge(lastAt);
    return;
  }
  el("activityStatus").textContent = "Waiting for WebCodex activity";
  el("activityDetail").textContent = activity.coverage_partial ? "Current activity is not fully observable" : "";
  el("activityAge").textContent = "";
}

export function renderActivityDetail({ document, formatDuration, formatTimestamp }, root, detail) {
  function detailItem(label, value) {
    const item = document.createElement("div"); item.className = "detail-item";
    const key = document.createElement("span"); key.textContent = label;
    const content = document.createElement("strong"); content.textContent = String(value);
    item.append(key, content);
    return item;
  }
  root.replaceChildren();
  const grid = document.createElement("div"); grid.className = "detail-grid";
  grid.append(
    detailItem("Trace", detail.server_trace_id),
    detailItem("Method", detail.tool_name || detail.method),
    detailItem("Status", detail.status),
  );
  if (detail.project) grid.append(detailItem("Project", detail.project));
  if (detail.service_ms != null) grid.append(detailItem("Service time", formatDuration(detail.service_ms)));
  if (detail.http_status != null) grid.append(detailItem("HTTP", detail.http_status));
  if (detail.window_transition_kind) grid.append(detailItem("Window transition", detail.window_transition_kind));
  if (detail.response_handed_at_ms != null) grid.append(detailItem("Response handed off", formatTimestamp(detail.response_handed_at_ms)));
  if (detail.async_job_id) grid.append(detailItem("Async job", detail.async_job_id));
  const observedJobIds = detail.observed_job_ids ?? [];
  if (observedJobIds.length) grid.append(detailItem("Observed jobs", observedJobIds.join(", ")));
  if (detail.workflow_sessions.length) grid.append(detailItem("Workflow Sessions", detail.workflow_sessions.map(item => item.workflow_session_id).join(", ")));
  root.append(grid);
}
