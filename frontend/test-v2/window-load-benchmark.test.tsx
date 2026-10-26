import { MantineProvider } from "@mantine/core";
import { act, render, renderHook } from "@testing-library/react";
import { Profiler } from "react";
import { expect, it, vi } from "vitest";
import { WindowActivityFeed } from "../src/runtime-v2/components/WindowActivityFeed.js";
import { useWindowWorkspace } from "../src/runtime-v2/state/useWindowWorkspace.js";
import { windowDetail, runtimeOverview } from "./fixtures.js";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";

const time = 1_790_000_000_000;
const detail = windowDetail({
  detail_level: "full",
  activity: Array.from({ length: 2_000 }, (_, index) => ({
    started_at_ms: time - index * 1_000,
    ended_at_ms: time - index * 1_000 + 100,
    duration_ms: 100,
    method: "tools/call",
    tool_name: "read_files",
    project: "agent:r:project-" + index % 100,
    status: "success",
    meaningful: true,
    server_trace_id: "trace-" + index,
    workflow_sessions: [{
      workflow_session_id: "wc_sess_" + index % 40,
      project: "agent:r:project-" + index % 100,
      relation: "recorder",
    }],
  })),
  activity_returned: 2_000,
});
const primary = {
  ...detail, detail_level: "primary", activity: detail.activity.slice(0, 80),
  activity_returned: 80, activity_truncated: true,
};
const percentile = (samples: number[], quantile: number) => [...samples].sort((left, right) => left - right)[
  Math.min(samples.length - 1, Math.ceil(samples.length * quantile) - 1)
];

// Reproducible synthetic load measurement, not a real-browser performance gate.
// Timings depend on the test host; request counts and retained rows are deterministic.
it("records synthetic 2000-call DOM and repeat-render measurements", async () => {
  const projects = Array.from({ length: 100 }, (_, index) => ({
    ...runtimeOverview().projects[0], id: "agent:r:project-" + index, path: "/fixture/" + index,
  }));
  const durations: number[] = [];
  let mountMs = 0;
  const client = { post: vi.fn(async () => ({ ok: true, status: 200, data: {} })) } as unknown as RuntimeV2Client;
  const select = vi.fn();
  const ui = (value: typeof detail) => <MantineProvider env="test">
    <Profiler id="feed" onRender={(_id, phase, ms) => {
      if (phase === "mount") mountMs = ms;
      else durations.push(ms);
    }}>
      <WindowActivityFeed detail={value} projects={projects} language="en" client={client} onSelectSession={select} />
    </Profiler>
  </MantineProvider>;
  const started = performance.now();
  const view = render(ui(detail));
  const mountWall = performance.now() - started;
  const nodes = view.container.querySelectorAll("*").length;
  for (let index = 0; index < 10; index++) view.rerender(ui({ ...detail, last_seen_at_ms: time + index }));
  console.info("WINDOW_FEED_BENCH " + JSON.stringify({
    environment: "jsdom", calls: 2_000, projects: 100, referenced_sessions: 40,
    dom_nodes: nodes, mount_profiler_ms: mountMs, mount_wall_ms: mountWall,
    update_samples: durations.length, update_p50_ms: percentile(durations, .5), update_p95_ms: percentile(durations, .95),
  }));
  expect(view.container.querySelectorAll(".window-call-card").length).toBeGreaterThan(0);
  view.unmount();
}, 60_000);

it("records two independent page polling and repeated failed history", async () => {
  vi.useFakeTimers();
  try {
    let fail = false;
    const counts = { inventory: 0, liveness: 0, primary: 0, full: 0 };
    const client = { post: vi.fn(async (path: string, body: { projection?: string; detail_level?: string }) => {
      if (path === "windows") {
        if (body.projection === "liveness") {
          counts.liveness++;
          return { ok: true, status: 200, data: { windows: [] } };
        }
        counts.inventory++;
        return { ok: true, status: 200, data: { windows: [{
          client_window_key: detail.client_window_key, last_seen_at_ms: time, source: "openai-session",
          active_count: 0, linked_session_count: 0, recorder_gap_count: 0,
        }], total: 1 } };
      }
      if (body.detail_level === "primary") {
        counts.primary++;
        return { ok: true, status: 200, data: structuredClone(primary) };
      }
      counts.full++;
      return fail ? { ok: false, status: 503, data: null } : { ok: true, status: 200, data: structuredClone(detail) };
    }) } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    // Independent hook trees share a jsdom document, not browser tabs or a cross-tab transport.
    const first = renderHook(() => useWindowWorkspace(client, true, unauthorized));
    const second = renderHook(() => useWindowWorkspace(client, true, unauthorized));
    await act(async () => {});
    let previous = first.result.current.detail!.activity;
    let replacements = 0;
    for (let tick = 0; tick < 9; tick++) {
      await act(async () => { await vi.advanceTimersByTimeAsync(3_000); });
      const current = first.result.current.detail!.activity;
      if (current !== previous) replacements++;
      previous = current;
    }
    console.info("WINDOW_POLL_BENCH " + JSON.stringify({
      environment: "jsdom", simulated_pages: 2, period_ms: 27_000, ...counts,
      unchanged_history_array_replacements: replacements,
    }));
    const fullBeforeFailure = counts.full;
    fail = true;
    for (let tick = 0; tick < 10; tick++) await act(async () => { await vi.advanceTimersByTimeAsync(3_000); });
    console.info("WINDOW_503_BENCH " + JSON.stringify({
      environment: "jsdom", simulated_pages: 2, period_ms: 30_000,
      full_failures: counts.full - fullBeforeFailure, detail_availability: first.result.current.detailAvailability, ...counts,
    }));
    expect(counts.full - fullBeforeFailure).toBe(2);
    expect(first.result.current.detailAvailability).toBe("stale");
    first.unmount();
    second.unmount();
  } finally {
    vi.useRealTimers();
  }
}, 60_000);
