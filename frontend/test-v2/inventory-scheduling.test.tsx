import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { projectInventoryKey } from "../src/runtime-v2/model/projectInventory.js";
import { useProjects } from "../src/runtime-v2/state/useProjects.js";
import { useRuntimeOverview } from "../src/runtime-v2/state/useRuntimeOverview.js";
import { runtimeOverview } from "./fixtures.js";

const ok = (data: unknown) => ({ ok: true, status: 200, data });
const primary = () => ({ ...runtimeOverview(), detail_level: "primary" });
const projects = () => ({ projects: runtimeOverview().projects, total: 1, truncated: false });
const tick = async (ms: number) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(0);
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.useRealTimers(); });

describe.each(["overview", "projects"] as const)("%s inventory scheduling", kind => {
  const response = () => ok(kind === "overview" ? primary() : projects());
  const baseMs = kind === "overview" ? 10_000 : 30_000;
  function useInventory(client: RuntimeV2Client, unauthorized: () => void) {
    const overview = useRuntimeOverview(client, kind === "overview", unauthorized, false);
    const list = useProjects(client, kind === "projects", unauthorized);
    return kind === "overview" ? overview : list;
  }

  it("coalesces explicit refresh and a resume burst behind a slow request", async () => {
    const finish: Array<(value: ReturnType<typeof ok>) => void> = [];
    const post = vi.fn(() => new Promise<ReturnType<typeof ok>>(resolve => finish.push(resolve)));
    const client = { post } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    const hook = renderHook(() => useInventory(client, unauthorized));
    expect(post).toHaveBeenCalledTimes(1);
    act(() => { hook.result.current.refresh(); hook.result.current.refresh(); hook.result.current.refresh(); });
    act(() => {
      window.dispatchEvent(new Event("focus"));
      window.dispatchEvent(new Event("online"));
      document.dispatchEvent(new Event("visibilitychange"));
    });
    await tick(baseMs + 150);
    expect(post).toHaveBeenCalledTimes(1);
    await act(async () => finish[0](response()));
    expect(post).toHaveBeenCalledTimes(2);
    expect(hook.result.current.refreshing).toBe(true);
    await act(async () => finish[1](response()));
    expect(hook.result.current.availability).toBe("available");
    expect(post).toHaveBeenCalledTimes(2);
  });

  it.each([false, true])("a resume queued before failure respects backoff (explicit retry: %s)", async explicit => {
    const finish: Array<(value: ReturnType<typeof ok>) => void> = [];
    const post = vi.fn(() => new Promise<ReturnType<typeof ok>>(resolve => finish.push(resolve)));
    const client = { post } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    const hook = renderHook(() => useInventory(client, unauthorized));
    if (explicit) act(() => hook.result.current.refresh());
    act(() => window.dispatchEvent(new Event("focus")));
    await tick(150);
    await act(async () => finish[0]({ ok: false, status: 503, data: null }));
    expect(post).toHaveBeenCalledTimes(explicit ? 2 : 1);
  });

  it("backs off 503 observations, preserves stale data, and lets an explicit retry recover", async () => {
    const post = vi.fn().mockResolvedValueOnce(response()).mockResolvedValue({ ok: false, status: 503, data: null });
    const client = { post } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    const hook = renderHook(() => useInventory(client, unauthorized));
    await act(async () => {});
    act(() => hook.result.current.refresh());
    await act(async () => {});
    expect(hook.result.current.availability).toBe("stale");
    expect(post).toHaveBeenCalledTimes(2);
    // Repeated visible/focus notifications must not turn a failing endpoint into
    // a 150 ms retry loop; automatic retries obey the same failure deadline.
    for (let index = 0; index < 10; index++) {
      act(() => window.dispatchEvent(new Event("focus")));
      await tick(150);
    }
    await tick(baseMs * 2 - 1_501);
    expect(post).toHaveBeenCalledTimes(2);
    post.mockResolvedValue(response());
    act(() => hook.result.current.refresh());
    await act(async () => {});
    expect(post).toHaveBeenCalledTimes(3);
    expect(hook.result.current.availability).toBe("available");
    expect(unauthorized).not.toHaveBeenCalled();
  });

  it.each([false, true])("a queued resume cannot replay a denied read (explicit retry: %s)", async explicit => {
    const finish: Array<(value: ReturnType<typeof ok>) => void> = [];
    const post = vi.fn(() => new Promise<ReturnType<typeof ok>>(resolve => finish.push(resolve)));
    const client = { post } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    const hook = renderHook(() => useInventory(client, unauthorized));
    if (explicit) act(() => hook.result.current.refresh());
    act(() => window.dispatchEvent(new Event("focus")));
    await tick(150);
    await act(async () => finish[0]({ ok: false, status: 403, data: null }));
    expect(post).toHaveBeenCalledTimes(explicit ? 2 : 1);
  });

  it("stops periodic denied reads but permits a deliberate retry", async () => {
    const post = vi.fn().mockResolvedValueOnce({ ok: false, status: 403, data: null }).mockResolvedValue(response());
    const client = { post } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    const hook = renderHook(() => useInventory(client, unauthorized));
    await act(async () => {});
    expect(hook.result.current.availability).toBe("denied");
    await tick(120_000);
    expect(post).toHaveBeenCalledTimes(1);
    act(() => hook.result.current.refresh());
    await act(async () => {});
    expect(post).toHaveBeenCalledTimes(2);
    expect(hook.result.current.availability).toBe("available");
  });
});

