import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RUNTIME_CREDENTIAL_SESSION_KEY } from "../src/runtime_storage.js";
import { App } from "../src/runtime-v2/App.js";
import { runtimeOverview, sessionDetail, sessionItem, windowDetail } from "./fixtures.js";

function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function installFetch(locateResponse?: () => Promise<Response>) {
  const overview = runtimeOverview();
  vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const body = init?.body ? JSON.parse(String(init.body)) : {};
    if (url.endsWith("/api/runtime-console/overview")) return json(overview);
    if (url.endsWith("/api/runtime-console/goals")) return json({
      goals: [{
        goal_id: "wc_goal_1234567890abcdef",
        title: "Runtime V2 Goal Workbench",
        lifecycle: "active",
        revision: 3,
        updated_at_unix_ms: 1_790_000_000_000,
        agent_task_count: 0,
        workflow_session_count: 1,
        total_step_count: 3,
        completed_step_count: 1,
        current_step_id: "implement",
        current_step_title: "Implement Goal Workbench",
        project_ids: ["agent:special:webcodex"],
      }],
      total: 1,
      source_total: 1,
      truncated: false,
    });
    if (url.endsWith("/api/runtime-console/goal")) return json({
      goal: {
        summary: { goal_id: body.goal_id, title: "Runtime V2 Goal Workbench", lifecycle: "active", revision: 3, created_at_unix_ms: 1_789_999_000_000, updated_at_unix_ms: 1_790_000_000_000, terminal_at_unix_ms: null, agent_task_count: 0, workflow_session_count: 1 },
        plan: { completion_conditions: ["dogfood"], steps: [{ id: "survey", title: "Survey", status: "completed", updated_at_unix_ms: 1_789_999_500_000 }, { id: "implement", title: "Implement Goal Workbench", status: "in_progress", updated_at_unix_ms: 1_790_000_000_000 }, { id: "validate", title: "Validate", status: "pending", updated_at_unix_ms: 1_790_000_000_000 }], progress_summary: "Implementing", checkpoint_at_unix_ms: 1_790_000_000_000 },
        objective: "Make Goal the durable work truth.", controller_agent_id: null, terminal_reason: null, correlations: [{ kind: "workflow_session", reference_id: "wc_sess_1234567890abcdef", created_at_unix_ms: 1_789_999_600_000 }],
      },
      goal_plan: { version: 3, goal_id: body.goal_id, title: "Runtime V2 Goal Workbench", total_step_count: 3, completed_step_count: 1, current_step_id: "implement", steps: [{ id: "survey", title: "Survey", status: "completed", updated_at_unix_ms: 1_789_999_500_000 }, { id: "implement", title: "Implement Goal Workbench", status: "in_progress", updated_at_unix_ms: 1_790_000_000_000 }, { id: "validate", title: "Validate", status: "pending", updated_at_unix_ms: 1_790_000_000_000 }], progress_summary: "Implementing", checkpoint_at_unix_ms: 1_790_000_000_000, controller_agent_id: null, lifecycle: "active", revision: 3, updated_at_unix_ms: 1_790_000_000_000, terminal_at_unix_ms: null, agent_task_count: 0, workflow_session_count: 1, activity: { available: true, state: "active", idle_threshold_ms: 300000, observation_lease_ms: 60000, linked_window_count: 0, active_meaningful_request_count: 0, coverage_partial: false }, continuity: { available: true, state: "not_configured", production_auto_resume_available: false, host_delivery: "not_applicable", fresh_turn: "not_applicable" } },
      projects: overview.projects,
      sessions: [], tasks: [], waits: [], waits_truncated: false, agents: [], windows: [],
    });
    if (url.endsWith("/api/runtime-console/workflow-session-locate") && locateResponse) return await locateResponse();
    if (url.endsWith("/api/runtime-console/project-git")) return json({ branch: "prototype/runtime-webui-v2", clean: false, git_available: true });
    if (url.endsWith("/api/runtime-console/workflow-session-messages")) return json({ session_id: body.session_id, messages: [] });
    if (url.endsWith("/api/runtime-console/workflow-session")) return json(sessionDetail({ session_id: body.session_id }));
    if (url.endsWith("/api/runtime-console/projects")) return json({ projects: overview.projects, total: 1, truncated: false });
    if (url.endsWith("/api/runtime-console/workflow-sessions")) return json({ sessions: [sessionItem()], total: 1, returned: 1, truncated: false });
    if (url.endsWith("/api/runtime-console/windows")) return json({
      windows: [{ client_window_key: "a".repeat(64), last_project: "agent:special:webcodex", source: "openai-session", last_seen_at_ms: 1_790_000_000_000, active_count: 0, linked_session_count: 1, recorder_gap_count: 0 }],
      returned: 1,
      total: 1,
      truncated: false,
      visibility: { scope: "principal" },
    });
    if (url.endsWith("/api/runtime-console/window")) return json(windowDetail({ client_window_key: body.client_window_key }));
    if (url.endsWith("/api/runtime-console/communication/agents")) return json({ agents: [], total: 0, returned: 0 });
    if (url.endsWith("/api/runtime-console/communication/conversations")) return json({ conversations: [], total: 0, returned: 0 });
    throw new Error("unexpected fetch " + url);
  }));
}

