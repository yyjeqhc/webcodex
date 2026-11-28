// Time and DOM adapters are supplied by the mounted App. This module owns no
// timers, caches, asynchronous requests, Host channel, or teardown lifecycle.
export function renderWindowCurrentActivity({ el, activityIdleElapsed, idleAfterMs, formatDuration, formatAge }, activity) {
  const lastAt = activity.last_activity_at_ms ?? activity.last_meaningful_activity_at_ms;
  if (activity.active && activity.current) {
    el("activityStatus").textContent = activity.current.label;
    el("activityDetail").textContent = "";
    el("activityAge").textContent = "当前活跃";
    return;
  }
  const idleElapsed = activityIdleElapsed(activity);
  if (idleElapsed !== null && idleElapsed >= idleAfterMs) {
    el("activityStatus").textContent = "WebCodex 暂无活动";
    el("activityDetail").textContent = activity.last ? "最近活动：" + activity.last.label
      : activity.coverage_partial ? "更早的活动无法完整观察" : "";
    el("activityAge").textContent = "已空闲 " + formatDuration(idleElapsed);
    return;
  }
  if (activity.last) {
    el("activityStatus").textContent = activity.last.label;
    el("activityDetail").textContent = "等待下一次调用";
    el("activityAge").textContent = lastAt === null || lastAt === undefined ? "" : "最近活动于 " + formatAge(lastAt);
    return;
  }
  el("activityStatus").textContent = "等待 WebCodex 活动";
  el("activityDetail").textContent = activity.coverage_partial ? "当前活动无法完整观察" : "";
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
    detailItem("追踪", detail.server_trace_id),
    detailItem("方法", detail.tool_name || detail.method),
    detailItem("状态", ({ success: "成功", succeeded: "成功", completed: "已完成", failed: "失败", error: "失败",
      timeout: "超时", running: "运行中", unknown: "结果未知" })[detail.status] || detail.status),
  );
  if (detail.project) grid.append(detailItem("项目", detail.project));
  if (detail.service_ms != null) grid.append(detailItem("服务耗时", formatDuration(detail.service_ms)));
  if (detail.http_status != null) grid.append(detailItem("HTTP", detail.http_status));
  if (detail.window_transition_kind) grid.append(detailItem("窗口转换", detail.window_transition_kind));
  if (detail.response_handed_at_ms != null) grid.append(detailItem("响应移交", formatTimestamp(detail.response_handed_at_ms)));
  if (detail.async_job_id) grid.append(detailItem("异步任务", detail.async_job_id));
  const observedJobIds = detail.observed_job_ids ?? [];
  if (observedJobIds.length) grid.append(detailItem("已观察任务", observedJobIds.join(", ")));
  if (detail.workflow_sessions.length) grid.append(detailItem("工作流会话", detail.workflow_sessions.map(item => item.workflow_session_id).join(", ")));
  root.append(grid);
}
