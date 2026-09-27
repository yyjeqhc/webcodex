import { useCallback, useEffect, useRef, useState } from "react";
import { fetchRuntimeOverview } from "../api/runtime.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, RuntimeOverview } from "../model/types.js";

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
): RuntimeOverviewState {
  const [availability, setAvailability] = useState<Availability>("idle");
  const [data, setData] = useState<RuntimeOverview | null>(null);
  const [updatedAt, setUpdatedAt] = useState<number | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [revision, setRevision] = useState(0);
  const current = useRef<AbortController | null>(null);

  const refresh = useCallback(() => {
    if (!current.current) setRevision((value) => value + 1);
  }, []);

  useEffect(() => {
    if (!enabled) {
      current.current?.abort();
      current.current = null;
      setData(null);
      setUpdatedAt(null);
      setRefreshing(false);
      setAvailability("idle");
      return;
    }
    let disposed = false;
    const controller = new AbortController();
    current.current?.abort();
    current.current = controller;
    setRefreshing(true);
    setAvailability((value) => (value === "idle" ? "loading" : value));

    void fetchRuntimeOverview(client, controller.signal).then((response) => {
      if (disposed || current.current !== controller || !response) return;
      current.current = null;
      setRefreshing(false);
      if (response.status === 401) {
        onUnauthorized();
        return;
      }
      if (response.status === 403) {
        setData(null);
        setAvailability("denied");
        return;
      }
      if (!response.ok || !response.data) {
        setAvailability((value) => value === "available" || value === "stale" ? "stale" : "error");
        return;
      }
      setData(response.data);
      setUpdatedAt(Date.now());
      setAvailability("available");
    });

    return () => {
      disposed = true;
      controller.abort();
      if (current.current === controller) current.current = null;
    };
  }, [client, enabled, onUnauthorized, revision]);

  useEffect(() => {
    if (!enabled) return;
    const refreshVisible = () => {
      if (document.visibilityState !== "hidden") refresh();
    };
    const timer = window.setInterval(refreshVisible, 5_000);
    window.addEventListener("focus", refreshVisible);
    document.addEventListener("visibilitychange", refreshVisible);
    window.addEventListener("online", refreshVisible);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", refreshVisible);
      document.removeEventListener("visibilitychange", refreshVisible);
      window.removeEventListener("online", refreshVisible);
    };
  }, [enabled, refresh]);

  return { availability, data, refresh, updatedAt, refreshing };
}
