import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/runtime-v2/App.js";
import { UiProvider } from "../src/ui/UiProvider.js";
import { RUNTIME_CREDENTIAL_SESSION_KEY } from "../src/runtime_storage.js";
import { runtimeOverview, sessionDetail, windowDetail } from "./fixtures.js";

const project = runtimeOverview().projects[0];
const keys = { a: "a".repeat(64), b: "b".repeat(64) };
const sessions = { a: "wc_sess_aaaaaaaaaaaaaaaa", b: "wc_sess_bbbbbbbbbbbbbbbb" };
function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), { status, headers: { "Content-Type": "application/json" } });
}
function installFetch(record?: (sessionId: string, signal: AbortSignal | null) => Promise<Response>) {
  const post = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = String(input).split("/").at(-1);
    const body = JSON.parse(String(init?.body || "{}"));
    if (path === "overview") return json(runtimeOverview());
    if (path === "projects") return json({ projects: [project], total: 1, truncated: false });
    if (path === "windows") return json({
      windows: (["a", "b"] as const).map(name => ({
        client_window_key: keys[name], label: "Window " + name.toUpperCase(),
        last_project: project.id, source: "openai-session", last_seen_at_ms: 1_790_000_000_000,
        active_count: 0, linked_session_count: 1, recorder_gap_count: 0,
      })), returned: 2, total: 2, truncated: false, visibility: { scope: "principal" },
    });
    if (path === "window") {
      const name = body.client_window_key === keys.b ? "b" : "a";
      return json(windowDetail({
        client_window_key: keys[name], label: "Window " + name.toUpperCase(), sessions_returned: 1,
        linked_sessions: [{
          workflow_session_id: sessions[name], project: project.id, title: "Session " + name.toUpperCase(),
          relations: ["recording"], relation_count: 1, first_linked_at_ms: 1, last_linked_at_ms: 2,
        }],
        activity: [{
          method: "tools/call", tool_name: "read_files", project: project.id, status: "success",
          started_at_ms: 1_790_000_000_000, ended_at_ms: 1_790_000_000_010, duration_ms: 10,
          meaningful: true, workflow_sessions: [{ workflow_session_id: sessions[name], relation: "recording" }],
        }], activity_returned: 1,
      }));
    }
    if (path === "workflow-session") return record
      ? record(body.session_id, init?.signal ?? null)
      : json(sessionDetail({ session_id: body.session_id, title: "Record " + (body.session_id === sessions.b ? "B" : "A"), lifecycle: "closed" }));
    if (path === "workflow-session-messages") return json({ session_id: body.session_id, messages: [] });
    if (path === "project-git") return json({ git_available: true, branch: "test", clean: true });
    throw new Error("Unexpected request " + path);
  });
  vi.stubGlobal("fetch", post);
  return post;
}
async function openRecord(name: "a" | "b") {
  fireEvent.click(await screen.findByTestId("work-window-row-" + keys[name]));
  await screen.findByRole("combobox", { name: "Session filter" });
  fireEvent.click(screen.getByTestId("window-session-tag"));
  fireEvent.click(screen.getByRole("button", { name: "View Session record" }));
}

