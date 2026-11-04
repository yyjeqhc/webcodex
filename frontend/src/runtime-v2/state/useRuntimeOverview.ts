import { useEffect, useRef, useState } from "react";
import { fetchRuntimeOverview } from "../api/runtime.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, RuntimeOverview } from "../model/types.js";
import { useInventoryRefresh } from "./useInventoryRefresh.js";

export type RuntimeOverviewState = {
  availability: Availability;
  data: RuntimeOverview | null;
  refresh: () => void;
  updatedAt: number | null;
  refreshing: boolean;
};

export function useRuntimeOverview(
  client: RuntimeV2Client,
  enabled: boolean,
  onUnauthorized: () => void,
  includeSessions = true,
): RuntimeOverviewState {
  const [availability, setAvailability] = useState<Availability>("idle");
  const [data, setData] = useState<RuntimeOverview | null>(null);
  const [updatedAt, setUpdatedAt] = useState<number | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const authorityRevision = client.authorityRevision ?? 0;
  const observed = useRef<RuntimeOverview | null>(null);
  const owner = useRef({ client, authorityRevision });
  const { request, revision, refresh, reset, failed: deferRetry, stopAutomatic } = useInventoryRefresh(
    enabled, includeSessions ? 30_000 : 10_000, availability !== "denied",
  );

  useEffect(() => {
    request.cancel();
    if (owner.current.client !== client || owner.current.authorityRevision !== authorityRevision) {
      owner.current = { client, authorityRevision }; observed.current = null;
      reset(); setData(null); setUpdatedAt(null); setAvailability("idle");
    }
    if (!enabled) {
      observed.current = null; reset();
      setData(null); setUpdatedAt(null); setRefreshing(false); setAvailability("idle");
      return;
    }
    setRefreshing(true);
    setAvailability(value => value === "idle" ? "loading" : value);
    const failed = () => {
      deferRetry();
      setAvailability(value => value === "available" || value === "stale" ? "stale" : "error");
    };
    // Render registry facts first. Only the Session/Runtime surfaces need the
    // more expensive fleet-wide retained-Session aggregation, never Window work.
    const full = includeSessions && observed.current !== null;
    void request.run(signal => fetchRuntimeOverview(client, signal, full), response => {
      setRefreshing(false);
      if (!response) { failed(); return; }
      if (response.status === 401) { stopAutomatic(); if ((client.authorityRevision ?? 0) === authorityRevision) onUnauthorized(); return; }
      if (response.status === 403) {
        stopAutomatic();
        observed.current = null; setData(null); setAvailability("denied"); return;
      }
      if (!response.ok || !response.data) { failed(); return; }
      reset(); observed.current = response.data;
      setData(response.data); setUpdatedAt(Date.now()); setAvailability("available");
      // Upgrade once after a primary request. A server that still returns a
      // partial result to a full request must not cause a zero-delay retry loop.
      if (!full && includeSessions && response.data.detail_level === "primary") refresh();
    });
    return () => request.cancel();
  }, [client, authorityRevision, enabled, onUnauthorized, revision, includeSessions, request, refresh, reset, deferRetry, stopAutomatic]);

  // Effects retire the old request, but may not expose its facts for even the
  // first render of a replacement authority.
  const visible = enabled && owner.current.client === client && owner.current.authorityRevision === authorityRevision;
  return {
    availability: visible ? availability : enabled ? "loading" : "idle",
    data: visible ? data : null, refresh, updatedAt: visible ? updatedAt : null,
    refreshing: enabled && (visible ? refreshing : true),
  };
}
