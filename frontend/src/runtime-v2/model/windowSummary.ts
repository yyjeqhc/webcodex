import type { WindowDetail, WindowSummary } from "./types.js";

export type WindowObservation = Pick<WindowSummary, "client_window_key" | "source" | "last_seen_at_ms" | "active_count"> & Partial<WindowSummary>;

/** Server activity times are monotonic evidence, not browser refresh times.
 * Older responses may add missing facts but cannot replace newer activity. */
export function mergeWindowSummary(current: WindowSummary, incoming: WindowObservation, preferIncoming = true): WindowSummary {
  if (current.client_window_key !== incoming.client_window_key) return current;
  const incomingIsNewer = incoming.last_seen_at_ms > current.last_seen_at_ms ||
    (incoming.last_seen_at_ms === current.last_seen_at_ms && preferIncoming);
  const first = [current.first_seen_at_ms, incoming.first_seen_at_ms].filter((value): value is number => value !== undefined);
  const merged: WindowSummary = {
    ...(incomingIsNewer ? { ...current, ...incoming } : { ...incoming, ...current }),
    last_seen_at_ms: Math.max(current.last_seen_at_ms || 0, incoming.last_seen_at_ms || 0),
    ...(first.length ? { first_seen_at_ms: Math.min(...first) } : {}),
    last_tool_call_at_ms: Math.max(current.last_tool_call_at_ms || 0, incoming.last_tool_call_at_ms || 0) || undefined,
    last_meaningful_activity_at_ms: Math.max(current.last_meaningful_activity_at_ms || 0, incoming.last_meaningful_activity_at_ms || 0) || undefined,
  };
  return JSON.stringify(current) === JSON.stringify(merged) ? current : merged;
}

export function summaryFromWindowDetail(detail: WindowDetail): WindowObservation {
  const active = detail.active_requests.reduce<(typeof detail.active_requests)[number] | undefined>(
    (latest, row) => !latest || row.started_at_ms > latest.started_at_ms ? row : latest, undefined);
  const completed = detail.activity.reduce<(typeof detail.activity)[number] | undefined>(
    (latest, row) => !latest || row.ended_at_ms > latest.ended_at_ms ? row : latest, undefined);
  return {
    client_window_key: detail.client_window_key,
    source: detail.source,
    last_seen_at_ms: detail.last_seen_at_ms,
    first_seen_at_ms: detail.first_seen_at_ms,
    last_tool_call_at_ms: detail.last_tool_call_at_ms,
    last_meaningful_activity_at_ms: detail.last_meaningful_activity_at_ms,
    active_count: detail.active_count,
    ...(active || completed ? {
      last_project: active?.project || completed?.project,
      last_activity_name: active?.tool_name || active?.method || completed?.tool_name || completed?.method,
      last_activity_status: active ? "running" : completed?.status,
      last_activity_meaningful: active ? undefined : completed?.meaningful,
    } : {}),
    ...(detail.detail_level === "full" && !detail.sessions_truncated ? { linked_session_count: detail.sessions_returned } : {}),
  };
}
