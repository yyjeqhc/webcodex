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
    if (path === "window-collaboration") return ok({ messages: [], can_send: true });
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
  expect(activeRow.textContent).toContain("apply_text_edits");
  expect(activeRow.textContent).toContain("WebCodex");
  expect(activeRow.textContent).toContain("webcodex-activity-fix");
  expect(activeRow.textContent).toContain("1 active");
  expect(activeRow.textContent).toContain("special");
  expect(activeRow.textContent).toContain("/root/.webcodex-managed-worktrees/webcodex-activity-fix");
  expect(activeRow.textContent).not.toContain(activeKey.slice(0, 8));
  expect(within(activeRow).getByText("webcodex-activity-fix")).toBeTruthy();
  const projectPicker = screen.getByRole("button", { name: "Projects: All projects" });
  fireEvent.click(projectPicker);
  const projectDialog = screen.getByRole("dialog", { name: "Projects" });
  expect(projectDialog.textContent).toContain("2 Windows");
  expect(projectDialog.textContent).toContain("1 active");
  fireEvent.click(projectPicker);
  expect(await screen.findByRole("heading", { name: "webcodex-activity-fix" })).toBeTruthy();
  const header = screen.getByRole("heading", { name: "webcodex-activity-fix" }).closest("header");
  expect(header?.textContent).toContain("WebCodex");
  expect(header?.textContent).toContain("apply_text_edits");
  expect(header?.textContent).toContain("Machine");
  expect(header?.textContent).toContain("special");
  expect(header?.textContent).toContain("Directory");
  expect(header?.textContent).toContain("/root/.webcodex-managed-worktrees/webcodex-activity-fix");
  expect(header?.textContent).toContain("Project address");
  expect(header?.textContent).toContain(worktree.id);
  expect(header?.textContent).toContain(activeKey);
  expect(screen.getByRole("button", { name: "Copy Window" })).toBeTruthy();

  expect(screen.queryByText("Each call is shown separately, from first to last.")).toBeNull();
  expect(screen.queryByRole("heading", { name: "Tool calls" })).toBeNull();
  expect(screen.getAllByText("runtime_status").length).toBeGreaterThan(0);
  expect(screen.queryByText("Inspect Runtime status")).toBeNull();
  expect(screen.queryByText("Observe")).toBeNull();
  expect(screen.queryByText("No explicit Session link")).toBeNull();
  expect(screen.queryByRole("complementary", { name: "Window context" })).toBeNull();
  expect(screen.queryByRole("tab", { name: "Window collaboration" })).toBeNull();
  expect(screen.queryByText("Technical details")).toBeNull();
  expect(screen.getAllByTitle(absoluteTime(observeStarted)).length).toBeGreaterThan(0);

  const search = screen.getByRole("searchbox", { name: "Search Windows" });
  fireEvent.change(search, { target: { value: "runtime_status" } });
  expect(screen.getByTestId("work-window-row-" + activeKey).closest(".window-current-selection")).toBeTruthy();
  expect(screen.getByTestId("work-window-row-" + observeKey)).toBeTruthy();
  expect(vi.mocked(client.post).mock.calls.some(([path]) => path === "window-collaboration")).toBe(false);
  fireEvent.click(screen.getByRole("tab", { name: "Collaboration" }));
  await waitFor(() => expect(vi.mocked(client.post).mock.calls.some(([path]) => path === "window-collaboration")).toBe(true));
});

