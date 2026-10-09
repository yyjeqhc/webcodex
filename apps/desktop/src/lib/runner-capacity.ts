import type { ServerRunnerSummary } from "../models/workspace";

export type RunnerCapacity = { state: "available"; running: number; queued: number; limit: number }
  | { state: "offline" | "stale" | "unsupported" | "unavailable" };

export function runnerCapacity(runner: ServerRunnerSummary | null | undefined, stale = false): RunnerCapacity {
  if (!runner) return { state: "unavailable" };
  if (stale || runner.status === "stale") return { state: "stale" };
  if (!runner.connected) return { state: "offline" };
  const { jobs_running: running, jobs_queued: queued, job_concurrency_limit: limit } = runner;
  if (![running, queued, limit].every(value => typeof value === "number" && Number.isSafeInteger(value) && value >= 0)
    || limit! < 1 || limit! > 64) return { state: "unsupported" };
  // Preserve reported counts; a stop request alone does not prove termination.
  return { state: "available", running: running!, queued: queued!, limit: limit! };
}

export function parseJobConcurrency(value: string): number | null {
  if (!/^\d+$/.test(value)) return null;
  const limit = Number(value);
  return Number.isInteger(limit) && limit >= 1 && limit <= 64 ? limit : null;
}
