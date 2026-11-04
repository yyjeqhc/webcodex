import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { useProjects } from "../src/runtime-v2/state/useProjects.js";
import { useRuntimeOverview } from "../src/runtime-v2/state/useRuntimeOverview.js";
import { runtimeOverview } from "./fixtures.js";

// This is a controlled request-load comparison in one jsdom document, not a
// production latency test or a claim that independent browser tabs share reads.
// The same file can run in the baseline checkout with INVENTORY_BASELINE=1.
const baseline = process.env.INVENTORY_BASELINE === "1";
const scenarios = [1, 5, 10].flatMap(consumers =>
  (["shared-client", "independent-clients"] as const).flatMap(scope =>
    (["healthy", "slow-503"] as const).map(profile => ({ consumers, scope, profile }))));
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.useRealTimers(); vi.unstubAllGlobals(); });

it.each(scenarios)("$consumers hook trees / $scope / $profile stay within a reproducible request budget",
  async ({ consumers, scope, profile }) => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    let visibility: DocumentVisibilityState = "visible";
    vi.spyOn(document, "visibilityState", "get").mockImplementation(() => visibility);
    const fixture = runtimeOverview();
    const latencyMs = profile === "healthy" ? 100 : 7_000;
    const durationMs = profile === "healthy" ? 25_000 : 55_000;
    const requests: Array<{ path: string; at: number }> = [];
    let pending = 0;
    let peakPending = 0;
    vi.stubGlobal("fetch", vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input).split("/").at(-1)!;
      requests.push({ path, at: Date.now() });
      pending++; peakPending = Math.max(peakPending, pending);
      return new Promise<Response>((resolve, reject) => {
        const finish = () => {
          pending--; init?.signal?.removeEventListener("abort", abort);
          const data = path === "overview"
            ? { ...fixture, detail_level: "primary", projects_included: false, projects: [] }
            : { projects: fixture.projects, total: fixture.projects.length, truncated: false };
          resolve(new Response(JSON.stringify(profile === "healthy" ? data : { error: "fixture overload" }),
            { status: profile === "healthy" ? 200 : 503, headers: { "Content-Type": "application/json" } }));
        };
        const abort = () => { clearTimeout(timer); pending--; reject(new DOMException("Aborted", "AbortError")); };
        const timer = setTimeout(finish, latencyMs);
        init?.signal?.addEventListener("abort", abort, { once: true });
      });
    }));
    const clients = Array.from({ length: scope === "shared-client" ? 1 : consumers }, () => {
      const client = new RuntimeV2Client(); client.setToken("inventory-fixture"); return client;
    });
    const unauthorized = vi.fn();
    const firstProjectReady: Array<number | null> = Array(consumers).fill(null);
    const hooks = Array.from({ length: consumers }, (_, index) => renderHook(() => {
      const client = clients[scope === "shared-client" ? 0 : index];
      const overview = useRuntimeOverview(client, true, unauthorized, false);
      const projects = useProjects(client, true, unauthorized);
      return { overview, projects };
    }));
    const advance = async (ms: number) => {
      await act(async () => { await vi.advanceTimersByTimeAsync(ms); });
      hooks.forEach((hook, index) => {
        if (firstProjectReady[index] === null && hook.result.current.projects.availability === "available") {
          firstProjectReady[index] = Date.now();
        }
      });
    };
    await advance(100);
    await advance(900);
    for (let elapsed = 1_000; elapsed < durationMs; elapsed += 1_000) await advance(1_000);
    const overviewReads = requests.filter(request => request.path === "overview").length;
    const projectReads = requests.filter(request => request.path === "projects").length;
    const multiplicity = baseline || scope === "independent-clients" ? consumers : 1;
    const expectedOverview = baseline ? 6 : profile === "healthy" ? 3 : 2;
    const expectedProjects = baseline ? profile === "healthy" ? 3 : 6 : 1;
    expect(overviewReads).toBe(expectedOverview * multiplicity);
    expect(projectReads).toBe(expectedProjects * multiplicity);
    expect(peakPending).toBe(2 * multiplicity);
    expect(firstProjectReady).toEqual(Array(consumers).fill(profile === "healthy" ? 100 : null));
    expect(unauthorized).not.toHaveBeenCalled();
    const beforeHidden = requests.length;
    await act(async () => { visibility = "hidden"; document.dispatchEvent(new Event("visibilitychange")); });
    await advance(60_000);
    expect(requests).toHaveLength(beforeHidden);
    console.info("INVENTORY_LOAD " + JSON.stringify({
      implementation: baseline ? "7e308d9ed" : "patch", consumers, scope, profile, duration_ms: durationMs,
      overview_requests: overviewReads, project_requests: projectReads, total_requests: beforeHidden,
      peak_pending: peakPending, first_project_ready_ms: firstProjectReady[0], hidden_60000ms: 0,
    }));
    hooks.forEach(hook => hook.unmount());
  });
