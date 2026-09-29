import { act, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ObservationRequest } from "../src/runtime-v2/state/useObservationRequest.js";
import { useGoalWorkspace } from "../src/runtime-v2/state/useGoalWorkspace.js";
import { useSessionWorkspace } from "../src/runtime-v2/state/useSessionWorkspace.js";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { sessionDetail } from "./fixtures.js";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}
const ok = (data: unknown) => ({ ok: true, status: 200, data });
afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); });

it("releases null/failed reads and coalesces explicit refresh without canceling slow work", async () => {
  const slot = new ObservationRequest();
  const receive = vi.fn();
  await slot.run(async () => null, receive);
  expect(slot.pending).toBe(false);
  await slot.run(async () => { throw new Error("offline"); }, receive);
  expect(receive).toHaveBeenCalledTimes(2);
  expect(slot.pending).toBe(false);
  const slow = deferred<number>();
  const refresh = vi.fn();
  const promise = slot.run(() => slow.promise, receive);
  slot.requestRefresh(refresh); slot.requestRefresh(refresh);
  expect(refresh).not.toHaveBeenCalled();
  const ignored = vi.fn(async () => 99);
  await slot.run(ignored, receive);
  expect(ignored).not.toHaveBeenCalled();
  slow.resolve(7); await promise;
  expect(receive).toHaveBeenLastCalledWith(7);
  expect(refresh).toHaveBeenCalledTimes(1);
});

it("a projection error is not replayed as a failed request and still releases the slot", async () => {
  const slot = new ObservationRequest();
  const receive = vi.fn(() => { throw new Error("projection bug"); });
  await expect(slot.run(async () => 1, receive)).rejects.toThrow("projection bug");
  expect(receive).toHaveBeenCalledTimes(1);
  expect(slot.pending).toBe(false);
});

it("canceled old completions cannot release a replacement slot or resurrect a queued refresh", async () => {
  const slot = new ObservationRequest();
  const first = deferred<number>(); const second = deferred<number>();
  const receive = vi.fn(); const staleRefresh = vi.fn();
  let signal!: AbortSignal;
  const old = slot.run(s => { signal = s; return first.promise; }, receive);
  slot.requestRefresh(staleRefresh); slot.cancel();
  expect(signal.aborted).toBe(true);
  const current = slot.run(() => second.promise, receive);
  first.resolve(1); await old;
  expect(slot.pending).toBe(true); expect(receive).not.toHaveBeenCalled();
  expect(staleRefresh).not.toHaveBeenCalled();
  second.resolve(2); await current;
  expect(slot.pending).toBe(false); expect(receive).toHaveBeenCalledWith(2);
});

it("Goal reads recover after null, coalesce resume events and stop periodic observation when hidden", async () => {
  vi.useFakeTimers();
  const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
  const post = vi.fn().mockResolvedValueOnce(null).mockResolvedValue(ok({ goals: [], total: 0 }));
  const unauthorized = vi.fn();
  const client = { post } as unknown as RuntimeV2Client;
  const { result, unmount } = renderHook(() => useGoalWorkspace(client, true, unauthorized));
  await act(async () => {});
  expect(result.current.availability).toBe("error");
  act(() => { window.dispatchEvent(new Event("focus")); window.dispatchEvent(new Event("online")); });
  await act(async () => { await vi.advanceTimersByTimeAsync(150); });
  expect(result.current.availability).toBe("available"); expect(post).toHaveBeenCalledTimes(2);
  visibility.mockReturnValue("hidden");
  act(() => document.dispatchEvent(new Event("visibilitychange")));
  await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
  expect(post).toHaveBeenCalledTimes(2);
  unmount(); expect(vi.getTimerCount()).toBe(0);
});

it("Session detail denial does not become an automatic retry loop", async () => {
  vi.useFakeTimers();
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
  const detail = sessionDetail();
  let detailReads = 0;
  const post = vi.fn(async (path: string) => {
    if (path === "workflow-session") {
      detailReads++;
      return { ok: false, status: 404, data: null };
    }
    throw new Error(`unexpected path ${path}`);
  });
  const location = { projectId: "agent:runner:demo", sessionId: detail.session_id, runner: "runner", projectName: "Demo" };
  const client = { post } as unknown as RuntimeV2Client;
  const unauthorized = vi.fn();
  const { result, unmount } = renderHook(() => useSessionWorkspace(client, true, location, unauthorized, { loadMessages: false }));
  await act(async () => {});
  expect(result.current.detailAvailability).toBe("denied");
  expect(detailReads).toBe(1);
  await act(async () => { await vi.advanceTimersByTimeAsync(20_000); });
  expect(detailReads).toBe(1);
  unmount();
  expect(vi.getTimerCount()).toBe(0);
});

it("Session detail and message observations do not cancel or block each other", async () => {
  vi.useFakeTimers();
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
  const detail = sessionDetail();
  const slow = deferred<ReturnType<typeof ok>>();
  let detailReads = 0; let messageReads = 0; let slowSignal!: AbortSignal;
  const post = vi.fn(async (path: string, _args: unknown, signal: AbortSignal) => {
    if (path === "workflow-session") {
      detailReads++; slowSignal = signal; return slow.promise;
    }
    if (path === "workflow-session-messages") {
      messageReads++; return ok({ session_id: detail.session_id, messages: [] });
    }
    throw new Error(`unexpected path ${path}`);
  });
  const location = { projectId: "agent:runner:demo", sessionId: detail.session_id, runner: "runner", projectName: "Demo" };
  const unauthorized = vi.fn();
  const client = { post } as unknown as RuntimeV2Client;
  const { result, unmount } = renderHook(() => useSessionWorkspace(client, true, location, unauthorized));
  await act(async () => {});
  // Flush each polling interval as a separate render, as in the browser.
  await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
  await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
  expect(detailReads).toBe(1); expect(messageReads).toBe(3); expect(slowSignal.aborted).toBe(false);
  await act(async () => slow.resolve(ok(detail)));
  expect(result.current.detail?.session_id).toBe(detail.session_id);
  unmount(); expect(vi.getTimerCount()).toBe(0);
});
