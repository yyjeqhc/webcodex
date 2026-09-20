import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { workItemFromRecent } from "../src/runtime-v2/model/work.js";
import { ProjectsView } from "../src/runtime-v2/views/ProjectsView.js";
import { RuntimeView } from "../src/runtime-v2/views/RuntimeView.js";
import { WorkView } from "../src/runtime-v2/views/WorkView.js";
import { recentSession, runtimeOverview, sessionDetail, sessionItem, windowDetail } from "./fixtures.js";

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
  it("renders Project -> multiple active Sessions and bounded Window counts", async () => {
    const sessions = [
      sessionItem({ session_id: "wc_sess_1111111111111111", title: "Runtime E2E hardening", running_jobs: 1 }),
      sessionItem({ session_id: "wc_sess_2222222222222222", title: "WebUI v2 rewrite", running_call: true }),
    ];
    const overview = runtimeOverview();
    const client = fakeClient((path, payload) => {
      if (path === "projects") return ok({ projects: overview.projects, total: 1, truncated: false });
      if (path === "project-git") return ok({ branch: "prototype/runtime-webui-v2", clean: false, git_available: true });
      if (path === "workflow-sessions") return ok({ sessions, total: 2, returned: 2, truncated: false });
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
        onUnauthorized={vi.fn()}
      />,
    );

    expect(await screen.findByText("Runtime E2E hardening")).toBeTruthy();
    expect(screen.getByText("WebUI v2 rewrite")).toBeTruthy();
    await waitFor(() => expect(screen.getByText(/2 Windows/)).toBeTruthy());
    expect(screen.getByText(/1 Windows/)).toBeTruthy();
  });

  it("keeps Add Project a single non-replayed write after an uncertain result", async () => {
    const overview = runtimeOverview();
    const handler = vi.fn((path: string) => {
      if (path === "projects") return ok({ projects: overview.projects, total: 1, truncated: false });
      if (path === "project-git") return ok({ branch: "main" });
      if (path === "workflow-sessions") return ok({ sessions: [], total: 0, returned: 0, truncated: false });
      if (path === "/api/projects/resolve-or-register") return { ok: false, status: 0, data: null };
      throw new Error("unexpected path " + path);
    });
    const client = fakeClient(handler);

    render(
      <ProjectsView client={client} language="en" runners={overview.runners} onOpenSession={vi.fn()} onUnauthorized={vi.fn()} />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Add Project" }));
    fireEvent.change(screen.getByPlaceholderText("Absolute folder path on the selected Runner"), {
      target: { value: "/root/git/new-project" },
    });
    fireEvent.click(screen.getAllByRole("button", { name: "Add Project" }).at(-1)!);
    await screen.findByText("The result could not be confirmed. Refresh Projects before trying again.");
    expect(handler.mock.calls.filter(([path]) => path === "/api/projects/resolve-or-register")).toHaveLength(1);
  });

  it("renders Window -> multiple Sessions with relation kinds and no ownership implication", async () => {
    const overview = runtimeOverview();
    const firstKey = "a".repeat(64);
    const secondKey = "b".repeat(64);
    const detail = windowDetail({
      client_window_key: firstKey,
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
      <RuntimeView
        client={client}
        language="en"
        overview={overview}
        overviewAvailability="available"
        projects={overview.projects}
        onOpenSession={vi.fn()}
        onUnauthorized={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("tab", { name: /Window Activity/ }));

    expect(await screen.findByText("Runtime E2E hardening")).toBeTruthy();
    expect(screen.getByText("WebUI v2 rewrite")).toBeTruthy();
    expect(screen.getByText("recording")).toBeTruthy();
    expect(screen.getByText("work_on_project")).toBeTruthy();
    expect(screen.getAllByText(/observation evidence/i).length).toBeGreaterThan(0);
    expect(screen.getByTestId("window-scope-note").textContent).toContain("observation principal");

    fireEvent.click(screen.getByRole("button", { name: /Window bbbbbbbbbb/ }));
    expect(await screen.findByText("Window with no current Session")).toBeTruthy();
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
      onOpenSession: vi.fn(),
      onLocateSession: vi.fn(async () => false),
      onUnauthorized: vi.fn(),
    };
    const rendered = render(<WorkView {...props} />);
    fireEvent.click(await screen.findByRole("tab", { name: "Evidence" }));
    expect(await screen.findByText(/Window 1111111111/)).toBeTruthy();
    expect(screen.getByText(/Window 2222222222/)).toBeTruthy();
    expect(screen.getAllByText(/A very long Session title A very long Session title/).length).toBeGreaterThan(0);

    rendered.unmount();
    detail = sessionDetail({ title: recent.title, linked_windows: [] });
    render(<WorkView {...props} />);
    fireEvent.click(await screen.findByRole("tab", { name: "Evidence" }));
    expect(await screen.findByText("No linked Windows in retained evidence.")).toBeTruthy();
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
        projects={overview.projects}
        onOpenSession={vi.fn()}
        onUnauthorized={vi.fn()}
      />,
    );

    expect(screen.getAllByText("Runtime overview unavailable").length).toBeGreaterThan(0);
    expect(screen.queryByText(/^connected$/)).toBeNull();
  });

  it("progressively reveals retained Window activity instead of silently dropping rows", async () => {
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
    }));
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
      <RuntimeView
        client={client}
        language="en"
        overview={overview}
        overviewAvailability="available"
        projects={overview.projects}
        onOpenSession={vi.fn()}
        onUnauthorized={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("tab", { name: /Window Activity/ }));
    expect(await screen.findByText("tool-204")).toBeTruthy();
    expect(screen.queryByText("tool-0")).toBeNull();
    const more = screen.getByRole("button", { name: /Show more activity/ });
    expect(more.textContent).toContain("5 remaining");
    fireEvent.click(more);
    expect(screen.getByText("tool-0")).toBeTruthy();
    expect(screen.getByText("Server activity history is bounded; older Window activity is not loaded.")).toBeTruthy();
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
