import { useShellText } from "../i18n/runtime-shell";
import type { RunnerCapacity } from "../lib/runner-capacity";

export function RunnerCapacitySummary({ capacity }: { capacity: RunnerCapacity }) {
  const s = useShellText();
  const labels = { offline: "Runner offline; capacity unknown.", stale: "Capacity needs refresh.", unsupported: "Job capacity is not reported by this Runner.", unavailable: "Job capacity unavailable." };
  return <p className="field-help" data-webcodex-capacity={capacity.state}>
    {capacity.state === "available"
      ? `${capacity.running} ${s("running")} · ${capacity.queued} ${s("queued")} · ${capacity.limit} ${s("max")}`
      : s(labels[capacity.state])}
  </p>;
}
