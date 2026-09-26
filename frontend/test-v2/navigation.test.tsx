import { fireEvent, render as testingRender, screen, waitFor } from "@testing-library/react";
import type { ReactElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RUNTIME_CREDENTIAL_SESSION_KEY } from "../src/runtime_storage.js";
import { App } from "../src/runtime-v2/App.js";
import { UiProvider } from "../src/ui/UiProvider.js";
import { runtimeOverview, sessionDetail, sessionItem, windowDetail } from "./fixtures.js";

const render = (ui: ReactElement) => testingRender(<UiProvider>{ui}</UiProvider>);

function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function installFetch(locateResponse?: () => Promise<Response>, windowResponse?: () => Promise<Response>, sessionResponse?: () => Promise<Response>, inventoryResponse?: () => Promise<Response>) {
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
    if (url.endsWith("/api/runtime-console/workflow-session") && sessionResponse) return await sessionResponse();
    if (url.endsWith("/api/runtime-console/workflow-session")) return json(sessionDetail({ session_id: body.session_id }));
    if (url.endsWith("/api/runtime-console/projects")) return json({ projects: overview.projects, total: 1, truncated: false });
    if (url.endsWith("/api/runtime-console/workflow-sessions")) return json({ sessions: [sessionItem()], total: 1, returned: 1, truncated: false });
    if (url.endsWith("/api/runtime-console/windows") && inventoryResponse) return await inventoryResponse();
    if (url.endsWith("/api/runtime-console/windows")) return json({
      windows: [{ client_window_key: "a".repeat(64), last_project: "agent:special:webcodex", source: "openai-session", last_seen_at_ms: 1_790_000_000_000, active_count: 0, linked_session_count: 1, recorder_gap_count: 0 }],
      returned: 1,
      total: 1,
      truncated: false,
      visibility: { scope: "principal" },
    });
    if (url.endsWith("/api/runtime-console/window") && windowResponse) return await windowResponse();
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
    expect(await screen.findByRole("searchbox", { name: "Search Windows" })).toBeTruthy();

    const primary = screen.getAllByRole("navigation", { name: "Workspace views" })[0];
    expect(primary.textContent).toContain("Work");
    expect(primary.textContent).toContain("Projects");
    expect(primary.textContent).toContain("Runtime");
    expect(primary.textContent).not.toContain("Workflow Sessions");
    expect(primary.textContent).not.toContain("Window Activity");
    expect((screen.getByRole("radio", { name: /Activity/ }) as HTMLInputElement).checked).toBe(true);
    expect(screen.getByRole("radio", { name: /Goals/ })).toBeTruthy();
    expect(screen.queryByRole("radio", { name: /Sessions/ })).toBeNull();

    fireEvent.click(screen.getAllByRole("button", { name: /Projects/ })[0]);
    expect(await screen.findByRole("heading", { name: "Projects" })).toBeTruthy();

    fireEvent.click(screen.getAllByRole("button", { name: /Runtime/ })[0]);
    expect(await screen.findByRole("heading", { name: "Runtime" })).toBeTruthy();
    expect(screen.queryByRole("tab", { name: /Window Activity/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /View activity/ }));
    expect(await screen.findByTestId("window-primary-workbench")).toBeTruthy();
    fireEvent.click(screen.getAllByRole("button", { name: /Runtime/ })[0]);
    expect(screen.getByRole("tab", { name: /Agents/ })).toBeTruthy();
  });

  const linkedWindow = (key: string) => ({
    client_window_key: key, source: "openai-session", first_linked_at_ms: 1, last_linked_at_ms: 2,
    last_seen_at_ms: 2, relations: ["recording"], relation_count: 1, recorder_gap_count: 0,
  });
  const openProjectSession = async () => {
    window.localStorage.setItem("webcodex.runtime.v2.view.v1", "projects");
    render(<App />);
    fireEvent.click(await screen.findByText(sessionItem().title));
  };

  it("opens a Project Session in the current Window workbench, even outside the Window inventory", async () => {
    const key = "b".repeat(64);
    const sessionId = sessionItem().session_id;
    let includeTarget = false;
    installFetch(undefined, async () => json(windowDetail({ client_window_key: key })), async () => json(sessionDetail({ linked_windows: [linkedWindow(key)] })), async () => json({
      windows: (includeTarget ? ["a".repeat(64), key] : ["a".repeat(64)]).map(client_window_key => ({ client_window_key, source: "openai-session", last_seen_at_ms: 1, active_count: 0 })),
      total: includeTarget ? 2 : 1, truncated: !includeTarget,
    }));
    await openProjectSession();
    expect(await screen.findByTestId("window-primary-workbench")).toBeTruthy();
    await waitFor(() => expect((screen.getByRole("combobox", { name: "Session filter" }) as HTMLSelectElement).value).toBe(sessionId));
    expect(screen.getByText("No calls in this Session")).toBeTruthy();
    const selectedRow = screen.getByTestId("work-window-row-" + key);
    expect(selectedRow.getAttribute("aria-current")).toBe("true");
    expect(selectedRow.closest(".window-current-selection")).toBeTruthy();
    expect(screen.getByTestId("current-window-identity").getAttribute("title")).toBe(key);
    expect(screen.getByTestId("work-window-row-" + "a".repeat(64)).getAttribute("aria-current")).toBeNull();
    includeTarget = true;
    fireEvent.focus(window);
    await waitFor(() => expect(screen.getByTestId("work-window-row-" + key).closest(".window-current-selection")).toBeNull());
    expect(screen.getAllByTestId("work-window-row-" + key)).toHaveLength(1);
    fireEvent.change(screen.getByRole("searchbox", { name: "Search Windows" }), { target: { value: "no-matching-window" } });
    expect(screen.getByTestId("work-window-row-" + key).closest(".window-current-selection")).toBeTruthy();
    expect(screen.getByTestId("current-window-identity").getAttribute("title")).toBe(key);
    expect(screen.queryByRole("searchbox", { name: "Search Sessions" })).toBeNull();
    const calls = vi.mocked(fetch).mock.calls;
    expect(calls.some(([url]) => String(url).endsWith("/project-git") || String(url).endsWith("/workflow-session-messages"))).toBe(false);
    const windowCalls = calls.filter(([url]) => String(url).endsWith("/window"));
    expect(windowCalls.length).toBeGreaterThan(0);
    expect(windowCalls.every(([, init]) => JSON.parse(String(init?.body)).client_window_key === key)).toBe(true);
  });

  it("represents the exact jump target in the sidebar before detail arrives", async () => {
    const key = "b".repeat(64);
    let release!: () => void;
    const pending = new Promise<void>(resolve => { release = resolve; });
    installFetch(undefined, async () => { await pending; return json(windowDetail({ client_window_key: key })); }, async () => json(sessionDetail({ linked_windows: [linkedWindow(key)] })));
    await openProjectSession();
    const row = await screen.findByTestId("work-window-row-" + key);
    expect(row.getAttribute("aria-current")).toBe("true");
    expect(row.textContent).toContain("Loading recent activity…");
    release();
    await waitFor(() => expect(screen.getByTestId("current-window-identity").getAttribute("title")).toBe(key));
    expect(screen.getAllByTestId("work-window-row-" + key)).toHaveLength(1);
  });

  it("asks which exact Window to open when a Session has multiple links", async () => {
    installFetch(undefined, undefined, async () => json(sessionDetail({ linked_windows: [linkedWindow("a".repeat(64)), linkedWindow("b".repeat(64))] })));
    await openProjectSession();
    expect(await screen.findByText("Choose a Window for this Session")).toBeTruthy();
    expect(screen.queryByTestId("window-primary-workbench")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Window bbbbb/ }));
    expect(await screen.findByTestId("window-primary-workbench")).toBeTruthy();
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.some(([url, init]) => String(url).endsWith("/window") && JSON.parse(String(init?.body)).client_window_key === "b".repeat(64))).toBe(true));
  });

  it("keeps unlinked history explicit instead of silently switching to the Session layout", async () => {
    await openProjectSession();
    expect(await screen.findByText("No linked Windows in retained evidence.")).toBeTruthy();
    expect(screen.queryByRole("searchbox", { name: "Search Sessions" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "View Session record" }));
    expect(await screen.findByRole("searchbox", { name: "Search Sessions" })).toBeTruthy();
  });

  it.each([403, 404])("does not redirect to another Window when Session access returns %s", async status => {
    installFetch(undefined, undefined, async () => json({}, status));
    await openProjectSession();
    expect(await screen.findByText("Session unavailable")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "View Session record" })).toBeNull();
    expect(screen.queryByTestId("window-primary-workbench")).toBeNull();
  });

  it("locks the workspace when resolving a Session returns 401", async () => {
    installFetch(undefined, undefined, async () => json({}, 401));
    await openProjectSession();
    expect(await screen.findByRole("heading", { name: "Connect to your workspace" })).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("ignores a Session navigation result after the dialog is dismissed", async () => {
    let resolveSession!: (response: Response) => void;
    const pending = new Promise<Response>(resolve => { resolveSession = resolve; });
    installFetch(undefined, undefined, () => pending);
    await openProjectSession();
    expect(await screen.findByText("Finding linked Windows…")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    resolveSession(json(sessionDetail({ linked_windows: [linkedWindow("b".repeat(64))] })));
    await new Promise(resolve => setTimeout(resolve, 0));
    expect(screen.queryByTestId("window-primary-workbench")).toBeNull();
    expect(screen.getByRole("heading", { name: "Projects" })).toBeTruthy();
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

  it("cannot apply delayed Window detail after the workspace is locked", async () => {
    let resolveWindow!: (response: Response) => void;
    const pendingWindow = new Promise<Response>((resolve) => { resolveWindow = resolve; });
    installFetch(undefined, () => pendingWindow);
    render(<App />);
    expect(await screen.findByRole("searchbox", { name: "Search Windows" })).toBeTruthy();
    await waitFor(() => expect((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.some(([url]) => String(url).endsWith("/api/runtime-console/window"))).toBe(true));

    fireEvent.click(screen.getByRole("button", { name: "Lock" }));
    expect(await screen.findByRole("heading", { name: "Connect to your workspace" })).toBeTruthy();

    resolveWindow(json(windowDetail({
      activity: [{
        started_at_ms: 1_790_000_000_000,
        ended_at_ms: 1_790_000_000_100,
        duration_ms: 100,
        method: "tools/call",
        tool_name: "stale-window-call",
        status: "success",
        meaningful: true,
        workflow_sessions: [],
      }],
      activity_returned: 1,
    })));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByText("stale-window-call")).toBeNull();
    expect(screen.getByRole("heading", { name: "Connect to your workspace" })).toBeTruthy();
  });
});