describe("Runtime v2 navigation", () => {
  beforeEach(() => {
    window.sessionStorage.setItem(RUNTIME_CREDENTIAL_SESSION_KEY, "test-token");
    installFetch();
  });

  it("keeps Work / Projects / Runtime as the only primary destinations", async () => {
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Runtime V2 Goal Workbench" })).toBeTruthy();

    const primary = screen.getAllByRole("navigation", { name: "Workspace views" })[0];
    expect(primary.textContent).toContain("Work");
    expect(primary.textContent).toContain("Projects");
    expect(primary.textContent).toContain("Runtime");
    expect(primary.textContent).not.toContain("Workflow Sessions");
    expect(primary.textContent).not.toContain("Window Activity");
    expect(screen.getByRole("tab", { name: /Goals/ }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tab", { name: /Sessions/ })).toBeTruthy();

    fireEvent.click(screen.getAllByRole("button", { name: /Projects/ })[0]);
    expect(await screen.findByRole("heading", { name: "Projects" })).toBeTruthy();

    fireEvent.click(screen.getAllByRole("button", { name: /Runtime/ })[0]);
    expect(await screen.findByRole("heading", { name: "Runtime" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: /Window Activity/ })).toBeTruthy();
    expect(screen.getByRole("tab", { name: /Agents/ })).toBeTruthy();
  });

  it("keeps the three primary destinations reachable in a narrow viewport", async () => {
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 640 });
    render(<App />);
    await waitFor(() => expect(screen.getAllByRole("navigation", { name: "Workspace views" }).length).toBe(2));
    const navigationRegions = screen.getAllByRole("navigation", { name: "Workspace views" });
    const mobile = navigationRegions.at(-1)!;
    expect(mobile.textContent).toContain("Work");
    expect(mobile.textContent).toContain("Projects");
    expect(mobile.textContent).toContain("Runtime");
  });

  it("cannot apply a delayed exact Session lookup after the workspace is locked", async () => {
    let resolveLocate!: (response: Response) => void;
    const pendingLocate = new Promise<Response>((resolve) => { resolveLocate = resolve; });
    installFetch(() => pendingLocate);
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Runtime V2 Goal Workbench" })).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: /Sessions/ }));

    const exact = "wc_sess_abcdef0123456789";
    const search = screen.getByRole("textbox", { name: "Search Sessions" });
    fireEvent.change(search, { target: { value: exact } });
    fireEvent.keyDown(search, { key: "Enter" });
    await waitFor(() => expect((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.some(([url]) => String(url).endsWith("/api/runtime-console/workflow-session-locate"))).toBe(true));

    fireEvent.click(screen.getByRole("button", { name: "Lock" }));
    expect(await screen.findByRole("heading", { name: "Connect to your workspace" })).toBeTruthy();

    resolveLocate(json({
      ...sessionDetail({ session_id: exact, title: "stale credential Session" }),
      client_id: "special",
      project_id: "agent:special:stale",
      project_name: "Stale project",
    }));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByText("stale credential Session")).toBeNull();
    expect(screen.getByRole("heading", { name: "Connect to your workspace" })).toBeTruthy();
  });
});
