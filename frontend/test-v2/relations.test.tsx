import { fireEvent, render as testingRender, screen, waitFor, within } from "@testing-library/react";
import type { ReactElement } from "react";
import { describe, expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { workItemFromRecent } from "../src/runtime-v2/model/work.js";
import { ProjectsView } from "../src/runtime-v2/views/ProjectsView.js";
import { RuntimeView } from "../src/runtime-v2/views/RuntimeView.js";
import { WindowWorkbench } from "../src/runtime-v2/components/WindowWorkbench.js";
import { WindowActivityFeed } from "../src/runtime-v2/components/WindowActivityFeed.js";
import { WorkView } from "../src/runtime-v2/views/WorkView.js";
import { UiProvider } from "../src/ui/UiProvider.js";
import { recentSession, runtimeOverview, sessionDetail, sessionItem, windowDetail } from "./fixtures.js";

const render = (ui: ReactElement) => testingRender(<UiProvider>{ui}</UiProvider>);

function fakeClient(handler: (path: string, payload: any) => any): RuntimeV2Client {
  return {
    post: vi.fn(async (path: string, payload: any) => handler(path, payload)),
    postAt: vi.fn(async (base: string, path: string, payload: any) => handler(base + path, payload)),
  } as unknown as RuntimeV2Client;
}

function ok(data: unknown) {
  return { ok: true, status: 200, data };
}

describe("Project / Session / Window relationships", () => {
  it("renders active Sessions without fetching per-Session or Git details", async () => {
    const sessions = [
      sessionItem({ session_id: "wc_sess_1111111111111111", title: "Runtime E2E hardening", running_jobs: 1 }),
      sessionItem({ session_id: "wc_sess_2222222222222222", title: "WebUI v2 rewrite", running_call: true }),
    ];
    const overview = runtimeOverview();
    const client = fakeClient((path, payload) => {
      if (path === "projects") return ok({ projects: overview.projects, total: 1, truncated: false });
      if (path === "project-git") return ok({ branch: "prototype/runtime-webui-v2", clean: false, git_available: true });
      if (path === "workflow-sessions") return ok({ sessions, total: 2, returned: 2, truncated: false });
      if (path === "windows") return ok({ windows: [], returned: 0, total: 0, truncated: false, visibility: { scope: "principal" } });
      if (path === "workflow-session") {
        const count = payload.session_id.endsWith("1111111111111111") ? 2 : 1;
        return ok(sessionDetail({
          ...sessions.find((session) => session.session_id === payload.session_id),
          linked_windows: Array.from({ length: count }, (_, index) => ({
            client_window_key: String(index + 1).repeat(64),
            source: "openai-session",
            first_linked_at_ms: 1_790_000_000_000,
            last_linked_at_ms: 1_790_000_000_000,
            last_seen_at_ms: 1_790_000_000_000,
            relations: ["recording"],
            relation_count: 1,
            recorder_gap_count: 0,
          })),
        }));
      }
      throw new Error("unexpected path " + path);
    });

    render(
      <ProjectsView
        client={client}
        language="en"
        runners={overview.runners}
        onOpenSession={vi.fn()}
        onOpenWindow={vi.fn()}
        onUnauthorized={vi.fn()}
      />,
    );

    expect(await screen.findByText("Runtime E2E hardening")).toBeTruthy();
    expect(screen.getByText("WebUI v2 rewrite")).toBeTruthy();
    expect(vi.mocked(client.post).mock.calls.map(([path]) => path).sort()).toEqual(["projects", "windows", "workflow-sessions"]);
    fireEvent.click(screen.getByRole("button", { name: "Check branch" }));
    expect(await screen.findByText("prototype/runtime-webui-v2")).toBeTruthy();
    expect(vi.mocked(client.post).mock.calls.filter(([path]) => path === "project-git")).toHaveLength(1);
  });

  it("loads the selected Project's retained Sessions when its card is opened", async () => {
    const overview = runtimeOverview();
    const first = { ...overview.projects[0], id: "agent:special:first", name: "First Project", project_ref: "~p1" };
    const second = { ...overview.projects[0], id: "agent:special:second", name: "Second Project", project_ref: "~p2" };
    const firstSession = sessionItem({ session_id: "wc_sess_aaaaaaaaaaaaaaaa", title: "First project Session" });
    const secondSession = sessionItem({ session_id: "wc_sess_bbbbbbbbbbbbbbbb", title: "Second project Session" });
    const handler = vi.fn((path: string, payload: any) => {
      if (path === "projects") return ok({ projects: [first, second], total: 2, truncated: false });
      if (path === "project-git") return ok({ branch: "main", clean: true, git_available: true });
      if (path === "workflow-sessions") {
        const sessions = payload.project === second.id ? [secondSession] : [firstSession];
        return ok({ sessions, total: 1, returned: 1, truncated: false });
      }
      if (path === "windows") return ok({ windows: [], returned: 0, total: 0, truncated: false, visibility: { scope: "principal" } });
      if (path === "workflow-session") return ok(sessionDetail({ session_id: payload.session_id, linked_windows: [] }));
      throw new Error("unexpected path " + path);
    });
    const client = fakeClient(handler);

    render(
      <ProjectsView client={client} language="en" runners={overview.runners} onOpenSession={vi.fn()} onOpenWindow={vi.fn()} onUnauthorized={vi.fn()} />,
    );
    expect(await screen.findByText("First project Session")).toBeTruthy();

    fireEvent.click(screen.getByTestId("project-card-" + second.id));
    expect(await screen.findByText("Second project Session")).toBeTruthy();
    expect(handler.mock.calls.some(([path, payload]) => path === "workflow-sessions" && payload.project === second.id)).toBe(true);
  });

  it("keeps Add Project a single non-replayed write after an uncertain result", async () => {
    const overview = runtimeOverview();
    const handler = vi.fn((path: string) => {
      if (path === "projects") return ok({ projects: overview.projects, total: 1, truncated: false });
      if (path === "project-git") return ok({ branch: "main" });
      if (path === "workflow-sessions") return ok({ sessions: [], total: 0, returned: 0, truncated: false });
      if (path === "windows") return ok({ windows: [], returned: 0, total: 0, truncated: false, visibility: { scope: "principal" } });
      if (path === "/api/projects/resolve-or-register") return { ok: false, status: 0, data: null };
      throw new Error("unexpected path " + path);
    });
    const client = fakeClient(handler);

    render(
      <ProjectsView client={client} language="en" runners={overview.runners} onOpenSession={vi.fn()} onOpenWindow={vi.fn()} onUnauthorized={vi.fn()} />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Add Project" }));
    fireEvent.change(await screen.findByPlaceholderText("Absolute folder path on the selected Runner"), {
      target: { value: "/root/git/new-project" },
    });
    fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Add Project" }));
    await screen.findByText("The result could not be confirmed. Refresh Projects before trying again.");
    expect(handler.mock.calls.filter(([path]) => path === "/api/projects/resolve-or-register")).toHaveLength(1);
  });

  it("renders Window calls without Session relationships or technical disclosures", async () => {
    const overview = runtimeOverview();
    const firstKey = "a".repeat(64);
    const secondKey = "b".repeat(64);
    const detail = windowDetail({
      client_window_key: firstKey,
      activity: [{
        started_at_ms: 1_790_000_100_000,
        ended_at_ms: 1_790_000_100_120,
        duration_ms: 120,
        service_ms: 90,
        method: "tools/call",
        tool_name: "read_files",
        activity_presentation: "Read Runtime source",
        activity_kind: "exploration",
        project: "agent:special:webcodex",
        status: "success",
        meaningful: true,
        server_trace_id: "trace-window-workflow-1",
        workflow_sessions: [{ workflow_session_id: "wc_sess_1111111111111111", project: "agent:special:webcodex", relation: "recording" }],
      }],
      activity_returned: 1,
      linked_sessions: [
        {
          workflow_session_id: "wc_sess_1111111111111111",
          project: "agent:special:webcodex",
          first_linked_at_ms: 1_790_000_000_000,
          last_linked_at_ms: 1_790_000_000_000,
          relations: ["recording"],
          relation_count: 1,
          title: "Runtime E2E hardening",
          lifecycle: "active",
        },
        {
          workflow_session_id: "wc_sess_2222222222222222",
          project: "agent:special:webcodex",
          first_linked_at_ms: 1_790_000_000_000,
          last_linked_at_ms: 1_790_000_000_000,
          relations: ["work_on_project"],
          relation_count: 1,
          title: "WebUI v2 rewrite",
          lifecycle: "active",
        },
      ],
      sessions_returned: 2,
    });
    const client = fakeClient((path, payload) => {
      if (path === "windows") return ok({
        windows: [
          { client_window_key: firstKey, last_project: "agent:special:webcodex", source: "openai-session", last_seen_at_ms: 1_790_000_000_000, active_count: 0, linked_session_count: 2, recorder_gap_count: 0 },
          { client_window_key: secondKey, last_project: "agent:special:webcodex", source: "openai-session", last_seen_at_ms: 1_790_000_000_000, active_count: 0, linked_session_count: 0, recorder_gap_count: 0 },
        ],
        returned: 2,
        total: 2,
        truncated: false,
        visibility: { scope: "principal" },
      });
      if (path === "window") return ok(payload.client_window_key === firstKey ? detail : windowDetail({ client_window_key: secondKey }));
      if (path === "workflow-session") {
        const count = payload.session_id.endsWith("1111111111111111") ? 2 : 3;
        return ok(sessionDetail({
          session_id: payload.session_id,
          linked_windows: Array.from({ length: count }, (_, index) => ({
            client_window_key: String(index).padStart(64, "c"),
            source: "openai-session",
            first_linked_at_ms: 1,
            last_linked_at_ms: 1,
            last_seen_at_ms: 1,
            relations: ["recording"],
            relation_count: 1,
            recorder_gap_count: 0,
          })),
        }));
      }
      if (path === "communication/agents") return { ok: false, status: 403, data: null };
      throw new Error("unexpected path " + path);
    });

    render(
      <WindowWorkbench client={client} language="en" projects={overview.projects}
        surface="windows" onSurfaceChange={vi.fn()} onUnauthorized={vi.fn()} />,
    );

    fireEvent.click(await screen.findByTestId("work-window-row-" + firstKey));
    expect((await screen.findAllByText("read_files")).length).toBeGreaterThan(0);
    const workflowStep = screen.getByTestId("window-workflow-step");
    expect(within(workflowStep).getByText("/root/git/webcodex")).toBeTruthy();
    expect(within(workflowStep).getByText("120ms")).toBeTruthy();
    expect(within(workflowStep).getByText("Succeeded")).toBeTruthy();
    expect(screen.queryByText("Technical details")).toBeNull();
    expect(screen.queryByText("Linked Sessions")).toBeNull();
    expect(screen.queryByText("Runtime E2E hardening")).toBeNull();
    expect((client.post as ReturnType<typeof vi.fn>).mock.calls.some(([path]) => path === "workflow-session")).toBe(false);

    fireEvent.click(screen.getByTestId("work-window-row-" + secondKey));
    expect(await screen.findByText("No tool calls yet")).toBeTruthy();
  });

  it("renders Session Evidence -> multiple Windows and handles Session with no Window", async () => {
    const overview = runtimeOverview();
    const recent = recentSession({
      title: "A very long Session title ".repeat(8),
    });
    const item = workItemFromRecent(recent);
    const withWindows = sessionDetail({
      title: recent.title,
      linked_windows: [
        {
          client_window_key: "1".repeat(64),
          source: "openai-session",
          first_linked_at_ms: 1,
          last_linked_at_ms: 2,
          last_seen_at_ms: 2,
          relations: ["recording"],
          relation_count: 1,
          recorder_gap_count: 0,
        },
        {
          client_window_key: "2".repeat(64),
          source: "openai-session",
          first_linked_at_ms: 1,
          last_linked_at_ms: 2,
          last_seen_at_ms: 2,
          relations: ["work_on_project"],
          relation_count: 1,
          recorder_gap_count: 0,
        },
      ],
    });
    let detail = withWindows;
    const client = fakeClient((path) => {
      if (path === "workflow-session") return ok(detail);
      if (path === "workflow-session-messages") return ok({ session_id: recent.session_id, messages: [] });
      if (path === "project-git") return ok({ branch: "main" });
      throw new Error("unexpected path " + path);
    });

    const props = {
      client,
      items: [item],
      selected: {
        projectId: recent.project_id,
        projectName: recent.project_name || recent.project_id,
        runner: recent.client_id,
        sessionId: recent.session_id,
      },
      projects: overview.projects,
      language: "en" as const,
      inventoryIncomplete: false,
      surface: "session" as const,
      onSurfaceChange: vi.fn(),
      onOpenSession: vi.fn(),
      onLocateSession: vi.fn(async () => false),
      onUnauthorized: vi.fn(),
    };
    const rendered = render(<WorkView {...props} />);
    expect(await screen.findByText("/root/git/webcodex")).toBeTruthy();
    fireEvent.click(await screen.findByRole("tab", { name: "Evidence" }));
    const sessionContext = within(screen.getByRole("complementary", { name: "Session context" }));
    expect(sessionContext.getByText("1".repeat(64))).toBeTruthy();
    expect(sessionContext.getByText("2".repeat(64))).toBeTruthy();
    expect(sessionContext.getAllByRole("button", { name: "Copy Window" })).toHaveLength(2);
    expect(screen.getAllByText(/A very long Session title A very long Session title/).length).toBeGreaterThan(0);

    rendered.unmount();
    detail = sessionDetail({ title: recent.title, linked_windows: [] });
    render(<WorkView {...props} />);
    fireEvent.click(await screen.findByRole("tab", { name: "Evidence" }));
    expect(await screen.findByText("No linked Windows in retained evidence.")).toBeTruthy();
  });

  it("keeps build diagnostics collapsed and defers Runtime inventories until requested", () => {
    const base = runtimeOverview();
    const overview = runtimeOverview({
      runners: [{
        ...base.runners[0],
        version: "0.4.1",
        build_alignment: "different_version",
        build_git_commit: "f080c8f3ea700000000000000000000000000000",
        protocol_compatibility: "compatible",
      }],
    });
    const client = fakeClient((path) => {
      if (path === "windows") return { ok: false, status: 403, data: null };
      if (path === "communication/agents") return { ok: false, status: 403, data: null };
      throw new Error("unexpected path " + path);
    });

    const { container } = render(
      <RuntimeView
        client={client}
        language="en"
        overview={overview}
        overviewAvailability="available"
        onOpenWork={vi.fn()}
        onUnauthorized={vi.fn()}
      />,
    );

    const build = container.querySelector(".runtime-row-build");
    expect(build).toBeTruthy();
    expect(build?.textContent).toContain("Build alignment: Different version");
    expect(container.querySelector(".runtime-diagnostics")?.hasAttribute("open")).toBe(false);
    expect(container.querySelector(".runtime-diagnostics")?.textContent).toContain("f080c8f3ea700000000000000000000000000000");
    expect(client.post).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText("Build diagnostics"));
    expect(container.querySelector(".runtime-diagnostics")?.hasAttribute("open")).toBe(true);
    expect(build?.textContent).not.toContain("different_version");
  });

  it("surfaces Runtime availability truth instead of presenting denied state as connected", () => {
    const overview = runtimeOverview();
    const client = fakeClient((path) => {
      if (path === "windows") return { ok: false, status: 403, data: null };
      if (path === "communication/agents") return { ok: false, status: 403, data: null };
      throw new Error("unexpected path " + path);
    });

    render(
      <RuntimeView
        client={client}
        language="en"
        overview={null}
        overviewAvailability="denied"
        onOpenWork={vi.fn()}
        onUnauthorized={vi.fn()}
      />,
    );

    expect(screen.getAllByText("Runtime overview unavailable").length).toBeGreaterThan(0);
    expect(screen.queryByText(/^connected$/)).toBeNull();
  });

  it("renders all server-returned Window activity without a second client-side cap", async () => {
    const overview = runtimeOverview();
    const key = "d".repeat(64);
    const activity = Array.from({ length: 205 }, (_, index) => ({
      started_at_ms: 1_790_000_000_000 + index,
      ended_at_ms: 1_790_000_000_001 + index,
      duration_ms: 1,
      method: "tools/call",
      tool_name: "tool-" + index,
      activity_presentation: "tool-" + index,
      status: "ok",
      meaningful: true,
      workflow_sessions: [],
    })).reverse();
    const client = fakeClient((path) => {
      if (path === "windows") return ok({
        windows: [{ client_window_key: key, source: "openai-session", last_seen_at_ms: 1_790_000_000_000, active_count: 0, linked_session_count: 0, recorder_gap_count: 0 }],
        returned: 1,
        total: 1,
        truncated: false,
        visibility: { scope: "global" },
      });
      if (path === "window") return ok(windowDetail({
        client_window_key: key,
        activity,
        activity_returned: activity.length,
        activity_truncated: true,
        visibility: { scope: "global" },
      }));
      if (path === "communication/agents") return { ok: false, status: 403, data: null };
      throw new Error("unexpected path " + path);
    });

    render(
      <WindowWorkbench client={client} language="en" projects={overview.projects}
        surface="windows" onSurfaceChange={vi.fn()} onUnauthorized={vi.fn()} />,
    );
    expect((await screen.findAllByText("tool-204")).length).toBeGreaterThan(0);
    expect(screen.getAllByText("tool-0").length).toBeGreaterThan(0);
    const steps = screen.getAllByTestId("window-workflow-step");
    expect(steps).toHaveLength(205);
    expect(steps[0].textContent).toContain("tool-0");
    expect(steps.at(-1)?.textContent).toContain("tool-204");
    expect(screen.queryByRole("button", { name: /Show more activity/ })).toBeNull();
    expect(screen.getByText("Earlier calls are not available in this view. Showing retained activity from oldest to newest.")).toBeTruthy();
  });

  it("stops presenting stale Session detail as current after authority is denied", async () => {
    const overview = runtimeOverview();
    const recent = recentSession();
    const client = fakeClient((path) => {
      if (path === "workflow-session" || path === "workflow-session-messages") return { ok: false, status: 403, data: null };
      if (path === "project-git") return ok({ branch: "main", git_available: true });
      throw new Error("unexpected path " + path);
    });

    render(
      <WorkView
        client={client}
        items={[workItemFromRecent(recent)]}
        selected={{ projectId: recent.project_id, projectName: recent.project_name || recent.project_id, runner: recent.client_id, sessionId: recent.session_id }}
        projects={overview.projects}
        language="en"
        inventoryIncomplete={false}
        surface="session"
        onSurfaceChange={vi.fn()}
        onOpenSession={vi.fn()}
        onLocateSession={vi.fn(async () => false)}
        onUnauthorized={vi.fn()}
      />,
    );

    expect(await screen.findByRole("heading", { name: "Session unavailable" })).toBeTruthy();
    expect(screen.getByText("This Session is no longer visible to the current credential.")).toBeTruthy();
    expect(screen.queryByRole("textbox", { name: "Send a message to this work session…" })).toBeNull();
  });
});


it("shows individual calls in order with exact Project paths and authoritative Session tags", () => {
  const first = runtimeOverview().projects[0];
  const second = { ...first, id: "agent:special:second", path: "/worktrees/second" };
  const base = { started_at_ms: 1_790_000_100_000, ended_at_ms: 1_790_000_100_120, duration_ms: 120, method: "tools/call", status: "success", meaningful: false, workflow_sessions: [] };
  const { container } = render(<WindowActivityFeed language="en" projects={[first, second]} detail={windowDetail({
    active_requests: [
      { server_trace_id: "running", method: "tools/call", tool_name: "run_process", project: second.id, started_at_ms: base.started_at_ms + 500, elapsed_ms: 4200 },
      { server_trace_id: "finished", method: "tools/call", tool_name: "runtime_info", started_at_ms: base.started_at_ms, elapsed_ms: 100 },
    ],
    activity: [
      { ...base, started_at_ms: base.started_at_ms + 300, tool_name: "apply_patch", project: second.id, status: "error" },
      { ...base, tool_name: "runtime_info", activity_presentation: "Inspect runtime", server_trace_id: "finished", workflow_sessions: [{ workflow_session_id: "hidden-session", project: first.id, relation: "recording" }] },
      { ...base, started_at_ms: base.started_at_ms + 100, tool_name: "runtime_info", project: first.id },
    ],
  })} />);
  const calls = screen.getAllByTestId("window-workflow-step");
  expect(calls.map((call) => call.querySelector("header strong")?.textContent)).toEqual(["runtime_info", "runtime_info", "apply_patch", "run_process"]);
  expect(within(calls[0]).queryByTestId("window-project-tag")).toBeNull();
  expect(within(calls[1]).getByTestId("window-project-tag").textContent).toBe(first.path);
  expect(within(calls[2]).getByTestId("window-project-tag").textContent).toBe(second.path);
  expect(within(calls[2]).getByText("Failed")).toBeTruthy();
  expect(within(calls[0]).getByText("Succeeded")).toBeTruthy();
  expect(within(calls[3]).getByText("Running")).toBeTruthy();
  expect(within(calls[3]).getByText("4s")).toBeTruthy();
  expect(calls.slice(0, 3).every((call) => call.querySelector("time[datetime]"))).toBe(true);
  expect(calls[3].querySelector("time[datetime]")).toBeNull();
  expect(calls[3].querySelector(".window-call-live-time")?.textContent).toBe("Running · 4s");
  expect(container.querySelector("details")).toBeNull();
  const sessionTag = screen.getByTestId("window-session-tag");
  expect(sessionTag.textContent).toBe("hidden-session");
  expect(sessionTag.getAttribute("data-session-tone")).toBe("0");
  expect(screen.queryByText("Inspect runtime")).toBeNull();
});
