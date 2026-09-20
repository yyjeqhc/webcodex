import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { useRuntimeOverview } from "../src/runtime-v2/state/useRuntimeOverview.js";
import { runtimeOverview } from "./fixtures.js";

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

  it("routes unauthorized responses to the credential boundary", async () => {
    const unauthorized = vi.fn();
    const client = clientWith([{ ok: false, status: 401, data: null }]);
    renderHook(() => useRuntimeOverview(client, true, unauthorized));
    await waitFor(() => expect(unauthorized).toHaveBeenCalledTimes(1));
  });
});
