import { describe, expect, it } from "vitest";
import { runnerCapacity, parseJobConcurrency } from "./runner-capacity";
const online = { client_id: "local", connected: true, status: "online", jobs_running: 4, jobs_queued: 2, job_concurrency_limit: 4 };
describe("durable Runner Job capacity", () => {
  it("preserves the Server-reported running and queued counts", () => {
    expect(runnerCapacity(online)).toEqual({ state: "available", running: 4, queued: 2, limit: 4 });
  });
  it("keeps offline, stale and unsupported distinct from idle", () => {
    expect(runnerCapacity({ ...online, connected: false }).state).toBe("offline");
    expect(runnerCapacity({ ...online, status: "stale" }).state).toBe("stale");
    expect(runnerCapacity(online, true).state).toBe("stale");
    expect(runnerCapacity({ ...online, job_concurrency_limit: undefined }).state).toBe("unsupported");
    expect(runnerCapacity(null).state).toBe("unavailable");
    expect(runnerCapacity({ ...online, jobs_running: 0, jobs_queued: 0 }).state).toBe("available");
  });
  it("does not manufacture zero from malformed or missing telemetry", () => {
    for (const value of [undefined, null, -1, 1.5, NaN]) {
      expect(runnerCapacity({ ...online, jobs_running: value as number }).state).toBe("unsupported");
    }
  });
  it("accepts only whole-number limits in the existing 1–64 range", () => {
    for (const value of ["1", "4", "12", "64"]) expect(parseJobConcurrency(value)).toBe(Number(value));
    for (const value of ["", "0", "65", "1.5", "1e1", "-1", "Infinity"]) expect(parseJobConcurrency(value)).toBeNull();
  });
});
