import type { RuntimeOverview } from "./types.js";

/** Registry facts that can change Project membership/availability, not heartbeat age. */
export function projectInventoryKey(overview: RuntimeOverview | null): string | undefined {
  if (!overview) return undefined;
  const runners = overview.runners.map(runner => [
    runner.client_id, runner.connected, runner.status,
    runner.project_inventory_generation, runner.project_inventory_sync_state, runner.project_inventory_updated_at,
  ]).sort((left, right) => String(left[0]).localeCompare(String(right[0])));
  return JSON.stringify([overview.visible_projects, runners]);
}