it("does not spin if a full overview request still returns only primary facts", async () => {
  const post = vi.fn().mockResolvedValue(ok(primary()));
  const client = { post } as unknown as RuntimeV2Client;
  const unauthorized = vi.fn();
  const hook = renderHook(() => useRuntimeOverview(client, true, unauthorized));
  await act(async () => {});
  expect(post).toHaveBeenCalledTimes(2);
  await tick(29_999);
  expect(post).toHaveBeenCalledTimes(2);
  await tick(1);
  expect(post).toHaveBeenCalledTimes(3);
  expect(hook.result.current.data?.detail_level).toBe("primary");
});

it("clears observed overview facts on an authority change even on the same client", async () => {
  let finish!: (value: ReturnType<typeof ok>) => void;
  const post = vi.fn().mockResolvedValueOnce(ok(primary()))
    .mockImplementation(() => new Promise<ReturnType<typeof ok>>(resolve => { finish = resolve; }));
  const client = { authorityRevision: 0, post } as unknown as RuntimeV2Client;
  const unauthorized = vi.fn();
  const hook = renderHook(({ revision }) => useRuntimeOverview(client, true, unauthorized, false), { initialProps: { revision: 0 } });
  await act(async () => {});
  expect(hook.result.current.data).not.toBeNull();
  (client as unknown as { authorityRevision: number }).authorityRevision = 1;
  hook.rerender({ revision: 1 });
  expect(hook.result.current.data).toBeNull();
  await act(async () => finish(ok(primary())));
  expect(hook.result.current.availability).toBe("available");
});

it.each(["overview", "projects"] as const)("never returns previous-authority facts on the first %s render", async kind => {
  const post = vi.fn().mockResolvedValueOnce(ok(kind === "overview" ? primary() : projects()))
    .mockImplementation(() => new Promise(() => {}));
  const client = { authorityRevision: 0, post } as unknown as RuntimeV2Client;
  const unauthorized = vi.fn();
  const renders: Array<{ revision: number; visible: boolean }> = [];
  const hook = renderHook(({ revision }) => {
    const overview = useRuntimeOverview(client, kind === "overview", unauthorized, false);
    const list = useProjects(client, kind === "projects", unauthorized);
    renders.push({ revision, visible: kind === "overview" ? overview.data !== null : list.projects.length > 0 });
    return { overview, list };
  }, { initialProps: { revision: 0 } });
  await act(async () => {});
  expect(renders.at(-1)?.visible).toBe(true);
  (client as unknown as { authorityRevision: number }).authorityRevision = 1;
  hook.rerender({ revision: 1 });
  expect(renders.filter(render => render.revision === 1).every(render => !render.visible)).toBe(true);
});

