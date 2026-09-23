import type { WindowCall, WindowDetail } from "../../models/workspace";
import type { ContinuationSummary } from "../../models/runtime-shell";

export function recentMeaningfulCalls(detail: WindowDetail): WindowCall[] {
  return detail.activity.filter(row => row.meaningful === true).slice().sort((a, b) => (b.ended_at_ms ?? b.started_at_ms ?? 0) - (a.ended_at_ms ?? a.started_at_ms ?? 0));
}
export function executionLabel(status?: string): string {
  if (status === "success" || status === "succeeded" || status === "completed") return "Completed";
  if (status === "failed" || status === "error") return "Failed";
  if (status === "cancelled") return "Cancelled";
  return "Unknown";
}
export function continuationFromWindow(detail: WindowDetail, now: number): ContinuationSummary | null {
  const latest = recentMeaningfulCalls(detail)[0];
  if (!latest) return null;
  const observed = latest.request_observed_at_ms ?? null;
  const handed = latest.response_handed_at_ms != null && observed != null && latest.response_handed_at_ms >= observed ? latest.response_handed_at_ms : null;
  const ended = handed ?? latest.ended_at_ms ?? latest.started_at_ms;
  const later = handed == null ? [] : detail.activity.filter(row => row.meaningful === true)
    .map(row => row.request_observed_at_ms ?? row.started_at_ms).filter((time): time is number => time != null && time > handed);
  const next = later.length ? Math.min(...later) : null;
  return { tool_name: latest.tool_name ?? null, execution: executionLabel(latest.status).toLowerCase(),
    response_handoff: handed == null ? "not_confirmed" : latest.response_streaming ? "stream_started" : "handler_returned",
    request_observed_at_ms: observed, response_handed_at_ms: handed, service_ms: latest.service_ms ?? null,
    previous_response_gap_ms: latest.next_call_gap_ms ?? null,
    next_call_gap_ms: next != null && handed != null ? next - handed : null, next_meaningful_call: next == null ? "not_observed" : "observed",
    elapsed_ms: ended == null ? null : Math.max(0, now - ended), window_transition_kind: latest.window_transition_kind ?? null,
    active_request_count: detail.active_count, history_partial: detail.activity_truncated };
}