describe("Window-owned Session record navigation", () => {
  beforeEach(() => {
    window.sessionStorage.setItem(RUNTIME_CREDENTIAL_SESSION_KEY, "test-token");
    window.localStorage.setItem("webcodex.runtime.v2.view.v1", "work");
  });

  it("keeps the exact Window, activity filters, DOM and scroll when opening and returning from a Session record", async () => {
    const post = installFetch();
    const { container } = render(<UiProvider><App /></UiProvider>);
    fireEvent.click(await screen.findByTestId("work-window-row-" + keys.b));
    await screen.findByRole("combobox", { name: "Session filter" });
    fireEvent.click(screen.getByTestId("window-session-tag"));
    const board = screen.getByTestId("window-primary-workbench");
    const list = container.querySelector<HTMLElement>(".window-board-list")!;
    const panel = container.querySelector<HTMLElement>("#window-activity-panel")!;
    const results = panel.querySelector<HTMLElement>(".window-activity-results")!;
    const detail = screen.getByLabelText("Window details");
    fireEvent.change(screen.getByRole("searchbox", { name: "Search Windows" }), { target: { value: "Window B" } });
    fireEvent.change(screen.getByRole("textbox", { name: "Search calls" }), { target: { value: "read_files" } });
    list.scrollTop = 137; panel.scrollTop = 263; results.scrollTop = 419; detail.scrollTop = 89;
    const before = post.mock.calls.length;
    fireEvent.click(screen.getByRole("button", { name: "View Session record" }));
    expect(await screen.findByRole("tab", { name: "Session record", selected: true })).toBeTruthy();
    expect(screen.getByTestId("window-primary-workbench")).toBe(board);
    expect(container.querySelector(".window-board-list")).toBe(list);
    expect(screen.getByTestId("work-window-row-" + keys.b).getAttribute("aria-current")).toBe("true");
    expect(screen.getByTestId("current-window-identity").getAttribute("title")).toBe(keys.b);
    expect(screen.queryByRole("searchbox", { name: "Search Sessions" })).toBeNull();
    await within(screen.getByLabelText("Window details")).findByRole("heading", { name: "Record B" });
    const added = post.mock.calls.slice(before).map(([input]) => String(input).split("/").at(-1));
    expect(added).not.toContain("overview");
    expect(added).not.toContain("project-git");
    expect(added.filter(path => path === "workflow-session")).toHaveLength(1);
    // Browsers may report zero offsets while display:none. Restoration cannot
    // rely on jsdom retaining a hidden element's scrollTop.
    panel.scrollTop = 0; results.scrollTop = 0; detail.scrollTop = 0;
    fireEvent.click(screen.getByRole("button", { name: "Back to Window" }));
    expect(screen.getByRole("tab", { name: "Window", selected: true })).toBeTruthy();
    expect(container.querySelector("#window-activity-panel")).toBe(panel);
    expect((screen.getByRole("textbox", { name: "Search calls" }) as HTMLInputElement).value).toBe("read_files");
    expect((screen.getByRole("searchbox", { name: "Search Windows" }) as HTMLInputElement).value).toBe("Window B");
    expect((screen.getByRole("combobox", { name: "Session filter" }) as HTMLInputElement).value).toContain("Session B");
    expect([list.scrollTop, panel.scrollTop, results.scrollTop, detail.scrollTop]).toEqual([137, 263, 419, 89]);
    panel.scrollTop = 501; results.scrollTop = 613;
    fireEvent.click(screen.getByRole("tab", { name: "Session record" }));
    await within(screen.getByLabelText("Window details")).findByRole("heading", { name: "Record B" });
    panel.scrollTop = 0; results.scrollTop = 0;
    fireEvent.click(screen.getByRole("tab", { name: "Window" }));
    expect([panel.scrollTop, results.scrollTop]).toEqual([501, 613]);
  });

  it("cancels an old record on another Window selection and keeps Session denial inside the selected Window", async () => {
    let resolve!: (value: Response) => void;
    let signal: AbortSignal | null = null;
    const pending = new Promise<Response>(done => { resolve = done; });
    installFetch(async (sessionId, nextSignal) => {
      if (sessionId === sessions.b) { signal = nextSignal; return pending; }
      return json({}, 403);
    });
    render(<UiProvider><App /></UiProvider>);
    await openRecord("b");
    await screen.findByRole("tab", { name: "Session record", selected: true });
    fireEvent.click(screen.getByTestId("work-window-row-" + keys.a));
    expect(screen.queryByRole("tab", { name: "Session record" })).toBeNull();
    expect(screen.getByRole("tab", { name: "Window", selected: true })).toBeTruthy();
    await waitFor(() => expect(signal?.aborted).toBe(true));
    await act(async () => resolve(json(sessionDetail({ session_id: sessions.b, title: "Old private record" }))));
    expect(screen.queryByText("Old private record")).toBeNull();
    await waitFor(() => expect(screen.getByTestId("current-window-identity").getAttribute("title")).toBe(keys.a));
    fireEvent.click(screen.getByTestId("window-session-tag"));
    fireEvent.click(screen.getByRole("button", { name: "View Session record" }));
    expect(await screen.findByText("Session unavailable")).toBeTruthy();
    expect(screen.getByTestId("work-window-row-" + keys.a).getAttribute("aria-current")).toBe("true");
    expect(screen.queryByRole("heading", { name: "Connect to your workspace" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Back to Window" }));
    expect(screen.getByRole("tab", { name: "Window", selected: true })).toBeTruthy();
  });
});
