import { useEffect, useState } from "react";
import { Button } from "@mantine/core";
import { workspaceQuery } from "../workspace/WorkspaceContext";
import type { DesktopState } from "../../models/topology";
import { useProduct } from "../../i18n/product";

interface BrowserStatus {
  client_id: string;
  state: "fresh" | "partial" | "unavailable";
  count: number | null;
  owned_count?: number;
  attached_count?: number;
}

interface ObservedBrowserStatus {
  revision: number;
  status: BrowserStatus;
}

/** On-demand only. An unavailable Runner is never silently interpreted as zero. */
export function BrowserRuntimeStatus({ state }: { state: DesktopState }) {
  const p = useProduct();
  const clientId = state.workspace_runner?.client_id;
  const ready = state.readiness.runtime_ready && !state.current_operation;
  const [observation, setObservation] = useState<ObservedBrowserStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    setObservation(null);
    if (!ready || !clientId) return;
    let cancelled = false;
    setLoading(true);
    workspaceQuery<BrowserStatus>({ kind: "browser_status" })
      .then(value => {
        if (!cancelled) setObservation({ revision, status: value?.client_id === clientId && value.state === "fresh"
          && Number.isInteger(value.count) && value.count! >= 0 && value.count! <= 4
          && Number.isInteger(value.owned_count) && value.owned_count! >= 0
          && Number.isInteger(value.attached_count) && value.attached_count! >= 0
          && value.owned_count! + value.attached_count! === value.count
          ? value : { client_id: clientId, state: "unavailable", count: null } });
      })
      .catch(() => {
        if (!cancelled) setObservation({ revision, status: { client_id: clientId, state: "unavailable", count: null } });
      })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [clientId, ready, revision]);

  if (!clientId) return null;
  // Scope successful data to the current Runner and request revision in render,
  // not only in the effect: a selection change must never paint the old count.
  const observed = observation?.revision === revision && observation.status.client_id === clientId
    ? observation.status : null;
  const available = ready && observed?.state === "fresh";
  return <section className="dashboard-handoff" aria-label={p("browserRuntimeTitle")}>
    <div>
      <h2>{p("browserRuntimeTitle")}</h2>
      <p role="status">
        {!ready ? p("browserRuntimeStopped")
          : loading ? p("browserRuntimeChecking")
          : available ? <>
              {p("browserRuntimeActive").replace("{count}", String(observed.count))}
              {" · "}{p("browserRuntimeOwnership").replace("{owned}", String(observed.owned_count)).replace("{attached}", String(observed.attached_count))}
            </>
          : p("browserRuntimeUnknown")}
      </p>
    </div>
    <Button className="secondary-button" variant="default" disabled={!ready || loading}
      onClick={() => setRevision(value => value + 1)}>{p("refresh")}</Button>
  </section>;
}