import { afterEach, describe, expect, it, vi } from "vitest";
import { RuntimeV2Client } from "../src/runtime-v2/api/client.js";

const response = (status: number, data: unknown) =>
  new Response(JSON.stringify(data), { status, headers: { "Content-Type": "application/json" } });

afterEach(() => { vi.unstubAllGlobals(); });

describe("RuntimeV2Client credential authority fence", () => {
  it.each(["post", "postAt"] as const)("discards a delayed 401 from the previous credential through %s", async method => {
    let settleOld!: (result: Response) => void;
    const old = new Promise<Response>(resolve => { settleOld = resolve; });
    const fetch = vi.fn().mockImplementationOnce(() => old)
      .mockResolvedValueOnce(response(200, { owner: "new" }));
    vi.stubGlobal("fetch", fetch);
    const client = new RuntimeV2Client();
    client.setToken("test-credential-old");
    const call = (key: string) => method === "post"
      ? client.post<{owner:string}>(key, {})
      : client.postAt<{owner:string}>("/api/runtime-console/", key, {});
    const first = call("overview");
    client.setToken("test-credential-new");
    const next = await call("overview");
    expect(next).toMatchObject({ status: 200, data: { owner: "new" } });
    settleOld(response(401, { error: "unauthorized" }));
    expect(await first).toBeNull();
    expect(fetch.mock.calls.map(([,init]) => init.headers.Authorization)).toEqual([
      "Bearer test-credential-old", "Bearer test-credential-new"
    ]);
  });

  it.each(["post", "postAt"] as const)("does not expose stale successful payloads after credential change through %s", async method => {
    let settleOld!: (result: Response) => void;
    const old = new Promise<Response>(resolve => { settleOld = resolve; });
    vi.stubGlobal("fetch", vi.fn(() => old));
    const client = new RuntimeV2Client();
    client.setToken("test-credential-old");
    const first = method === "post" ? client.post("projects", {}) : client.postAt("/api/runtime-console/", "projects", {});
    client.clearToken();
    settleOld(response(200, { sensitive: "previous authority" }));
    expect(await first).toBeNull();
  });

  it("does not invalidate legitimate requests when a token is assigned unchanged", async () => {
    let release!: (result: Response) => void;
    const pending = new Promise<Response>(resolve => { release = resolve; });
    vi.stubGlobal("fetch", vi.fn(() => pending));
    const client = new RuntimeV2Client();
    client.setToken("test-credential");
    const request = client.post("overview", {});
    const revision = client.authorityRevision;
    client.setToken("test-credential");
    expect(client.authorityRevision).toBe(revision);
    release(response(200, { valid: true }));
    expect(await request).toMatchObject({ status: 200, data: { valid: true } });
  });
});
