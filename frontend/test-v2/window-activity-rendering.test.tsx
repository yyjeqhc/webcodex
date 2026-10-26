import { MantineProvider } from "@mantine/core";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WindowActivityFeed } from "../src/runtime-v2/components/WindowActivityFeed.js";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import type { WindowActivity, WindowDetail } from "../src/runtime-v2/model/types.js";
import { runtimeOverview, windowDetail } from "./fixtures.js";

const time = 1_790_000_000_000;
const projects = runtimeOverview().projects;
function row(index: number): WindowActivity {
  return {
    started_at_ms: time - index * 1_000, ended_at_ms: time - index * 1_000 + 100, duration_ms: 100,
    method: "tools/call", tool_name: "tool-" + index, status: "success", meaningful: true,
    server_trace_id: "trace-" + index, workflow_sessions: [],
  };
}
const client = (post = vi.fn(async () => ({ ok: true, status: 200, data: {
  status: "available", events: [{ diagnostic: { marker: "expanded-evidence" } }],
} }))) => ({ post } as unknown as RuntimeV2Client);
function ui(detail: WindowDetail, api: RuntimeV2Client, selectedSessionId = "") {
  return <MantineProvider env="test"><WindowActivityFeed detail={detail} projects={projects} language="en"
    client={api} selectedSessionId={selectedSessionId} /></MantineProvider>;
}
afterEach(cleanup);

it("bounds initial DOM while filtering the complete loaded history and exposing failures beyond the first slice", () => {
  const activity = Array.from({ length: 2_000 }, (_, index) => row(index));
  activity[1_999] = { ...activity[1_999], tool_name: "rare-old-failure", status: "failed" };
  const detail = windowDetail({ detail_level: "full", activity, activity_returned: activity.length });
  const api = client();
  const view = render(ui(detail, api));
  expect(screen.getAllByTestId("window-workflow-step")).toHaveLength(80);
  expect(view.container.querySelectorAll("*").length).toBeLessThan(2_000);
  expect(screen.getByTestId("window-call-count").textContent).toContain("80/2000");
  expect(screen.getByTestId("window-call-count").textContent).toContain("Loaded calls: 2000");
  expect(screen.queryByText("rare-old-failure")).toBeNull();
  expect(api.post).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Failed calls (1)" }));
  expect(screen.getByText("rare-old-failure")).toBeTruthy();
  expect(screen.getAllByTestId("window-workflow-step")).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: "All calls (2000)" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Search calls" }), { target: { value: "rare-old-failure" } });
  expect(screen.getByText("rare-old-failure")).toBeTruthy();
  expect(screen.getByTestId("window-call-count").textContent).toContain("1/1");
  expect(screen.getByTestId("window-call-count").textContent).toContain("Loaded calls: 2000");
});

it("resolves Session segments before paging and exposes running calls even in oldest-first history", () => {
  const activity = Array.from({ length: 200 }, (_, index) => row(index));
  activity[190] = { ...activity[190], workflow_sessions: [{ workflow_session_id: "far-session", relation: "recorder" }] };
  activity[180] = { ...activity[180], workflow_sessions: [{ workflow_session_id: "other-session", relation: "recorder" }] };
  const detail = windowDetail({ activity, activity_returned: 200, active_count: 1,
    active_requests: [{ server_trace_id: "live", method: "tools/call", tool_name: "live-call", started_at_ms: time + 1, elapsed_ms: 50 }] });
  const api = client();
  const view = render(ui(detail, api, "far-session"));
  const calls = screen.getAllByTestId("window-workflow-step");
  expect(calls).toHaveLength(10);
  expect(screen.getByText("tool-190")).toBeTruthy();
  expect(screen.getByText("tool-181")).toBeTruthy();
  expect(screen.queryByText("tool-180")).toBeNull();
  view.rerender(ui(detail, api));
  fireEvent.click(screen.getByRole("radio", { name: "Oldest first" }));
  expect(screen.queryByText("live-call")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Running calls (1)" }));
  expect(screen.getByText("live-call")).toBeTruthy();
});

it("keeps expansion and row identity across metadata, paging and filters, and resets them for another Window", async () => {
  const detail = windowDetail({ activity: Array.from({ length: 160 }, (_, index) => row(index)), activity_returned: 160 });
  const api = client();
  const view = render(ui(detail, api));
  const first = screen.getAllByTestId("window-workflow-step")[0];
  fireEvent.click(within(first).getByRole("button", { name: "Inspect call diagnostics" }));
  await screen.findByText(/expanded-evidence/);
  const calls = vi.mocked(api.post).mock.calls.length;
  view.rerender(ui({ ...detail, last_seen_at_ms: time + 1 }, api));
  expect(screen.getAllByTestId("window-workflow-step")[0]).toBe(first);
  expect(within(first).getByRole("button", { name: "Hide call diagnostics" }).getAttribute("aria-expanded")).toBe("true");
  expect(api.post).toHaveBeenCalledTimes(calls);
  fireEvent.click(screen.getByRole("button", { name: /Show more calls/ }));
  expect(screen.getAllByTestId("window-workflow-step")).toHaveLength(160);
  expect(screen.getAllByTestId("window-workflow-step")[0]).toBe(first);
  fireEvent.change(screen.getByRole("textbox", { name: "Search calls" }), { target: { value: "tool-159" } });
  expect(screen.queryByRole("button", { name: "Hide call diagnostics" })).toBeNull();
  fireEvent.change(screen.getByRole("textbox", { name: "Search calls" }), { target: { value: "" } });
  expect(screen.getByRole("button", { name: "Hide call diagnostics" }).getAttribute("aria-expanded")).toBe("true");
  await screen.findByText(/expanded-evidence/);
  view.rerender(ui({ ...detail, client_window_key: "b".repeat(64) }, api));
  expect(screen.queryByRole("button", { name: "Hide call diagnostics" })).toBeNull();
  expect(screen.queryByText(/expanded-evidence/)).toBeNull();
  expect(screen.getAllByTestId("window-workflow-step")).toHaveLength(80);
});

it("renders changed status, duration, Project and Session facts for the same exact call", () => {
  const original = { ...row(0), project: projects[0].id };
  const detail = windowDetail({ activity: [original], activity_returned: 1 });
  const api = client();
  const view = render(ui(detail, api));
  expect(screen.getByText("Succeeded")).toBeTruthy();
  const changed = { ...original, status: "failed", duration_ms: 987,
    workflow_sessions: [{ workflow_session_id: "session-new", project: projects[0].id, relation: "recorder" }] };
  view.rerender(ui({ ...detail, activity: [changed] }, api));
  expect(screen.getByText("Failed")).toBeTruthy();
  expect(screen.getByText("987ms")).toBeTruthy();
  expect(screen.getByTestId("window-session-tag").getAttribute("title")).toBe("session-new");
});
