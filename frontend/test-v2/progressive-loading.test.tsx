import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { useRuntimeOverview } from "../src/runtime-v2/state/useRuntimeOverview.js";
import { useProjects, PROJECT_PAGE_SIZE } from "../src/runtime-v2/state/useProjects.js";
import { useVisibleRefresh } from "../src/runtime-v2/state/useVisibleRefresh.js";
import { runtimeOverview, sessionItem } from "./fixtures.js";
import { useProjectSessions } from "../src/runtime-v2/state/useProjectSessions.js";

const ok = (data: unknown) => ({ ok: true, status: 200, data });

describe("progressive inventory boundaries", () => {
  it("renders registry facts before slow Session hydration completes", async () => {
    let resolveFull!: (value: ReturnType<typeof ok>) => void;
    const full = new Promise<ReturnType<typeof ok>>(resolve => { resolveFull = resolve; });
    const primary = { ...runtimeOverview(), detail_level: "primary", recent_sessions: { sessions: [], returned: 0, candidate_count: 0, truncated: false, scan_truncated: true } };
    const post = vi.fn(async (_path: string, args: { include_sessions?: boolean }) => args.include_sessions === false ? ok(primary) : full);
    const client = { post } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    const { result, unmount } = renderHook(() => useRuntimeOverview(client, true, unauthorized));
    await waitFor(() => expect(result.current.data?.detail_level).toBe("primary"));
    await waitFor(() => expect(post).toHaveBeenCalledTimes(2));
    expect(result.current.data?.projects).toEqual(primary.projects);
    expect(result.current.refreshing).toBe(true);
    await act(async () => resolveFull(ok({ ...runtimeOverview(), detail_level: "full" })));
    await waitFor(() => expect(result.current.data?.detail_level).toBe("full"));
    unmount();
  });

  it("Window work does not request a full fleet Session scan", async () => {
    const post = vi.fn(async () => ok({ ...runtimeOverview(), detail_level: "primary" }));
    const client = { post } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    const { result, unmount } = renderHook(() => useRuntimeOverview(client, true, unauthorized, false));
    await waitFor(() => expect(result.current.availability).toBe("available"));
    act(() => result.current.refresh());
    await waitFor(() => expect(post).toHaveBeenCalledTimes(2));
    for (const call of (post.mock.calls as unknown[][])) expect(call[1]).toEqual({ include_sessions: false });
    unmount();
  });

  it("138 Projects use explicit bounded pages and filters restart at the first page", async () => {
    const post = vi.fn(async (_path: string, args: { limit: number; query?: string }) => {
      const total = args.query ? 2 : 138;
      return ok({ total, truncated: args.limit < total, projects: Array.from({ length: Math.min(total, args.limit) }, (_, index) => ({ id: `agent:runner:p${index}`, client_id: "runner", connected: true })) });
    });
    const client = { post } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    const { result, unmount } = renderHook(() => useProjects(client, true, unauthorized));
    await waitFor(() => expect(result.current.projects).toHaveLength(PROJECT_PAGE_SIZE));
    expect(post).toHaveBeenCalledTimes(1);
    act(() => result.current.loadMore());
    await waitFor(() => expect(result.current.projects).toHaveLength(PROJECT_PAGE_SIZE * 2));
    act(() => result.current.showLess());
    await waitFor(() => expect(result.current.projects).toHaveLength(PROJECT_PAGE_SIZE));
    act(() => result.current.setQuery("specific"));
    await waitFor(() => expect(result.current.projects).toHaveLength(2));
    expect(post.mock.calls.at(-1)?.[1].limit).toBe(PROJECT_PAGE_SIZE);
    expect(result.current.canShowLess).toBe(false);
    unmount();
  });

  it("a failed Session inventory read releases single-flight so focus can recover", async () => {
    const post = vi.fn().mockResolvedValueOnce(null).mockResolvedValue(ok({ sessions: [sessionItem()], total: 1, truncated: false }));
    const client = { post } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    const { result, unmount } = renderHook(() => useProjectSessions(client, true, "agent:runner:demo", unauthorized));
    await waitFor(() => expect(result.current.availability).toBe("error"));
    act(() => window.dispatchEvent(new Event("focus")));
    await waitFor(() => expect(result.current.availability).toBe("available"));
    expect(result.current.sessions).toHaveLength(1);
    expect(post).toHaveBeenCalledTimes(2);
    unmount();
  });

  it("coalesces visibility/focus/online and stops all timer reads while hidden", async () => {
    vi.useFakeTimers();
    const state = vi.spyOn(document, "visibilityState", "get");
    state.mockReturnValue("visible");
    try {
      const refresh = vi.fn();
      const { unmount } = renderHook(() => useVisibleRefresh(true, refresh, 10_000));
      await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
      expect(refresh).toHaveBeenCalledTimes(1);
      state.mockReturnValue("hidden");
      act(() => document.dispatchEvent(new Event("visibilitychange")));
      await act(async () => { await vi.advanceTimersByTimeAsync(120_000); });
      expect(refresh).toHaveBeenCalledTimes(1);
      state.mockReturnValue("visible");
      act(() => {
        document.dispatchEvent(new Event("visibilitychange"));
        window.dispatchEvent(new Event("focus"));
        window.dispatchEvent(new Event("online"));
      });
      await act(async () => { await vi.advanceTimersByTimeAsync(150); });
      expect(refresh).toHaveBeenCalledTimes(2);
      unmount();
      expect(vi.getTimerCount()).toBe(0);
    } finally { state.mockRestore(); vi.useRealTimers(); }
  });
});
