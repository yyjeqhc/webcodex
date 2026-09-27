import { act, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { useGoalWorkspace } from "../src/runtime-v2/state/useGoalWorkspace.js";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";

const ok = (data: unknown) => ({ ok: true, status: 200, data });
const goals = [{ goal_id: "a", lifecycle: "active", updated_at_unix_ms: 2 }, { goal_id: "b", lifecycle: "active", updated_at_unix_ms: 1 }];
const detail = (id: string) => ok({ goal: { summary: { goal_id: id } } });
function deferred() {
  let resolve!: (value: unknown) => void;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}
afterEach(() => vi.useRealTimers());

it("preserves slow Goal list and detail reads while polling idle resources independently", async () => {
  vi.useFakeTimers();
  const list = deferred();
  const full = deferred();
  const signals: AbortSignal[] = [];
  let lists = 0;
  let details = 0;
  const client = { post: vi.fn(async (path: string, _payload: unknown, signal: AbortSignal) => {
    signals.push(signal);
    if (path === "goals") return ++lists === 1 ? list.promise : ok({ goals });
    details++;
    return full.promise;
  }) } as unknown as RuntimeV2Client;
  const unauthorized = vi.fn();
  const { result, unmount } = renderHook(() => useGoalWorkspace(client, true, unauthorized));
  await act(async () => { await vi.advanceTimersByTimeAsync(11_000); });
  expect(lists).toBe(1);
  expect(signals[0].aborted).toBe(false);
  await act(async () => list.resolve(ok({ goals })));
  expect(result.current.availability).toBe("available");
  expect(details).toBe(1);
  await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
  expect(lists).toBe(2);
  expect(details).toBe(1);
  expect(signals[1].aborted).toBe(false);
  await act(async () => full.resolve(detail("a")));
  expect(result.current.detail?.goal.summary.goal_id).toBe("a");
  unmount();
});

it("cancels old Goal detail on selection and disable, ignoring late responses", async () => {
  const old = deferred();
  const current = deferred();
  const signals: AbortSignal[] = [];
  const client = { post: vi.fn(async (path: string, payload: { goal_id: string }, signal: AbortSignal) => {
    if (path === "goals") return ok({ goals });
    signals.push(signal);
    return payload.goal_id === "a" ? old.promise : current.promise;
  }) } as unknown as RuntimeV2Client;
  const unauthorized = vi.fn();
  const { result, rerender, unmount } = renderHook(({ enabled }) => useGoalWorkspace(client, enabled, unauthorized), { initialProps: { enabled: true } });
  await act(async () => {});
  act(() => result.current.selectGoal("b"));
  expect(signals[0].aborted).toBe(true);
  await act(async () => old.resolve(detail("a")));
  expect(result.current.detail).toBeNull();
  rerender({ enabled: false });
  expect(signals[1].aborted).toBe(true);
  await act(async () => current.resolve(detail("b")));
  expect(result.current.detail).toBeNull();
  expect(result.current.detailAvailability).toBe("idle");
  unmount();
});