it("never returns the previous Project filter on its first replacement render", async () => {
  const post = vi.fn().mockResolvedValueOnce(ok(projects())).mockImplementation(() => new Promise(() => {}));
  const client = { post } as unknown as RuntimeV2Client;
  const unauthorized = vi.fn();
  const renders: Array<{ runner: string; rows: number }> = [];
  const hook = renderHook(() => {
    const list = useProjects(client, true, unauthorized);
    renders.push({ runner: list.runner, rows: list.projects.length });
    return list;
  });
  await act(async () => {});
  expect(renders.at(-1)?.rows).toBeGreaterThan(0);
  act(() => hook.result.current.setRunner("replacement"));
  expect(renders.filter(render => render.runner === "replacement").every(render => render.rows === 0)).toBe(true);
});

it("refreshes Projects on inventory change without treating every heartbeat as invalidation", async () => {
  const overview = runtimeOverview();
  overview.runners[0].project_inventory_generation = "generation-1";
  const first = projectInventoryKey(overview);
  const heartbeat = projectInventoryKey({ ...overview, runners: overview.runners.map(runner => ({ ...runner, last_seen_age_secs: 9, active_jobs: 3 })) });
  expect(heartbeat).toBe(first);
  const next = projectInventoryKey({ ...overview, runners: overview.runners.map(runner => ({ ...runner, project_inventory_generation: "generation-2" })) });
  expect(next).not.toBe(first);
  const finish: Array<(value: ReturnType<typeof ok>) => void> = [];
  const post = vi.fn(() => new Promise<ReturnType<typeof ok>>(resolve => finish.push(resolve)));
  const client = { post } as unknown as RuntimeV2Client;
  const unauthorized = vi.fn();
  const hook = renderHook(({ inventoryKey }) => useProjects(client, true, unauthorized, { inventoryKey }),
    { initialProps: { inventoryKey: undefined as string | undefined } });
  hook.rerender({ inventoryKey: first });
  hook.rerender({ inventoryKey: heartbeat });
  expect(post).toHaveBeenCalledTimes(1);
  hook.rerender({ inventoryKey: next });
  hook.rerender({ inventoryKey: next + "-another-update" });
  await act(async () => finish[0](ok(projects())));
  expect(post).toHaveBeenCalledTimes(2);
  await act(async () => finish[1](ok(projects())));
  expect(post).toHaveBeenCalledTimes(2);
});

it.each(["after-denial", "during-pending"] as const)("inventory invalidation honors denial (%s), while manual retry recovers", async timing => {
  const finish: Array<(value: ReturnType<typeof ok>) => void> = [];
  const post = vi.fn(() => new Promise<ReturnType<typeof ok>>(resolve => finish.push(resolve)));
  const client = { post } as unknown as RuntimeV2Client;
  const unauthorized = vi.fn();
  const hook = renderHook(({ inventoryKey }) => useProjects(client, true, unauthorized, { inventoryKey }),
    { initialProps: { inventoryKey: "generation-1" } });
  if (timing === "after-denial") await act(async () => finish[0]({ ok: false, status: 403, data: null }));
  hook.rerender({ inventoryKey: "generation-2" });
  if (timing === "during-pending") await act(async () => finish[0]({ ok: false, status: 403, data: null }));
  expect(hook.result.current.availability).toBe("denied");
  expect(post).toHaveBeenCalledTimes(1);
  act(() => hook.result.current.refresh());
  expect(post).toHaveBeenCalledTimes(2);
  await act(async () => finish[1](ok(projects())));
  expect(hook.result.current.availability).toBe("available");
  expect(hook.result.current.projects.length).toBeGreaterThan(0);
});

it("drops a queued refresh from an obsolete Project filter", async () => {
  const finish: Array<(value: ReturnType<typeof ok>) => void> = [];
  const post = vi.fn(() => new Promise<ReturnType<typeof ok>>(resolve => finish.push(resolve)));
  const client = { post } as unknown as RuntimeV2Client;
  const unauthorized = vi.fn();
  const hook = renderHook(() => useProjects(client, true, unauthorized));
  act(() => { hook.result.current.refresh(); hook.result.current.setRunner("new-runner"); });
  expect(post).toHaveBeenCalledTimes(2);
  await act(async () => finish[0](ok(projects())));
  expect(post).toHaveBeenCalledTimes(2);
  expect(hook.result.current.projects).toEqual([]);
  await act(async () => finish[1](ok({ projects: [], total: 0, truncated: false })));
  expect(post).toHaveBeenCalledTimes(2);
  expect(hook.result.current.availability).toBe("available");
});
