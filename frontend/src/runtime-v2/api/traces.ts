import type { RuntimeV2Client } from "./client.js";

export type TraceQuery = {
  window_key?: string;
  project?: string;
  tool_name?: string;
  since_ms?: number;
  until_ms?: number;
  include_nonmeaningful?: boolean;
};
export type TraceRequest = {
  trace_ref?: string;
  query?: TraceQuery;
  offset?: number;
  limit?: number;
  payload_index?: number;
};
export type TraceEvent = {
  event?: string;
  phase?: string;
  diagnostic?: unknown;
  payload_index?: number;
  payload_bytes?: number;
  [field: string]: unknown;
};
export type TraceCall = {
  trace_ref: string;
  window_key: string | null;
  project: string | null;
  tool_name: string | null;
  observed_at_ms: number;
  duration_ms: number;
  status: string;
};
export type TracePage = {
  status?: string;
  reason?: string;
  error_kind?: string;
  capture_mode?: string;
  capture_health?: { scope: string; queue_drops: number; budget_drops: number; write_failures: number; loss_marker_drops?: number };
  response_handoff_observed?: boolean;
  handler_finished_observed?: boolean;
  known_dropped_events?: number;
  trace_mode?: string;
  coverage?: string;
  events?: TraceEvent[];
  calls?: TraceCall[];
  query?: TraceQuery;
  next_offset?: number | null;
  payload?: unknown;
  payload_available?: boolean;
  payload_bytes?: number;
};

export function readTrace(client: RuntimeV2Client, request: TraceRequest, signal?: AbortSignal) {
  return client.post<TracePage>("trace", request, signal);
}
