import type { RuntimeOverview, RunnerSummary } from "../model/types.js";
import type { RuntimeV2Client } from "./client.js";

export function fetchRuntimeOverview(client: RuntimeV2Client, signal?: AbortSignal) {
  return client.post<RuntimeOverview>("overview", {}, signal);
}

export function fetchRunner(client: RuntimeV2Client, clientId: string, signal?: AbortSignal) {
  return client.post<RunnerSummary>("runner", { client_id: clientId }, signal);
}
