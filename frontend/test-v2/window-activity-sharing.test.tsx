import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import type { WindowActivity } from "../src/runtime-v2/model/types.js";
import { mergeWindowActivity, sameWindowValue } from "../src/runtime-v2/model/windowActivity.js";
import { useWindowWorkspace } from "../src/runtime-v2/state/useWindowWorkspace.js";
import { windowDetail } from "./fixtures.js";

const time = 1_790_000_000_000;
function activity(index: number): WindowActivity {
  return {
    started_at_ms: time - index * 1_000, ended_at_ms: time - index * 1_000 + 100,
    duration_ms: 100, method: "tools/call", tool_name: "read_files", project: "project-a",
    status: "success", meaningful: true, server_trace_id: "trace-" + index,
    workflow_sessions: [{ workflow_session_id: "session-a", project: "project-a", relation: "recorder" }],
  };
}
afterEach(() => { cleanup(); vi.useRealTimers(); });

it("shares the retained array for semantically unchanged primary heads and full snapshots", () => {
  const retained = Array.from({ length: 2_000 }, (_, index) => activity(index));
  expect(mergeWindowActivity(retained, structuredClone(retained.slice(0, 80))).activity).toBe(retained);
  expect(mergeWindowActivity(retained, structuredClone(retained)).activity).toBe(retained);
});

it.each([
  ["status", { status: "failed" }],
  ["duration", { duration_ms: 901 }],
  ["Project", { project: "project-b" }],
  ["Session", { workflow_sessions: [{ workflow_session_id: "session-b", project: "project-b", relation: "recorder" }] }],
  ["observed Jobs", { observed_job_ids: ["job-new"] }],
  ["timing", { service_ms: 81, next_call_gap_ms: 12, cycle_ms: 93 }],
  ["expectation", { failure_expectation_result: "unexpected_failure" }],
  ["new response field", { newly_observed_fact: true }],
])("observes changed %s facts while retaining untouched row objects", (_name, changed) => {
  const retained = [activity(0), activity(1)];
  const next = { ...structuredClone(retained[0]), ...changed };
  expect(sameWindowValue(retained[0], next)).toBe(false);
  const merged = mergeWindowActivity(retained, [next]).activity;
  expect(merged).not.toBe(retained);
  expect(merged[0]).toEqual(next);
  expect(merged[1]).toBe(retained[1]);
});

it("retains a newly observed trace instead of treating equal timing and status as unchanged", () => {
  const retained = [activity(0)];
  const changed = { ...retained[0], server_trace_id: "trace-new" };
  expect(sameWindowValue(retained[0], changed)).toBe(false);
  expect(mergeWindowActivity(retained, [changed]).activity).toContainEqual(changed);
});

it("updates mutable untraced call facts and preserves repeated invocations", () => {
  const first = { ...activity(0), server_trace_id: undefined };
  const second = { ...first, duration_ms: 150 };
  const changed = { ...first, status: "failed", duration_ms: 999, project: "project-b" };
  const result = mergeWindowActivity([first, second], [changed, second, activity(-1)]).activity;
  expect(result).toHaveLength(3);
  expect(result).toContainEqual(changed);
  expect(result).toContain(second);
});

it("keeps the retained history bounded when new calls arrive", () => {
  const retained = Array.from({ length: 2_000 }, (_, index) => activity(index));
  const next = activity(-1);
  const merged = mergeWindowActivity(retained, [next]);
  expect(merged.activity).toHaveLength(2_000);
  expect(merged.truncated).toBe(true);
  expect(merged.activity[0]).toBe(next);
  expect(merged.activity[1]).toBe(retained[0]);
});

it("shares activity, linked Sessions, Jobs and live rows through real primary and full polling", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(time);
  let current = windowDetail({
    detail_level: "full", activity: [activity(0), activity(1)], activity_returned: 2, active_count: 1,
    active_requests: [{ server_trace_id: "live", method: "tools/call", started_at_ms: time, elapsed_ms: 100 }],
    linked_sessions: [{ workflow_session_id: "session-a", project: "project-a", first_linked_at_ms: time,
      last_linked_at_ms: time, relations: ["recorder"], relation_count: 1 }],
    sessions_returned: 1,
    jobs: [{ job_id: "job-a", status: "completed", active: false, terminal: true }],
  });
  let fullCalls = 0;
  const client = { post: vi.fn(async (path: string, body: { detail_level?: string }) => {
    if (path === "windows") return { ok: true, status: 200, data: { windows: [], total: 0 } };
    if (body.detail_level !== "primary") fullCalls++;
    return { ok: true, status: 200, data: { ...structuredClone(current), detail_level: body.detail_level || "full" } };
  }) } as unknown as RuntimeV2Client;
  const unauthorized = vi.fn();
  const hook = renderHook(() => useWindowWorkspace(client, true, unauthorized, { initialWindowKey: current.client_window_key }));
  await act(async () => {});
  const initial = hook.result.current.detail!;
  for (let tick = 0; tick < 10; tick++) await act(async () => { await vi.advanceTimersByTimeAsync(3_000); });
  const refreshed = hook.result.current.detail!;
  expect(fullCalls).toBe(2);
  expect(refreshed.activity).toBe(initial.activity);
  expect(refreshed.activity[0]).toBe(initial.activity[0]);
  expect(refreshed.active_requests).toBe(initial.active_requests);
  expect(refreshed.linked_sessions).toBe(initial.linked_sessions);
  expect(refreshed.jobs).toBe(initial.jobs);

  current = { ...current, last_seen_at_ms: time + 1, activity: [
    { ...current.activity[0], status: "failed", duration_ms: 987, project: "project-b",
      workflow_sessions: [{ workflow_session_id: "session-b", project: "project-b", relation: "recorder" }] },
    current.activity[1],
  ] };
  await act(async () => { await vi.advanceTimersByTimeAsync(3_000); });
  expect(hook.result.current.detail!.activity).not.toBe(initial.activity);
  expect(hook.result.current.detail!.activity[0]).toMatchObject({ status: "failed", duration_ms: 987, project: "project-b" });
  expect(hook.result.current.detail!.activity[0].workflow_sessions[0].workflow_session_id).toBe("session-b");
  expect(hook.result.current.detail!.activity[1]).toBe(initial.activity[1]);
});
