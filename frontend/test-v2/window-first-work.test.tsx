import { fireEvent, render as testingRender, screen, waitFor, within } from "@testing-library/react";
import type { ReactElement } from "react";
import { expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { absoluteTime } from "../src/runtime-v2/model/format.js";
import type { ProjectRow } from "../src/runtime-v2/model/types.js";
import { ProjectsView } from "../src/runtime-v2/views/ProjectsView.js";
import { WorkView } from "../src/runtime-v2/views/WorkView.js";
import { UiProvider } from "../src/ui/UiProvider.js";
import { runtimeOverview, windowDetail } from "./fixtures.js";

const render = (ui: ReactElement) => testingRender(<UiProvider>{ui}</UiProvider>);

function ok(data: unknown) {
  return { ok: true, status: 200, data };
}

function fakeClient(handler: (path: string, payload: any) => any): RuntimeV2Client {
  return {
    post: vi.fn(async (path: string, payload: any) => handler(path, payload)),
    postAt: vi.fn(async (base: string, path: string, payload: any) => handler(base + path, payload)),
  } as unknown as RuntimeV2Client;
}

function projectFamily(): [ProjectRow, ProjectRow] {
  const source: ProjectRow = {
    ...runtimeOverview().projects[0],
    id: "agent:special:webcodex",
    name: "WebCodex",
    path: "/root/git/webcodex",
  };
  const worktree: ProjectRow = {
    ...source,
    id: "agent:special:webcodex-activity-fix-1234",
    name: "webcodex-activity-fix",
    path: "/root/.webcodex-managed-worktrees/webcodex-activity-fix",
    lineage: {
      kind: "managed_worktree_source",
      source_project_id: "webcodex",
      base_sha: "d8d92a5412f26410f8f61799e347241aaf70c5a2",
    },
  };
  return [source, worktree];
}

it("shows active Window work without any Workflow Session and keeps observe calls visible", async () => {
  const [source, worktree] = projectFamily();
  const activeKey = "a".repeat(64);
  const observeKey = "b".repeat(64);
  const observeStarted = 1_790_000_050_000;
  const activeStarted = 1_790_000_100_000;

  const client = fakeClient((path, payload) => {
    if (path === "windows") return ok({
      windows: [
        {
          client_window_key: activeKey,
          last_project: worktree.id,
          source: "openai-session",
          last_seen_at_ms: activeStarted,
          last_activity_name: "apply_text_edits",
          last_activity_status: "running",
          last_activity_meaningful: true,
          active_count: 1,
          linked_session_count: 0,
          recorder_gap_count: 0,
        },
        {
          client_window_key: observeKey,
          last_project: source.id,
          source: "openai-session",
          last_seen_at_ms: observeStarted + 100,
          last_activity_name: "runtime_status",
          last_activity_status: "success",
          last_activity_meaningful: false,
          active_count: 0,
          linked_session_count: 0,
          recorder_gap_count: 0,
        },
      ],
      returned: 2,
      total: 2,
      truncated: false,
      visibility: { scope: "principal" },
    });
    if (path === "window" && payload.client_window_key === activeKey) return ok(windowDetail({
      client_window_key: activeKey,
      last_seen_at_ms: activeStarted,
      active_count: 1,
      active_requests: [{
        server_trace_id: "trace-active-edit",
        method: "tools/call",
        tool_name: "apply_text_edits",
        project: worktree.id,
        started_at_ms: activeStarted,
        elapsed_ms: 4200,
      }],
      linked_sessions: [],
      sessions_returned: 0,
      activity: [{
        started_at_ms: observeStarted,
        ended_at_ms: observeStarted + 100,
        duration_ms: 100,
        method: "tools/call",
        tool_name: "runtime_status",
        activity_presentation: "Inspect Runtime status",
        activity_kind: "observation",
        project: worktree.id,
        status: "success",
        meaningful: false,
        server_trace_id: "trace-observe",
        workflow_sessions: [],
      }],
      activity_returned: 1,
    }));
    if (path === "window") return ok(windowDetail({ client_window_key: observeKey }));
    throw new Error("unexpected path " + path);
  });

  render(
    <WorkView
      client={client}
      items={[]}
      selected={null}
      projects={[source, worktree]}
      language="en"
      inventoryIncomplete={false}
      onOpenSession={vi.fn()}
      onLocateSession={vi.fn(async () => false)}
      onUnauthorized={vi.fn()}
    />,
  );

  expect(await screen.findByRole("searchbox", { name: "Search Windows" })).toBeTruthy();
  const activeRow = await screen.findByTestId("work-window-row-" + activeKey);
  expect(within(activeRow).getByText("apply_text_edits")).toBeTruthy();
  expect(activeRow.textContent).toContain("WebCodex");
  expect(activeRow.textContent).toContain("webcodex-activity-fix");
  expect(activeRow.textContent).toContain("1 active");

  expect(await screen.findByText("Every observed WebCodex request is shown, including observe and diagnostic actions.")).toBeTruthy();
  expect(screen.getByText("Inspect Runtime status")).toBeTruthy();
  expect(screen.getByText("Observe")).toBeTruthy();
  expect(screen.getAllByText("No explicit Session link").length).toBeGreaterThan(0);
  expect(screen.getAllByTitle(absoluteTime(observeStarted)).length).toBeGreaterThan(0);

  const search = screen.getByRole("searchbox", { name: "Search Windows" });
  fireEvent.change(search, { target: { value: "runtime_status" } });
  expect(screen.queryByTestId("work-window-row-" + activeKey)).toBeNull();
  expect(screen.getByTestId("work-window-row-" + observeKey)).toBeTruthy();
});

it("groups a managed worktree under one human Project and exposes Window activity at the family level", async () => {
  const [source, worktree] = projectFamily();
  const key = "c".repeat(64);
  const openWindow = vi.fn();

  const client = fakeClient((path) => {
    if (path === "projects") return ok({ projects: [source, worktree], total: 2, truncated: false });
    if (path === "project-git") return ok({ branch: "main", clean: true, git_available: true });
    if (path === "windows") return ok({
      windows: [{
        client_window_key: key,
        last_project: worktree.id,
        source: "openai-session",
        last_seen_at_ms: 1_790_000_100_000,
        last_activity_name: "apply_text_edits",
        last_activity_status: "running",
        last_activity_meaningful: true,
        active_count: 1,
        linked_session_count: 0,
        recorder_gap_count: 0,
      }],
      returned: 1,
      total: 1,
      truncated: false,
      visibility: { scope: "principal" },
    });
    if (path === "workflow-sessions") return ok({ sessions: [], total: 0, returned: 0, truncated: false });
    throw new Error("unexpected path " + path);
  });

  render(
    <ProjectsView
      client={client}
      language="en"
      runners={runtimeOverview().runners}
      onOpenSession={vi.fn()}
      onOpenWindow={openWindow}
      onUnauthorized={vi.fn()}
    />,
  );

  await waitFor(() => expect(screen.getAllByTestId(/^project-card-/)).toHaveLength(1));
  const card = screen.getByTestId("project-card-" + source.id);
  expect(card.textContent).toContain("WebCodex");
  expect(card.textContent).toContain("2 workspaces");
  expect(card.textContent).toContain("1 worktrees");

  expect(await screen.findByText("Primary workspace")).toBeTruthy();
  expect(screen.getByText("webcodex-activity-fix")).toBeTruthy();
  const activity = await screen.findByRole("button", { name: /apply_text_edits/ });
  expect(activity.textContent).toContain("webcodex-activity-fix");
  fireEvent.click(activity);
  expect(openWindow).toHaveBeenCalledWith(key);
});
