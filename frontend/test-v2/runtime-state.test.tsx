import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { useRuntimeOverview } from "../src/runtime-v2/state/useRuntimeOverview.js";
import { useWindowWorkspace } from "../src/runtime-v2/state/useWindowWorkspace.js";
import { useSessionWorkspace } from "../src/runtime-v2/state/useSessionWorkspace.js";
import { sessionDetail, runtimeOverview } from "./fixtures.js";

type ResponseShape = { ok: boolean; status: number; data: unknown };

function clientWith(responses: Array<ResponseShape | Promise<ResponseShape>>): RuntimeV2Client {
  const post = vi.fn(async () => {
    const next = responses.shift();
    return await next!;
  });
  return { post } as unknown as RuntimeV2Client;
}

describe("Runtime server state", () => {
  it("distinguishes loading, available, stale, error and denied", async () => {
    let resolveFirst!: (value: ResponseShape) => void;
    const pending = new Promise<ResponseShape>((resolve) => { resolveFirst = resolve; });
    const client = clientWith([
      pending,
      { ok: false, status: 500, data: null },
    ]);
    const unauthorized = vi.fn();
    const { result } = renderHook(() => useRuntimeOverview(client, true, unauthorized));

    await waitFor(() => expect(result.current.availability).toBe("loading"));
    await act(async () => resolveFirst({ ok: true, status: 200, data: runtimeOverview() }));
    await waitFor(() => expect(result.current.availability).toBe("available"));

    act(() => result.current.refresh());
    await waitFor(() => expect(result.current.availability).toBe("stale"));
    expect(result.current.data?.service).toBe("WebCodex");

    const errorClient = clientWith([{ ok: false, status: 500, data: null }]);
    const errorHook = renderHook(() => useRuntimeOverview(errorClient, true, unauthorized));
    await waitFor(() => expect(errorHook.result.current.availability).toBe("error"));

    const deniedClient = clientWith([{ ok: false, status: 403, data: null }]);
    const deniedHook = renderHook(() => useRuntimeOverview(deniedClient, true, unauthorized));
    await waitFor(() => expect(deniedHook.result.current.availability).toBe("denied"));
  });

  it("clears stale Window inventory metadata when observation authority is revoked", async () => {
    const key = "a".repeat(64);
    const client = clientWith([
      {
        ok: true,
        status: 200,
        data: {
          windows: [{ client_window_key: key, source: "openai-session", last_seen_at_ms: 1, active_count: 0, linked_session_count: 0, recorder_gap_count: 0 }],
          returned: 1,
          total: 7,
          truncated: true,
          visibility: { scope: "principal" },
        },
      },
      { ok: false, status: 403, data: null },
    ]);
    const unauthorized = vi.fn();
    const { result } = renderHook(() => useWindowWorkspace(client, true, unauthorized, { loadDetail: false, refreshMs: 60_000 }));

    await waitFor(() => expect(result.current.availability).toBe("available"));
    expect(result.current.total).toBe(7);
    expect(result.current.truncated).toBe(true);

    act(() => result.current.refresh());
    await waitFor(() => expect(result.current.availability).toBe("denied"));
    expect(result.current.windows).toEqual([]);
    expect(result.current.total).toBe(0);
    expect(result.current.truncated).toBe(false);
  });

  it("routes unauthorized responses to the credential boundary", async () => {
    const unauthorized = vi.fn();
    const client = clientWith([{ ok: false, status: 401, data: null }]);
    renderHook(() => useRuntimeOverview(client, true, unauthorized));
    await waitFor(() => expect(unauthorized).toHaveBeenCalledTimes(1));
  });
});

it("does not display the previous Window while the newly selected detail is pending", async () => {
  const first = "a".repeat(64);
  const second = "b".repeat(64);
  let resolveSecond!: (value: ResponseShape) => void;
  const pending = new Promise<ResponseShape>(resolve => { resolveSecond = resolve; });
  const detail = (key: string) => ({ client_window_key: key, linked_sessions: [], activity: [], active_requests: [] });
  const client = { post: vi.fn(async (path, payload) => path === "windows"
    ? { ok: true, status: 200, data: { windows: [first, second].map(client_window_key => ({ client_window_key })), total: 2 } }
    : payload.client_window_key === first ? { ok: true, status: 200, data: detail(first) } : pending) } as unknown as RuntimeV2Client;
  const unauthorized = vi.fn();
  const { result } = renderHook(() => useWindowWorkspace(client, true, unauthorized));
  await waitFor(() => expect(result.current.detail?.client_window_key).toBe(first));
  act(() => result.current.select(second));
  expect(result.current.detail).toBeNull();
  await act(async () => resolveSecond({ ok: true, status: 200, data: detail(second) }));
  await waitFor(() => expect(result.current.detail?.client_window_key).toBe(second));
});

