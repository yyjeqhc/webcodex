import { afterEach, describe, expect, it, vi } from "vitest";
import { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { registerProject } from "../src/runtime-v2/api/projects.js";

const observation = { observation: true } as const;
const selector = { client_window_key: "fixture-window", limit: 50 };
const response = (snapshot: string, status = 200) => new Response(JSON.stringify({ snapshot }), {
  status, headers: { "Content-Type": "application/json" },
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}
afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

describe("observation freshness across mutations", () => {
  it("does not reuse a pre-write read retained by another subscriber after a successful write", async () => {
    const old = deferred<Response>();
    let reads = 0;
    const fetch = vi.fn(async (input: RequestInfo | URL) => {
      if (String(input).endsWith("/window-collaboration")) {
        return ++reads === 1 ? old.promise : response("after-write");
      }
      return response("write-complete");
    });
    vi.stubGlobal("fetch", fetch);
    const client = new RuntimeV2Client();
    const owner = new AbortController();
    const before = client.post("window-collaboration", selector, owner.signal, observation);
    const retainedByOther = client.post("window-collaboration", selector, undefined, observation);
    await client.post("window-collaboration-post", { message: "new message" });
    owner.abort();
    const after = client.post("window-collaboration", selector, undefined, observation);
    const dispatched = reads;
    // Release the old transport even on the unfixed implementation, so an
    // assertion failure cannot leave a thirty-second timer or pending Promise.
    old.resolve(response("before-write"));
    const [first, retained, refreshed] = await Promise.all([before, retainedByOther, after]);
    expect(first).toBeNull();
    expect(retained).toMatchObject({ data: { snapshot: "before-write" } });
    expect(dispatched).toBe(2);
    expect(refreshed).toMatchObject({ data: { snapshot: "after-write" } });
  });

  it.each(["post", "project-registration"] as const)("separates pre-write, during-write and post-write reads for %s", async mutationPath => {
    const old = deferred<Response>();
    const during = deferred<Response>();
    const write = deferred<Response>();
    let reads = 0;
    const fetch = vi.fn(async (input: RequestInfo | URL) => {
      if (String(input).endsWith("/projects")) {
        reads++;
        return reads === 1 ? old.promise : reads === 2 ? during.promise : response("after-write");
      }
      return write.promise;
    });
    vi.stubGlobal("fetch", fetch);
    const client = new RuntimeV2Client();
    const read = () => client.post("projects", { client_id: "fixture-runner" }, undefined, observation);
    const before = read();
    const mutation = mutationPath === "post"
      ? client.post("window-label", { label: "new name" })
      : registerProject(client, "fixture-runner", "/fixture/project");
    const inFlight = read();
    const duringDispatches = reads;
    write.resolve(response("write-complete"));
    await mutation;
    const after = read();
    const afterDispatches = reads;
    old.resolve(response("before-write"));
    during.resolve(response("during-write"));
    const snapshots = await Promise.all([before, inFlight, after]);
    expect(duringDispatches).toBe(2);
    expect(afterDispatches).toBe(3);
    expect(snapshots).toEqual([
      expect.objectContaining({ data: { snapshot: "before-write" } }),
      expect.objectContaining({ data: { snapshot: "during-write" } }),
      expect.objectContaining({ data: { snapshot: "after-write" } }),
    ]);
    if (mutationPath === "project-registration") {
      expect(fetch.mock.calls.some(([input]) => String(input) === "/api/projects/resolve-or-register")).toBe(true);
    }
  });

  it.each([503, 0])("keeps an explicit reconciliation read fresh after mutation failure %s", async status => {
    const old = deferred<Response>();
    let reads = 0;
    vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => {
      if (String(input).endsWith("/window-collaboration")) {
        return ++reads === 1 ? old.promise : response("reconciled");
      }
      if (status === 0) throw new TypeError("fixture transport failure");
      return response("uncertain-write", status);
    }));
    const client = new RuntimeV2Client();
    const before = client.post("window-collaboration", selector, undefined, observation);
    const write = await client.post("window-collaboration-post", { message: "new message" });
    const after = client.post("window-collaboration", selector, undefined, observation);
    const dispatched = reads;
    old.resolve(response("before-write"));
    await before;
    const refreshed = await after;
    expect(write).toMatchObject({ ok: false, status });
    expect(dispatched).toBe(2);
    expect(refreshed).toMatchObject({ data: { snapshot: "reconciled" } });
  });
});
