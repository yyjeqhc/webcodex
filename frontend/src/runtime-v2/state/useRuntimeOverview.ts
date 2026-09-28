import { useCallback, useEffect, useRef, useState } from "react";
import { fetchRuntimeOverview } from "../api/runtime.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, RuntimeOverview } from "../model/types.js";
import { useVisibleRefresh } from "./useVisibleRefresh.js";

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
  const [revision, setRevision] = useState(0);
  const current = useRef<AbortController | null>(null);
  const observed = useRef<RuntimeOverview | null>(null);
  const owner = useRef(client);
  const refresh = useCallback(() => {
    if (!current.current) setRevision(value => value + 1);
  }, []);

  useEffect(() => {
    if (owner.current !== client) {
      owner.current = client; observed.current = null;
      setData(null); setUpdatedAt(null); setAvailability("idle");
    }
    if (!enabled) {
      current.current?.abort(); current.current = null; observed.current = null;
      setData(null); setUpdatedAt(null); setRefreshing(false); setAvailability("idle");
      return;
    }
    const controller = new AbortController();
    current.current?.abort(); current.current = controller;
    let hydration: ReturnType<typeof setTimeout> | undefined;
    setRefreshing(true);
    setAvailability(value => value === "idle" ? "loading" : value);
    const failed = () => setAvailability(value => value === "available" || value === "stale" ? "stale" : "error");
    // Render registry facts first. Only the Session/Runtime surfaces need the
    // more expensive fleet-wide retained-Session aggregation, never Window work.
    const full = includeSessions && observed.current !== null;
    void fetchRuntimeOverview(client, controller.signal, full).then(response => {
      if (controller.signal.aborted || current.current !== controller) return;
      current.current = null; setRefreshing(false);
      if (!response) { failed(); return; }
      if (response.status === 401) { onUnauthorized(); return; }
      if (response.status === 403) {
        observed.current = null; setData(null); setAvailability("denied"); return;
      }
      if (!response.ok || !response.data) { failed(); return; }
      observed.current = response.data;
      setData(response.data); setUpdatedAt(Date.now()); setAvailability("available");
      if (includeSessions && response.data.detail_level === "primary") hydration = setTimeout(refresh, 0);
    }).catch(() => {
      if (controller.signal.aborted || current.current !== controller) return;
      current.current = null; setRefreshing(false); failed();
    });
    return () => {
      controller.abort();
      if (hydration !== undefined) clearTimeout(hydration);
      if (current.current === controller) current.current = null;
    };
  }, [client, enabled, onUnauthorized, revision, includeSessions, refresh]);

  useVisibleRefresh(enabled, refresh, includeSessions ? 30_000 : 10_000);
  return { availability, data, refresh, updatedAt, refreshing };
}