it("lets slow Window polls finish and discovers new Windows independently of slow detail", async () => {
  vi.useFakeTimers();
  try {
    const first = "a".repeat(64);
    const second = "b".repeat(64);
    const row = (client_window_key: string) => ({ client_window_key, source: "openai-session", last_seen_at_ms: 1, active_count: 0, linked_session_count: 0, recorder_gap_count: 0 });
    const list = (keys: string[]) => ({ ok: true, status: 200, data: { windows: keys.map(row), total: keys.length } });
    let resolveList!: (value: ResponseShape) => void;
    let resolveDetail!: (value: ResponseShape) => void;
    const pendingList = new Promise<ResponseShape>(resolve => { resolveList = resolve; });
    const pendingDetail = new Promise<ResponseShape>(resolve => { resolveDetail = resolve; });
    const signals: AbortSignal[] = [];
    let lists = 0;
    let details = 0;
    const client = { post: vi.fn(async (path: string, _payload: unknown, signal: AbortSignal) => {
      signals.push(signal);
      if (path === "windows") return ++lists === 1 ? pendingList : list([first, second]);
      details++;
      return pendingDetail;
    }) } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    const { result, unmount } = renderHook(() => useWindowWorkspace(client, true, unauthorized));
    await act(async () => { await vi.advanceTimersByTimeAsync(9_000); });
    expect(lists).toBe(1);
    expect(signals[0].aborted).toBe(false);
    await act(async () => resolveList(list([first])));
    expect(result.current.windows).toHaveLength(1);
    expect(details).toBe(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(9_000); });
    expect(result.current.windows).toHaveLength(2);
    expect(result.current.selectedKey).toBe(first);
    expect(details).toBe(1);
    expect(signals[1].aborted).toBe(false);
    await act(async () => resolveDetail({ ok: true, status: 200, data: { client_window_key: first, linked_sessions: [], activity: [], active_requests: [] } }));
    expect(result.current.detail?.client_window_key).toBe(first);
    const beforeFocus = lists;
    await act(async () => window.dispatchEvent(new Event("focus")));
    expect(lists).toBe(beforeFocus + 1);
    unmount();
    expect(vi.getTimerCount()).toBe(0);
  } finally {
    vi.useRealTimers();
  }
});


it("does not cancel a slow active Session refresh on the next five-second tick", async () => {
  vi.useFakeTimers();
  try {
    let resolveDetail!: (value: ResponseShape) => void;
    const pending = new Promise<ResponseShape>(resolve => { resolveDetail = resolve; });
    let calls = 0;
    let slowSignal: AbortSignal | undefined;
    const detail = sessionDetail();
    const client = { post: vi.fn(async (path: string, _payload: unknown, signal: AbortSignal) => {
      if (path === "workflow-session") {
        if (++calls > 1) { slowSignal = signal; return pending; }
        return { ok: true, status: 200, data: detail };
      }
      return { ok: true, status: 200, data: { messages: [] } };
    }) } as unknown as RuntimeV2Client;
    const location = { projectId: "agent:special:webcodex", projectName: "WebCodex", runner: "special", sessionId: detail.session_id };
    const unauthorized = vi.fn();
    const { result, unmount } = renderHook(() => useSessionWorkspace(client, true, location, unauthorized));
    await act(async () => {});
    await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
    expect(calls).toBe(2);
    await act(async () => { await vi.advanceTimersByTimeAsync(15_000); });
    expect(calls).toBe(2);
    expect(slowSignal?.aborted).toBe(false);
    await act(async () => resolveDetail({ ok: true, status: 200, data: { ...detail, title: "Fresh evidence" } }));
    expect(result.current.detail?.title).toBe("Fresh evidence");
    unmount();
  } finally {
    vi.useRealTimers();
  }
});
