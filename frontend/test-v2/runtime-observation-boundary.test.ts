import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { useWindowWorkspace } from "../src/runtime-v2/state/useWindowWorkspace.js";
import { windowDetail } from "./fixtures.js";

const observation = { observation: true } as const;
const response = (data: unknown, status = 200) => new Response(JSON.stringify(data), {
  status, headers: { "Content-Type": "application/json" },
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}
afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); vi.useRealTimers(); });

describe("bounded Runtime observation transport", () => {
  it("coalesces identical concurrent reads, isolates consumer cancellation, and does not cache results", async () => {
    const pending = deferred<Response>();
    const fetch = vi.fn().mockReturnValueOnce(pending.promise).mockResolvedValue(response({ value: 2 }));
    vi.stubGlobal("fetch", fetch);
    const client = new RuntimeV2Client();
    const firstOwner = new AbortController();
    const first = client.post("overview", {}, firstOwner.signal, observation);
    const second = client.post("overview", {}, undefined, observation);
    expect(fetch).toHaveBeenCalledTimes(1);
    firstOwner.abort();
    expect(await first).toBeNull();
    expect(fetch.mock.calls[0][1].signal.aborted).toBe(false);
    pending.resolve(response({ value: 1 }));
    expect(await second).toMatchObject({ ok: true, data: { value: 1 } });
    expect(await client.post("overview", {}, undefined, observation)).toMatchObject({ data: { value: 2 } });
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("expires a hung read, releases its slot, and ignores a delayed unauthorized completion", async () => {
    vi.useFakeTimers();
    const old = deferred<Response>();
    const fetch = vi.fn().mockReturnValueOnce(old.promise).mockResolvedValue(response({ recovered: true }));
    vi.stubGlobal("fetch", fetch);
    const client = new RuntimeV2Client();
    const first = client.post("window", { client_window_key: "window-a" }, undefined, observation);
    const settled = vi.fn();
    void first.then(settled);
    await vi.advanceTimersByTimeAsync(29_999);
    expect(settled).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(await first).toMatchObject({ ok: false, status: 0 });
    expect(fetch.mock.calls[0][1].signal.aborted).toBe(true);
    expect(await client.post("window", { client_window_key: "window-a" }, undefined, observation))
      .toMatchObject({ ok: true, data: { recovered: true } });
    old.resolve(response({ error: "old unauthorized" }, 401));
    await Promise.resolve();
    expect(settled).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("bounds the entire response body read, not only response headers", async () => {
    vi.useFakeTimers();
    const body = deferred<unknown>();
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: true, status: 200, json: () => body.promise }));
    const client = new RuntimeV2Client();
    const call = client.post("overview", {}, undefined, observation);
    await vi.advanceTimersByTimeAsync(30_000);
    expect(await call).toMatchObject({ ok: false, status: 0 });
    body.resolve({ late: true });
    await Promise.resolve();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("aborts and releases an unobserved request even if transport ignores abort", async () => {
    vi.useFakeTimers();
    const fetch = vi.fn(() => new Promise<Response>(() => {}));
    vi.stubGlobal("fetch", fetch);
    const client = new RuntimeV2Client();
    const owner = new AbortController();
    const call = client.post("overview", {}, owner.signal, observation);
    owner.abort();
    expect(await call).toBeNull();
    expect(fetch.mock.calls[0][1].signal.aborted).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("revokes old observers immediately and never coalesces across credentials or clients", async () => {
    const old = deferred<Response>();
    const fetch = vi.fn().mockReturnValueOnce(old.promise).mockResolvedValue(response({ owner: "new" }));
    vi.stubGlobal("fetch", fetch);
    const client = new RuntimeV2Client();
    client.setToken("fixture-old");
    const first = client.post("overview", {}, undefined, observation);
    client.setToken("fixture-new");
    expect(await first).toBeNull();
    expect(fetch.mock.calls[0][1].signal.aborted).toBe(true);
    expect(await client.post("overview", {}, undefined, observation)).toMatchObject({ data: { owner: "new" } });
    const other = new RuntimeV2Client();
    other.setToken("fixture-new");
    await other.post("overview", {}, undefined, observation);
    expect(fetch).toHaveBeenCalledTimes(3);
    old.resolve(response({ owner: "old" }));
  });

  it("does not coalesce distinct selectors or writes", async () => {
    const pending = deferred<Response>();
    const fetch = vi.fn(() => pending.promise);
    vi.stubGlobal("fetch", fetch);
    const client = new RuntimeV2Client();
    const reads = [client.post("window", { key: "a" }, undefined, observation),
      client.post("window", { key: "b" }, undefined, observation)];
    const writes = [client.post("window-label", { key: "a", label: "name" }),
      client.post("window-label", { key: "a", label: "name" })];
    expect(fetch).toHaveBeenCalledTimes(4);
    pending.resolve(response({ ok: true }));
    await Promise.all([...reads, ...writes]);
  });


  it("recovers Window polling after a hung primary read without losing the selected Window", async () => {
    vi.useFakeTimers();
    vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
    const key = "a".repeat(64);
    const row = { client_window_key: key, source: "openai-session", last_seen_at_ms: 1_790_000_000_000,
      active_count: 0, linked_session_count: 0, recorder_gap_count: 0 };
    let primary = 0;
    const stalled = deferred<Response>();
    const recovered = deferred<Response>();
    const fetch = vi.fn((url: string, init: RequestInit) => {
      const body = JSON.parse(String(init.body));
      if (url.endsWith("/windows")) return Promise.resolve(response({
        windows: [row], total: 1, returned: 1, truncated: false, visibility: { scope: "principal" },
      }));
      if (url.endsWith("/window")) {
        if (body.detail_level === "primary") return ++primary === 1 ? stalled.promise : recovered.promise;
        return Promise.resolve(response(windowDetail({ detail_level: body.detail_level || "full" })));
      }
      throw new Error("Unexpected observation: " + url);
    });
    vi.stubGlobal("fetch", fetch);
    const client = new RuntimeV2Client();
    const unauthorized = vi.fn();
    const hook = renderHook(() => useWindowWorkspace(client, true, unauthorized, { initialWindowKey: key }));
    await act(async () => {});
    expect(hook.result.current.detailAvailability).toBe("loading");
    await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
    expect(hook.result.current.detailAvailability).toBe("error");
    await act(async () => { await vi.advanceTimersByTimeAsync(3_000); });
    expect(primary).toBe(2);
    await act(async () => { recovered.resolve(response(windowDetail({ detail_level: "primary" }))); });
    expect(hook.result.current.detailAvailability).toBe("available");
    expect(hook.result.current.selectedKey).toBe(key);
    expect(hook.result.current.detail?.client_window_key).toBe(key);
    await act(async () => { stalled.resolve(response({ error: "late old failure" }, 401)); });
    expect(unauthorized).not.toHaveBeenCalled();
    expect(hook.result.current.detailAvailability).toBe("available");
    hook.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("bounds pending observation retention and frees capacity after cancellation", async () => {
    vi.useFakeTimers();
    const fetch = vi.fn(() => new Promise<Response>(() => {}));
    vi.stubGlobal("fetch", fetch);
    const client = new RuntimeV2Client();
    const owners = Array.from({ length: 64 }, () => new AbortController());
    const pending = owners.map((owner, index) => client.post("window", { key: index }, owner.signal, observation));
    expect(await client.post("window", { key: 65 }, undefined, observation)).toMatchObject({ ok: false, status: 0 });
    expect(fetch).toHaveBeenCalledTimes(64);
    for (const owner of owners) owner.abort();
    expect(await Promise.all(pending)).toEqual(Array(64).fill(null));
    const nextOwner = new AbortController();
    const next = client.post("window", { key: 65 }, nextOwner.signal, observation);
    expect(fetch).toHaveBeenCalledTimes(65);
    nextOwner.abort();
    expect(await next).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("keeps writes outside the observation timeout boundary", async () => {
    vi.useFakeTimers();
    const pending = deferred<Response>();
    vi.stubGlobal("fetch", vi.fn(() => pending.promise));
    const client = new RuntimeV2Client();
    const done = vi.fn();
    const write = client.post("workflow-session-post-message", { message: "fixture" }).then(done);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(done).not.toHaveBeenCalled();
    pending.resolve(response({ success: true }));
    await write;
    expect(done).toHaveBeenCalledWith(expect.objectContaining({ ok: true }));
  });
});
