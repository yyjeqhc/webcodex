import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { UiProvider } from "../src/ui/UiProvider.js";
import { WindowWorkbench } from "../src/runtime-v2/components/WindowWorkbench.js";
import { compareWindowRows, matchesWindowRow, windowBoardRow, WINDOW_RECENT_MS } from "../src/runtime-v2/model/windowBoard.js";
import type { WindowSummary } from "../src/runtime-v2/model/types.js";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { runtimeOverview, windowDetail } from "./fixtures.js";
const projects = runtimeOverview().projects;
const index = new Map(projects.map(p => [p.id, p]));
const now = new Date(2026, 9, 27, 12).getTime();
const summary = (overrides: Partial<WindowSummary> = {}): WindowSummary => ({
  client_window_key: "a".repeat(64), source: "openai-session", last_seen_at_ms: now,
  active_count: 0, linked_session_count: 2, recorder_gap_count: 0, ...overrides,
});
describe("Window board projection", () => {
  it("uses meaningful time, includes the exact ten minute boundary, and compares local calendar days", () => {
    const row = (overrides: Partial<WindowSummary>) => windowBoardRow(summary(overrides), index, now);
    expect(row({ last_meaningful_activity_at_ms: now - WINDOW_RECENT_MS }).tier).toBe("recent");
    expect(row({ last_meaningful_activity_at_ms: now - WINDOW_RECENT_MS - 1 }).tier).toBe("today");
    expect(row({ last_meaningful_activity_at_ms: new Date(2026, 9, 26, 23, 59).getTime() }).tier).toBe("earlier");
    expect(row({ active_count: 1, last_seen_at_ms: 0 }).tier).toBe("running");
  });
  it("sorts running first, then meaningful time, then exact key", () => {
    const rows = [summary({ client_window_key: "b", last_seen_at_ms: now + 1, last_meaningful_activity_at_ms: now - 10 }),
      summary({ client_window_key: "a", last_seen_at_ms: now - 10 }), summary({ client_window_key: "r", active_count: 1, last_seen_at_ms: 1 })];
    expect(rows.map(w => windowBoardRow(w, index, now)).sort(compareWindowRows).map(r => r.window.client_window_key)).toEqual(["r", "a", "b"]);
  });
  it("builds translated fallback names and searches all visible context", () => {
    expect(windowBoardRow(summary(), index, now, "窗口").displayName).toBe("窗口 · aaaa");
    const row = windowBoardRow(summary({ last_project: projects[0].id, last_activity_name: "read_files" }), index, now);
    expect(row.displayName).toContain(" · aaaa");
    for (const query of [row.projectName, row.machine, row.path, "ChatGPT", "read_files", "aaaa"]) expect(matchesWindowRow(row, query!)).toBe(true);
    expect(matchesWindowRow(row, "missing phrase")).toBe(false);
  });
  it.each([["openai-session", "ChatGPT"], ["openai-conversation", "ChatGPT"], ["codex-thread", "Codex"], ["client-window-header", "Header"], ["future", "future"]])("maps source %s", (source, label) => {
    expect(windowBoardRow(summary({ source }), index, now).sourceLabel).toBe(label);
  });
});
const originalMedia = window.matchMedia;
afterEach(() => { window.matchMedia = originalMedia; vi.restoreAllMocks(); window.history.replaceState(null, ""); });
function width(px: number) {
  window.matchMedia = vi.fn(query => ({ ...originalMedia(query), matches: query.includes("max-width: 700") ? px <= 700 : query.includes("max-width: 1279") ? px < 1280 : false }));
}
function board(onMobileDetailChange = vi.fn()) {
  const key = summary().client_window_key;
  const client = { post: vi.fn(async (path: string) => ({ ok: true, status: 200, data: path === "windows" ? { windows: [summary()], total: 1, truncated: false }
    : path === "window-liveness" ? { windows: [] } : windowDetail({ client_window_key: key }) })) } as unknown as RuntimeV2Client;
  render(<UiProvider><WindowWorkbench client={client} projects={projects} language="en" onUnauthorized={vi.fn()} onMobileDetailChange={onMobileDetailChange} /></UiProvider>);
  return { key, client };
}
it("starts at overview, opens split detail, ignores Escape in inputs, and returns with Escape or close", async () => {
  width(1440); const { key } = board();
  const row = await screen.findByTestId("work-window-row-" + key);
  expect(screen.queryByLabelText("Window details")).toBeNull();
  row.focus(); fireEvent.click(row);
  expect(screen.getByLabelText("Window details").getAttribute("data-layout")).toBe("split");
  fireEvent.keyDown(screen.getByRole("searchbox", { name: "Search Windows" }), { key: "Escape" });
  expect(screen.getByLabelText("Window details")).toBeTruthy();
  fireEvent.keyDown(window, { key: "Escape" });
  expect(screen.queryByLabelText("Window details")).toBeNull();
  expect(document.activeElement).toBe(row);
  fireEvent.click(row); fireEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(screen.queryByLabelText("Window details")).toBeNull();
});
it("opens a medium-width drawer and moves focus into it", async () => {
  width(1024); const { key } = board(); fireEvent.click(await screen.findByTestId("work-window-row-" + key));
  const detail = screen.getByLabelText("Window details"); expect(detail.getAttribute("data-layout")).toBe("drawer");
  expect(detail.contains(document.activeElement)).toBe(true);
  fireEvent.keyDown(detail, { key: "Escape" }); expect(screen.queryByLabelText("Window details")).toBeNull();
});
it("pushes only a mobile detail marker, restores navigation on popstate, and uses history back", async () => {
  width(390); const signal = vi.fn(); const push = vi.spyOn(window.history, "pushState"); const back = vi.spyOn(window.history, "back");
  const { key } = board(signal); const url = window.location.href;
  fireEvent.click(await screen.findByTestId("work-window-row-" + key));
  expect(push).toHaveBeenCalledWith({ webcodexWindowDetail: true }, ""); expect(window.location.href).toBe(url);
  expect(signal).toHaveBeenLastCalledWith(true);
  fireEvent.click(screen.getByRole("button", { name: "Back" })); expect(back).toHaveBeenCalledOnce();
  await act(async () => window.dispatchEvent(new PopStateEvent("popstate")));
  await waitFor(() => expect(screen.queryByLabelText("Window details")).toBeNull()); expect(signal).toHaveBeenLastCalledWith(false);
});
it("names an active server-side Project filter on the Filter control", async () => {
  width(1440); const { key, client } = board();
  await screen.findByTestId("work-window-row-" + key);
  fireEvent.click(screen.getByRole("button", { name: "Filter" }));
  fireEvent.click(await screen.findByRole("button", { name: "Projects: All projects" }));
  fireEvent.click(within(screen.getByRole("dialog", { name: "Projects" })).getByRole("button", { name: /WebCodex/ }));
  // Every count below now describes one Project, so the control must say which.
  expect((await screen.findByRole("button", { name: "Filter: WebCodex" })).className).toBe("active");
  await waitFor(() => expect(vi.mocked(client.post).mock.calls.some(([path, body]) =>
    path === "windows" && Array.isArray((body as { projects?: string[] }).projects))).toBe(true));
});
