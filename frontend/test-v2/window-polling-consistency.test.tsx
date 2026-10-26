import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import type { WindowDetail, WindowSummary } from "../src/runtime-v2/model/types.js";
import { mergeWindowSummary, summaryFromWindowDetail } from "../src/runtime-v2/model/windowSummary.js";
import { useWindowInventory } from "../src/runtime-v2/state/useWindowInventory.js";
import { useWindowWorkspace } from "../src/runtime-v2/state/useWindowWorkspace.js";
import { windowDetail } from "./fixtures.js";

type Response = { ok: boolean; status: number; data: unknown };
type Body = { projection?: string; detail_level?: string; client_window_key?: string; projects?: string[] };
const key = "a".repeat(64);
const time = 1_790_000_000_000;
const ok = (data: unknown): Response => ({ ok: true, status: 200, data });
const denied = (status: number): Response => ({ ok: false, status, data: null });
function deferred() {
  let resolve!: (value: Response) => void;
  const promise = new Promise<Response>(done => { resolve = done; });
  return { promise, resolve };
}
function summary(overrides: Partial<WindowSummary> = {}): WindowSummary {
  return { client_window_key: key, source: "openai-session", last_seen_at_ms: time,
    active_count: 0, linked_session_count: 7, recorder_gap_count: 0, ...overrides };
}
function inventory(rows: WindowSummary[] = [summary()]): Response {
  return ok({ windows: rows, total: rows.length, returned: rows.length, truncated: false, visibility: { scope: "principal" } });
}
function transport(handler: (path: string, body: Body, signal?: AbortSignal) => Response | Promise<Response>) {
  const value = { authorityRevision: 0, post: vi.fn(async (path: string, body: Body, signal?: AbortSignal) => handler(path, body, signal)) };
  return { value, client: value as unknown as RuntimeV2Client };
}
const flush = () => act(async () => {});
const advance = (ms: number) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(time);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("Window observation consistency", () => {
  it("keeps request and meaningful activity clocks distinct and never infers total Sessions from a truncated page", () => {
    const current = summary({ last_seen_at_ms: time + 100, last_meaningful_activity_at_ms: time });
    const detail = windowDetail({ detail_level: "full", last_seen_at_ms: time + 200,
      last_meaningful_activity_at_ms: time, sessions_returned: 2, sessions_truncated: true });
    const merged = mergeWindowSummary(current, summaryFromWindowDetail(detail));
    expect(merged.last_seen_at_ms).toBe(time + 200);
    expect(merged.last_meaningful_activity_at_ms).toBe(time);
    expect(merged.linked_session_count).toBe(7);
    expect(summaryFromWindowDetail({ ...detail, sessions_truncated: false }).linked_session_count).toBe(2);
  });

  it("advances the local authority fence only when credentials change", () => {
    const client = new RuntimeV2Client();
    expect(client.authorityRevision).toBe(0);
    client.setToken("fixture-principal-a");
    const revision = client.authorityRevision;
    client.setToken("fixture-principal-a");
    expect(client.authorityRevision).toBe(revision);
    client.setToken("fixture-principal-b");
    expect(client.authorityRevision).toBe(revision + 1);
    client.clearToken();
    expect(client.authorityRevision).toBe(revision + 2);
    client.clearToken();
    expect(client.authorityRevision).toBe(revision + 2);
  });

  it("does not let an older inventory response overwrite an accepted exact observation", async () => {
    const pending = deferred();
    let calls = 0;
    const { client } = transport(() => ++calls === 1 ? inventory() : pending.promise);
    const unauthorized = vi.fn();
    const hook = renderHook(() => useWindowInventory(client, true, unauthorized));
    await flush();
    act(() => hook.result.current.refresh());
    act(() => hook.result.current.updateWindow(summary({
      last_seen_at_ms: time + 200, active_count: 0, last_activity_status: "success", last_activity_name: "run_process",
    })));
    await act(async () => pending.resolve(inventory([summary({
      last_seen_at_ms: time + 100, active_count: 1, last_activity_status: "running", last_activity_name: "read_files",
    })])));
    expect(hook.result.current.windows[0]).toMatchObject({
      last_seen_at_ms: time + 200, active_count: 0, last_activity_name: "run_process", last_activity_status: "success",
    });
  });

  it.each([0, -100])("preserves a later detail update when a recent live row arrives late (timestamp offset %i)", async offset => {
    const pending = deferred();
    let liveCalls = 0;
    const { client } = transport((_path, body) => body.projection === "liveness"
      ? ++liveCalls === 1 ? inventory([summary({ active_count: 1 })]) : pending.promise
      : inventory([]));
    const unauthorized = vi.fn();
    const hook = renderHook(() => useWindowInventory(client, true, unauthorized));
    await flush();
    await advance(3_000);
    expect(hook.result.current.windows).toHaveLength(1);
    await advance(3_000);
    act(() => hook.result.current.updateWindow(summary({
      active_count: 0, last_activity_status: "success", last_activity_name: "completed", linked_session_count: 9,
    })));
    await act(async () => pending.resolve(inventory([summary({
      last_seen_at_ms: time + offset, active_count: 1, last_activity_status: "running", last_activity_name: "older", linked_session_count: 0,
    })])));
    expect(hook.result.current.historicalWindows[0]).toMatchObject({
      last_seen_at_ms: time, active_count: 0, last_activity_name: "completed", linked_session_count: 9,
    });
    expect(hook.result.current.windows[0].active_count).toBe(0);
  });

  it("retains newer primary activity when an older full-history request finishes later", async () => {
    const full = deferred();
    const historical = { started_at_ms: time - 2_000, ended_at_ms: time - 1_000, duration_ms: 1_000,
      method: "tools/call", tool_name: "old_call", status: "success", server_trace_id: "old-trace" };
    let primaryCalls = 0;
    const { client } = transport((path, body) => path === "windows" ? inventory() : body.detail_level === "primary"
      ? ok(windowDetail({ detail_level: "primary", last_seen_at_ms: time + (++primaryCalls * 100), active_count: primaryCalls === 1 ? 1 : 0 }))
      : full.promise);
    const unauthorized = vi.fn();
    const hook = renderHook(() => useWindowWorkspace(client, true, unauthorized, { initialWindowKey: key }));
    await flush();
    await advance(3_000);
    await act(async () => full.resolve(ok(windowDetail({
      detail_level: "full", last_seen_at_ms: time + 100, active_count: 1, activity: [historical], activity_returned: 1,
    }))));
    expect(hook.result.current.detail).toMatchObject({ last_seen_at_ms: time + 200, active_count: 0, detail_level: "full" });
    expect(hook.result.current.detail?.activity).toContainEqual(historical);
    expect(hook.result.current.windows[0]).toMatchObject({ last_seen_at_ms: time + 200, active_count: 0 });
  });

  it("prioritizes server activity time over request order when a later primary contains older state", async () => {
    const full = deferred(), primary = deferred();
    let primaryCalls = 0;
    const { client } = transport((path, body) => path === "windows" ? inventory() : body.detail_level === "primary"
      ? ++primaryCalls === 1 ? ok(windowDetail({ detail_level: "primary", last_seen_at_ms: time })) : primary.promise
      : full.promise);
    const unauthorized = vi.fn();
    const hook = renderHook(() => useWindowWorkspace(client, true, unauthorized, { initialWindowKey: key }));
    await flush();
    await advance(3_000);
    await act(async () => full.resolve(ok(windowDetail({ detail_level: "full", last_seen_at_ms: time + 200, active_count: 0 }))));
    await act(async () => primary.resolve(ok(windowDetail({ detail_level: "primary", last_seen_at_ms: time + 100, active_count: 1 }))));
    expect(hook.result.current.detail).toMatchObject({ last_seen_at_ms: time + 200, active_count: 0 });
    expect(hook.result.current.windows[0]).toMatchObject({ last_seen_at_ms: time + 200, active_count: 0 });
  });

  it.each(["client", "authority", "scope"] as const)("clears the same selected key before the first render after a %s change", async mode => {
    const oldFull = deferred(), newPrimary = deferred(), newInventory = deferred(), newFull = deferred();
    let changed = false;
    let oldFullSignal: AbortSignal | undefined;
    const first = transport((path, body, signal) => {
      if (changed) return path === "windows" ? newInventory.promise : body.detail_level === "primary" ? newPrimary.promise : newFull.promise;
      if (path === "windows") return inventory();
      if (body.detail_level === "primary") return ok(windowDetail({ detail_level: "primary", source: "old-principal" }));
      oldFullSignal = signal;
      return oldFull.promise;
    });
    const second = transport((path, body) => path === "windows" ? newInventory.promise : body.detail_level === "primary" ? newPrimary.promise : newFull.promise);
    const seen: Array<WindowDetail | null> = [];
    const unauthorized = vi.fn();
    const hook = renderHook(({ client, projects }) => {
      const state = useWindowWorkspace(client, true, unauthorized, { initialWindowKey: key, projects });
      if (changed) seen.push(state.detail);
      return state;
    }, { initialProps: { client: first.client, projects: ["project-a"] } });
    await flush();
    expect(hook.result.current.detail?.source).toBe("old-principal");
    changed = true;
    if (mode === "authority") first.value.authorityRevision++;
    hook.rerender({ client: mode === "client" ? second.client : first.client, projects: mode === "scope" ? ["project-b"] : ["project-a"] });
    expect(seen.length).toBeGreaterThan(0);
    expect(seen.every(detail => detail === null)).toBe(true);
    expect(hook.result.current.windows).toEqual([]);
    expect(hook.result.current.total).toBe(0);
    expect(oldFullSignal?.aborted).toBe(true);
    await act(async () => oldFull.resolve(ok(windowDetail({ detail_level: "full", source: "old-principal" }))));
    expect(hook.result.current.detail).toBeNull();
    await act(async () => newPrimary.resolve(ok(windowDetail({ detail_level: "primary", source: "new-principal" }))));
    expect(hook.result.current.detail?.source).toBe("new-principal");
  });

  it.each([401, 403, 404])("clears inventory and metadata on HTTP %i", async status => {
    let fail = false;
    const { client } = transport(() => fail ? denied(status) : inventory());
    const unauthorized = vi.fn();
    const hook = renderHook(() => useWindowInventory(client, true, unauthorized));
    await flush();
    expect(hook.result.current.windows).toHaveLength(1);
    fail = true;
    act(() => hook.result.current.refresh());
    await flush();
    expect(hook.result.current.availability).toBe("denied");
    expect(hook.result.current.windows).toEqual([]);
    expect(hook.result.current.historicalWindows).toEqual([]);
    expect(hook.result.current.total).toBe(0);
    expect(hook.result.current.truncated).toBe(false);
    expect(unauthorized).toHaveBeenCalledTimes(status === 401 ? 1 : 0);
  });

  it.each([401, 403, 404])("clears detail and ignores pending history after primary HTTP %i", async status => {
    const full = deferred();
    let primaryCalls = 0;
    let fullSignal: AbortSignal | undefined;
    const { client } = transport((path, body, signal) => {
      if (path === "windows") return body.projection === "liveness" ? inventory([]) : inventory();
      if (body.detail_level === "primary") return ++primaryCalls === 1 ? ok(windowDetail({ detail_level: "primary" })) : denied(status);
      fullSignal = signal;
      return full.promise;
    });
    const unauthorized = vi.fn();
    const hook = renderHook(() => useWindowWorkspace(client, true, unauthorized, { initialWindowKey: key }));
    await flush();
    expect(hook.result.current.detail).not.toBeNull();
    await advance(3_000);
    expect(hook.result.current.detail).toBeNull();
    expect(hook.result.current.detailAvailability).toBe("denied");
    expect(fullSignal?.aborted).toBe(true);
    await act(async () => full.resolve(ok(windowDetail({ detail_level: "full" }))));
    expect(hook.result.current.detail).toBeNull();
    expect(unauthorized).toHaveBeenCalledTimes(status === 401 ? 1 : 0);
  });

  it.each([401, 403, 404])("clears already visible primary detail after full-history HTTP %i", async status => {
    const full = deferred();
    const { client } = transport((path, body) => path === "windows" ? inventory() : body.detail_level === "primary"
      ? ok(windowDetail({ detail_level: "primary" })) : full.promise);
    const unauthorized = vi.fn();
    const hook = renderHook(() => useWindowWorkspace(client, true, unauthorized, { initialWindowKey: key }));
    await flush();
    expect(hook.result.current.detail).not.toBeNull();
    await act(async () => full.resolve(denied(status)));
    expect(hook.result.current.detail).toBeNull();
    expect(hook.result.current.detailAvailability).toBe("denied");
    expect(unauthorized).toHaveBeenCalledTimes(status === 401 ? 1 : 0);
  });

  it("keeps history stale through healthy primary ticks and retries full-history failures only every 30 seconds", async () => {
    let fullCalls = 0;
    const retained = { started_at_ms: time - 1_000, ended_at_ms: time, duration_ms: 1_000, method: "tools/call", status: "success" };
    const { client } = transport((path, body) => path === "windows" ? inventory() : body.detail_level === "primary"
      ? ok(windowDetail({ detail_level: "primary" }))
      : ++fullCalls === 1 ? ok(windowDetail({ detail_level: "full", activity: [retained], activity_returned: 1 })) : denied(503));
    const unauthorized = vi.fn();
    const hook = renderHook(() => useWindowWorkspace(client, true, unauthorized, { initialWindowKey: key }));
    await flush();
    expect(fullCalls).toBe(1);
    for (let tick = 0; tick < 10; tick++) await advance(3_000);
    expect(fullCalls).toBe(2);
    expect(hook.result.current.detailAvailability).toBe("stale");
    expect(hook.result.current.detail?.activity).toContainEqual(retained);
    for (let tick = 0; tick < 9; tick++) await advance(3_000);
    expect(fullCalls).toBe(2);
    expect(hook.result.current.detailAvailability).toBe("stale");
    await advance(3_000);
    expect(fullCalls).toBe(3);
    expect(unauthorized).not.toHaveBeenCalled();
  });

  it("bounds active-to-idle completion reads to four non-selected Windows", async () => {
    const rows = Array.from({ length: 7 }, (_, index) => summary({ client_window_key: index === 0 ? key : String(index).repeat(64) }));
    let liveCalls = 0;
    const completed: string[] = [];
    const { client } = transport((_path, body) => {
      if (body.projection === "liveness") return inventory(++liveCalls === 1 ? rows.map(row => ({ ...row, active_count: 1 })) : []);
      if (body.client_window_key) {
        completed.push(body.client_window_key);
        return inventory(rows.filter(row => row.client_window_key === body.client_window_key));
      }
      return inventory(rows);
    });
    const unauthorized = vi.fn();
    renderHook(() => useWindowInventory(client, true, unauthorized, { detailWindowKey: key }));
    await flush();
    await advance(3_000);
    await advance(3_000);
    expect(completed).toHaveLength(4);
    expect(completed).not.toContain(key);
    await advance(3_000);
    expect(completed).toHaveLength(4);
  });

  it("keeps each poll single-flight, aborts hidden requests, and ignores their late completion", async () => {
    let visibility: DocumentVisibilityState = "visible";
    vi.spyOn(document, "visibilityState", "get").mockImplementation(() => visibility);
    const requests: Array<{ path: string; body: Body; signal?: AbortSignal; pending: ReturnType<typeof deferred> }> = [];
    const { client } = transport((path, body, signal) => {
      const pending = deferred();
      requests.push({ path, body, signal, pending });
      return pending.promise;
    });
    const unauthorized = vi.fn();
    const hook = renderHook(() => useWindowWorkspace(client, true, unauthorized, { initialWindowKey: key }));
    await advance(9_000);
    expect(requests.filter(request => request.path === "window")).toHaveLength(1);
    expect(requests.filter(request => request.body.projection === "inventory")).toHaveLength(1);
    expect(requests.filter(request => request.body.projection === "liveness")).toHaveLength(1);
    act(() => { visibility = "hidden"; document.dispatchEvent(new Event("visibilitychange")); });
    expect(requests.every(request => request.signal?.aborted)).toBe(true);
    await act(async () => {
      for (const request of requests) request.pending.resolve(request.path === "window" ? ok(windowDetail()) : inventory());
    });
    expect(hook.result.current.detail).toBeNull();
    expect(hook.result.current.windows).toEqual([]);
    await advance(60_000);
    expect(requests).toHaveLength(3);
    act(() => {
      visibility = "visible";
      document.dispatchEvent(new Event("visibilitychange"));
      window.dispatchEvent(new Event("focus"));
      window.dispatchEvent(new Event("online"));
    });
    await advance(150);
    expect(requests).toHaveLength(6);
    hook.unmount();
    expect(requests.every(request => request.signal?.aborted)).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
  });
});
