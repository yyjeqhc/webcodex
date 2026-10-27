import { act, fireEvent, render, renderHook, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { UiProvider } from "../src/ui/UiProvider.js";
import { WindowWorkbench } from "../src/runtime-v2/components/WindowWorkbench.js";
import { useWindowWorkspace } from "../src/runtime-v2/state/useWindowWorkspace.js";
import { mergeWindowSummary } from "../src/runtime-v2/model/windowSummary.js";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { runtimeOverview, windowDetail } from "./fixtures.js";
const key = "a".repeat(64);
const base = { client_window_key: key, source: "openai-session", last_seen_at_ms: 1_790_000_000_000,
  active_count: 0, linked_session_count: 0, recorder_gap_count: 0, label: "Before" };
const ok = (data: unknown) => ({ ok: true, status: 200, data });
function deferred() { let resolve!: (value: any) => void; const promise = new Promise<any>(done => { resolve = done; }); return { resolve, promise }; }
function fixture(label: string | null = "Before") {
  let current = label;
  const post = vi.fn(async (path: string, body: any, _signal?: AbortSignal): Promise<any> => {
    if (path === "window-label") { current = body.label; return ok({ client_window_key: key, label: current }); }
    if (path === "windows") return ok({ windows: body.projection === "liveness" ? [] : [{ ...base, label: current }], total: 1, truncated: false });
    if (path === "window") return ok(windowDetail({ client_window_key: key, label: current }));
    return ok({ messages: [], can_send: true });
  });
  return { post, client: { post } as unknown as RuntimeV2Client };
}
const originalMedia = window.matchMedia;
afterEach(() => { window.matchMedia = originalMedia; vi.restoreAllMocks(); window.history.replaceState(null, ""); });
it("preserves absent liveness labels, accepts explicit null and fences older label observations independently of activity timestamps", () => {
  const { label: _label, ...live } = base;
  expect(mergeWindowSummary(base, { ...live, last_seen_at_ms: base.last_seen_at_ms + 100 }).label).toBe("Before");
  expect(mergeWindowSummary(base, { ...base, label: null, last_seen_at_ms: 1 }).label).toBeNull();
  expect(mergeWindowSummary(base, { ...base, label: "stale", last_seen_at_ms: base.last_seen_at_ms + 100 }, false).label).toBe("Before");
});
it("optimistically renames and rejects a pre-write inventory response arriving after success", async () => {
  const { client, post } = fixture(); const unauthorized = vi.fn();
  const { result } = renderHook(() => useWindowWorkspace(client, true, unauthorized, { initialWindowKey: key, autoSelect: false }));
  await waitFor(() => expect(result.current.detail?.label).toBe("Before"));
  const stale = deferred(); const write = deferred(); const normal = post.getMockImplementation()!;
  post.mockImplementation((path, body, signal) => path === "window-label" ? write.promise : path === "windows" && body.projection === "inventory" ? stale.promise : normal(path, body, signal));
  act(() => result.current.refresh());
  let saved!: Promise<boolean>;
  act(() => { saved = result.current.saveLabel(key, "After"); });
  expect(result.current.windows[0].label).toBe("After"); expect(result.current.detail?.label).toBe("After");
  await act(async () => { write.resolve(ok({ client_window_key: key, label: "After" })); await saved; });
  await act(async () => stale.resolve(ok({ windows: [{ ...base, label: "Before", last_seen_at_ms: base.last_seen_at_ms + 1000 }], total: 1, truncated: false })));
  expect(result.current.windows[0].label).toBe("After"); expect(result.current.detail?.label).toBe("After");
});
it("fences a pre-write first detail response even when no detail was present at save time", async () => {
  const { client, post } = fixture(); const stale = deferred(); const normal = post.getMockImplementation()!;
  let held = false;
  post.mockImplementation((path, body, signal) => {
    if (path === "window" && !held) { held = true; return stale.promise; }
    return normal(path, body, signal);
  });
  const unauthorized = vi.fn();
  const { result } = renderHook(() => useWindowWorkspace(client, true, unauthorized, { initialWindowKey: key, autoSelect: false }));
  await waitFor(() => expect(result.current.windows[0]?.label).toBe("Before"));
  expect(result.current.detail).toBeNull();
  await act(async () => { expect(await result.current.saveLabel(key, "After")).toBe(true); });
  await act(async () => stale.resolve(ok(windowDetail({ client_window_key: key, label: "Before" }))));
  await waitFor(() => expect(result.current.detail?.label).toBe("After"));
  expect(result.current.windows[0].label).toBe("After");
});
it("fences a pre-write first inventory response when the selected Window only had exact detail", async () => {
  const { client, post } = fixture(); const stale = deferred(); const normal = post.getMockImplementation()!;
  post.mockImplementation((path, body, signal) => path === "windows" && body.projection === "inventory"
    ? stale.promise : normal(path, body, signal));
  const unauthorized = vi.fn();
  const { result } = renderHook(() => useWindowWorkspace(client, true, unauthorized, { initialWindowKey: key, autoSelect: false }));
  await waitFor(() => expect(result.current.detail?.label).toBe("Before"));
  expect(result.current.windows).toEqual([]);
  await act(async () => { expect(await result.current.saveLabel(key, "After")).toBe(true); });
  await act(async () => stale.resolve(ok({ windows: [base], total: 1, truncated: false })));
  expect(result.current.windows[0].label).toBe("After");
  expect(result.current.detail?.label).toBe("After");
});
it.each([[403, "You do not have permission"], [404, "no longer visible"], [409, "no recorded activity yet"], [503, "Please retry"]])("rolls back a failed %i write and exposes a specific error", async (status, message) => {
  const { client, post } = fixture(); const unauthorized = vi.fn();
  const { result } = renderHook(() => useWindowWorkspace(client, true, unauthorized, { initialWindowKey: key }));
  await waitFor(() => expect(result.current.detail?.label).toBe("Before"));
  post.mockResolvedValueOnce({ ok: false, status, data: null });
  await act(async () => { expect(await result.current.saveLabel(key, "Changed")).toBe(false); });
  expect(result.current.windows[0].label).toBe("Before"); expect(result.current.detail?.label).toBe("Before");
  expect(result.current.labelError).toContain(message);
});
it("aborts a superseded save, ignores its delayed result and cancels writes on authority replacement", async () => {
  const { client, post } = fixture(); const unauthorized = vi.fn();
  const { result, rerender } = renderHook(({ api }) => useWindowWorkspace(api, true, unauthorized, { initialWindowKey: key }), { initialProps: { api: client } });
  await waitFor(() => expect(result.current.detail?.label).toBe("Before"));
  const first = deferred(); const second = deferred(); const normal = post.getMockImplementation()!;
  const signals: AbortSignal[] = [];
  post.mockImplementation((path, body, signal) => {
    if (path !== "window-label") return normal(path, body, signal);
    signals.push(signal!); return signals.length === 1 ? first.promise : second.promise;
  });
  let older!: Promise<boolean>; let newer!: Promise<boolean>;
  act(() => { older = result.current.saveLabel(key, "First"); });
  act(() => { newer = result.current.saveLabel(key, "Second"); });
  expect(signals[0].aborted).toBe(true);
  await act(async () => { second.resolve(ok({ client_window_key: key, label: "Second" })); await newer; first.resolve(ok({ client_window_key: key, label: "First" })); await older; });
  expect(result.current.windows[0].label).toBe("Second");
  const pending = deferred(); post.mockImplementation(() => pending.promise);
  act(() => { void result.current.saveLabel(key, "Secret old authority"); });
  const other = fixture("Other credential"); rerender({ api: other.client });
  await act(async () => pending.resolve(ok({ client_window_key: key, label: "Secret old authority" })));
  await waitFor(() => expect(result.current.windows[0].label).toBe("Other credential"));
});
function mount(label: string | null = null) {
  const fixtureValue = fixture(label);
  render(<UiProvider><WindowWorkbench client={fixtureValue.client} projects={runtimeOverview().projects} language="en" onUnauthorized={vi.fn()} /></UiProvider>);
  return fixtureValue;
}
it("renames with F2, cancels Escape, clears on blur and distinguishes default titles", async () => {
  const { post } = mount(); const row = await screen.findByTestId("work-window-row-" + key);
  expect(row.querySelector("strong")?.className).toBe("window-board-default-name");
  fireEvent.click(row); fireEvent.keyDown(row, { key: "F2" });
  let input = await screen.findByRole("textbox", { name: "Window name" });
  fireEvent.change(input, { target: { value: "Research" } }); fireEvent.keyDown(input, { key: "Enter" });
  await screen.findByRole("heading", { name: "Research" });
  expect(row.querySelector("strong")?.className).toBe("");
  fireEvent.doubleClick(row.querySelector("strong")!); input = await screen.findByRole("textbox", { name: "Window name" });
  fireEvent.change(input, { target: { value: "Discard" } }); fireEvent.keyDown(input, { key: "Escape" });
  expect(screen.getByRole("heading", { name: "Research" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Rename Window" })); input = await screen.findByRole("textbox", { name: "Window name" });
  fireEvent.change(input, { target: { value: "  " } }); fireEvent.blur(input);
  await screen.findByRole("heading", { name: "Window · aaaa" });
  expect(post.mock.calls.filter(([path]) => path === "window-label").map(([, body]) => body.label)).toEqual(["Research", null]);
  fireEvent.click(screen.getByRole("button", { name: /^Named/ }));
  // The selected unnamed Window remains pinned even when Named excludes it.
  expect(screen.getByText("Current Window")).toBeTruthy();
});
it("opens the mobile name sheet, saves and clears through explicit buttons", async () => {
  window.matchMedia = vi.fn(query => ({ ...originalMedia(query), matches: /max-width/.test(query) }));
  mount(); fireEvent.click(await screen.findByTestId("work-window-row-" + key));
  fireEvent.click(screen.getByRole("button", { name: "Rename Window" }));
  const input = await screen.findByRole("textbox", { name: "Window name" });
  expect(screen.getByRole("dialog", { name: "Rename Window" })).toBeTruthy();
  fireEvent.change(input, { target: { value: "Mobile research" } }); fireEvent.click(screen.getByRole("button", { name: "Save", exact: true }));
  await screen.findByRole("heading", { name: "Mobile research" });
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Rename Window" })).toBeNull());
  fireEvent.click(screen.getByRole("button", { name: "Rename Window" })); fireEvent.click(await screen.findByRole("button", { name: "Clear name" }));
  await screen.findByRole("heading", { name: "Window · aaaa" });
});