it("uses exact Session tags to focus contiguous Window call segments", async () => {
  const [source, worktree] = projectFamily();
  const windowKey = "d".repeat(64);
  const sessionA = "wc_sess_aaaaaaaaaaaaaaaa";
  const sessionB = "wc_sess_bbbbbbbbbbbbbbbb";
  const client = fakeClient((path, payload) => {
    if (path === "windows") return ok({
      windows: [{
        client_window_key: windowKey,
        last_project: worktree.id,
        source: "openai-session",
        last_seen_at_ms: 1_790_000_200_000,
        last_activity_name: "apply_text_edits",
        last_activity_status: "success",
        last_activity_meaningful: true,
        active_count: 0,
        linked_session_count: 2,
        recorder_gap_count: 0,
      }],
      returned: 1,
      total: 1,
      truncated: false,
      visibility: { scope: "principal" },
    });
    if (path === "window") return ok(windowDetail({
      client_window_key: windowKey,
      last_seen_at_ms: 1_790_000_200_000,
      linked_sessions: [
        {
          workflow_session_id: sessionA,
          project: worktree.id,
          first_linked_at_ms: 1_790_000_100_000,
          last_linked_at_ms: 1_790_000_150_000,
          relations: ["recording"],
          relation_count: 2,
          title: "Implement Window tabs",
          lifecycle: "active",
        },

      ],
      sessions_returned: 1,
      sessions_truncated: true,
      activity: [
        {
          started_at_ms: 1_790_000_100_000,
          ended_at_ms: 1_790_000_101_000,
          duration_ms: 1000,
          method: "tools/call",
          tool_name: "read_files",
          project: worktree.id,
          status: "success",
          meaningful: true,
          server_trace_id: "trace-session-a-1",
          workflow_sessions: [{
            workflow_session_id: sessionA,
            project: worktree.id,
            relation: "recording",
          }],
        },
        {
          started_at_ms: 1_790_000_120_000,
          ended_at_ms: 1_790_000_121_000,
          duration_ms: 1000,
          method: "tools/call",
          tool_name: "run_shell",
          project: worktree.id,
          status: "success",
          meaningful: true,
          server_trace_id: "trace-session-a-context",
          workflow_sessions: [],
        },
        {
          started_at_ms: 1_790_000_140_000,
          ended_at_ms: 1_790_000_141_000,
          duration_ms: 1000,
          method: "tools/call",
          tool_name: "apply_text_edits",
          project: worktree.id,
          status: "success",
          meaningful: true,
          server_trace_id: "trace-session-b",
          workflow_sessions: [{
            workflow_session_id: sessionB,
            project: worktree.id,
            relation: "work_on_project",
          }],
        },
        {
          started_at_ms: 1_790_000_160_000,
          ended_at_ms: 1_790_000_161_000,
          duration_ms: 1000,
          method: "tools/call",
          tool_name: "runtime_status",
          project: worktree.id,
          status: "success",
          meaningful: false,
          server_trace_id: "trace-session-b-context",
          workflow_sessions: [],
        },
        {
          started_at_ms: 1_790_000_180_000,
          ended_at_ms: 1_790_000_181_000,
          duration_ms: 1000,
          method: "tools/call",
          tool_name: "read_files",
          project: worktree.id,
          status: "success",
          meaningful: true,
          server_trace_id: "trace-session-a-2",
          workflow_sessions: [{
            workflow_session_id: sessionA,
            project: worktree.id,
            relation: "recording",
          }],
        },
      ],
      activity_returned: 5,
    }));
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

  expect(await screen.findByRole("tab", { name: /^Window/ })).toBeTruthy();
  expect(screen.queryByRole("tab", { name: /Work Sessions/ })).toBeNull();
  expect(screen.getByRole("tab", { name: "Collaboration" })).toBeTruthy();

  const selector = await screen.findByRole("combobox", { name: "Session filter" }) as HTMLSelectElement;
  expect(selector.value).toBe("");
  expect(screen.getAllByTestId("window-workflow-step")).toHaveLength(5);

  const tags = await screen.findAllByTestId("window-session-tag");
  expect(tags).toHaveLength(3);
  expect(tags[0].getAttribute("data-session-tone")).toBe("0");
  expect(tags[1].getAttribute("data-session-tone")).toBe("1");
  expect(tags[2].getAttribute("data-session-tone")).toBe("0");

  fireEvent.click(tags[1]);
  expect(selector.value).toBe(sessionB);
  let focusedCalls = screen.getAllByTestId("window-workflow-step");
  expect(focusedCalls).toHaveLength(2);
  expect(focusedCalls.map((call) => call.querySelector("header strong")?.textContent)).toEqual(["apply_text_edits", "runtime_status"]);

  fireEvent.change(selector, { target: { value: "" } });
  expect(screen.getAllByTestId("window-workflow-step")).toHaveLength(5);

  const allCalls = screen.getAllByTestId("window-workflow-step");
  fireEvent.click(allCalls[0]);
  expect(selector.value).toBe(sessionA);
  focusedCalls = screen.getAllByTestId("window-workflow-step");
  expect(focusedCalls).toHaveLength(3);
  expect(focusedCalls.map((call) => call.querySelector("header strong")?.textContent)).toEqual(["read_files", "run_shell", "read_files"]);

  fireEvent.change(selector, { target: { value: sessionB } });
  focusedCalls = screen.getAllByTestId("window-workflow-step");
  expect(focusedCalls).toHaveLength(2);
  expect((client.post as unknown as ReturnType<typeof vi.fn>).mock.calls.some(([path]) =>
    path === "workflow-session" || path === "workflow-session-messages"
  )).toBe(false);
});

it("shows background Job identity on handoff calls and observe_jobs", async () => {
  const [source, worktree] = projectFamily();
  const windowKey = "e".repeat(64);
  const jobId = "wc_job_background_123456";
  const client = fakeClient((path, payload) => {
    if (path === "windows") return ok({
      windows: [{
        client_window_key: windowKey,
        last_project: worktree.id,
        source: "openai-session",
        last_seen_at_ms: 1_790_000_300_000,
        last_activity_name: "observe_jobs",
        last_activity_status: "success",
        last_activity_meaningful: false,
        active_count: 0,
        linked_session_count: 0,
        recorder_gap_count: 0,
      }],
      returned: 1,
      total: 1,
      truncated: false,
      visibility: { scope: "principal" },
    });
    if (path === "window") return ok(windowDetail({
      client_window_key: windowKey,
      last_seen_at_ms: 1_790_000_300_000,
      activity: [
        {
          started_at_ms: 1_790_000_200_000,
          ended_at_ms: 1_790_000_201_000,
          duration_ms: 1000,
          method: "tools/call",
          tool_name: "cargo_test",
          project: worktree.id,
          status: "success",
          meaningful: true,
          server_trace_id: "trace-job-handoff",
          async_job_id: jobId,
          observed_job_ids: [],
          workflow_sessions: [],
        },
        {
          started_at_ms: 1_790_000_250_000,
          ended_at_ms: 1_790_000_251_000,
          duration_ms: 1000,
          method: "tools/call",
          tool_name: "observe_jobs",
          project: worktree.id,
          status: "success",
          meaningful: false,
          server_trace_id: "trace-job-observe",
          observed_job_ids: [jobId],
          workflow_sessions: [],
        },
      ],
      activity_returned: 2,
      jobs: [{
        job_id: jobId,
        status: "running",
        active: true,
        terminal: false,
        elapsed_secs: 90,
      }],
      jobs_truncated: false,
    }));
    throw new Error("unexpected path " + path + " " + JSON.stringify(payload));
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

  const jobTags = await screen.findAllByTitle(jobId);
  expect(jobTags).toHaveLength(2);
  expect(jobTags[0].textContent).toContain("Background running");
  expect(jobTags[0].textContent).toContain("1m 30s");
  expect(jobTags[1].textContent).toContain("Observing");
  expect(screen.getAllByTestId("window-workflow-step")).toHaveLength(2);
});

it("renders Window identity from inventory before recent activity and hydrates full history later", async () => {
  const [source, worktree] = projectFamily();
  const key = "f".repeat(64);
  const startedAt = 1_790_000_400_000;
  let resolvePrimary!: (value: ReturnType<typeof ok>) => void;
  let resolveFull!: (value: ReturnType<typeof ok>) => void;
  const primaryResponse = new Promise<ReturnType<typeof ok>>((resolve) => { resolvePrimary = resolve; });
  const fullResponse = new Promise<ReturnType<typeof ok>>((resolve) => { resolveFull = resolve; });
  const windowPayloads: any[] = [];

  const recentActivity = {
    started_at_ms: startedAt,
    ended_at_ms: startedAt + 20,
    duration_ms: 20,
    method: "tools/call",
    tool_name: "read_files",
    project: worktree.id,
    status: "success",
    meaningful: true,
    server_trace_id: "trace-progressive-recent",
    workflow_sessions: [],
  };
  const olderActivity = {
    ...recentActivity,
    started_at_ms: startedAt - 10_000,
    ended_at_ms: startedAt - 9_000,
    duration_ms: 1000,
    tool_name: "run_shell",
    server_trace_id: "trace-progressive-old",
  };

  const client = fakeClient((path, payload) => {
    if (path === "windows") return ok({
      windows: [{
        client_window_key: key,
        last_project: worktree.id,
        source: "openai-session",
        last_seen_at_ms: startedAt,
        first_seen_at_ms: startedAt - 3_600_000,
        last_activity_name: "read_files",
        last_activity_status: "success",
        last_activity_meaningful: true,
        active_count: 0,
        linked_session_count: 0,
        recorder_gap_count: 0,
      }],
      returned: 1,
      total: 1,
      truncated: false,
      visibility: { scope: "principal" },
    });
    if (path === "window") {
      windowPayloads.push(payload);
      return payload.detail_level === "primary" ? primaryResponse : fullResponse;
    }
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

  // Window inventory already carries enough human identity to make selection
  // feel instant; the detail endpoint is still pending here.
  expect(await screen.findByRole("heading", { name: "webcodex-activity-fix" })).toBeTruthy();
  const header = screen.getByRole("heading", { name: "webcodex-activity-fix" }).closest("header");
  expect(header?.textContent).toContain("special");
  expect(header?.textContent).toContain("/root/.webcodex-managed-worktrees/webcodex-activity-fix");
  expect(header?.textContent).toContain("First active");
  expect(screen.getByText("Loading recent activity…")).toBeTruthy();
  await waitFor(() => expect(windowPayloads).toHaveLength(1));
  expect(windowPayloads[0]).toMatchObject({
    client_window_key: key,
    activity_limit: 80,
    detail_level: "primary",
  });

  resolvePrimary(ok(windowDetail({
    client_window_key: key,
    detail_level: "primary",
    last_seen_at_ms: startedAt + 20,
    activity: [recentActivity],
    activity_returned: 1,
    activity_truncated: true,
  })));

  expect(await screen.findByText("read_files")).toBeTruthy();
  expect(await screen.findByText("Loading history…")).toBeTruthy();
  await waitFor(() => expect(windowPayloads).toHaveLength(2));
  expect(windowPayloads[1]).toEqual({ client_window_key: key, activity_limit: 2_000 });

  resolveFull(ok(windowDetail({
    client_window_key: key,
    detail_level: "full",
    last_seen_at_ms: startedAt + 20,
    activity: [recentActivity, olderActivity],
    activity_returned: 2,
    activity_truncated: false,
  })));

  expect(await screen.findByText("run_shell")).toBeTruthy();
  await waitFor(() => expect(screen.queryByText("Loading history…")).toBeNull());
  expect(screen.getAllByTestId("window-workflow-step")).toHaveLength(2);
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
  await waitFor(() => expect(card.textContent).toContain("1 active"));

  expect((await screen.findAllByText("Primary workspace"))[0]).toBeTruthy();
  expect(screen.getByText("webcodex-activity-fix")).toBeTruthy();
  const activity = await screen.findByRole("button", { name: /apply_text_edits/ });
  expect(activity.textContent).toContain("webcodex-activity-fix");
  fireEvent.click(activity);
  expect(openWindow).toHaveBeenCalledWith(key);
});
