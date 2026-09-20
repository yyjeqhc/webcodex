import { useCallback, useEffect, useRef, useState } from "react";
import { fetchRuntimeOverview } from "../api/runtime.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, RuntimeOverview } from "../model/types.js";

export type RuntimeOverviewState = {
  availability: Availability;
  data: RuntimeOverview | null;
  refresh: () => void;
};

export function useRuntimeOverview(
  client: RuntimeV2Client,
  enabled: boolean,
  onUnauthorized: () => void,
): RuntimeOverviewState {
  const [availability, setAvailability] = useState<Availability>("idle");
  const [data, setData] = useState<RuntimeOverview | null>(null);
  const [revision, setRevision] = useState(0);
  const current = useRef<AbortController | null>(null);

  const refresh = useCallback(() => setRevision((value) => value + 1), []);

  useEffect(() => {
    if (!enabled) {
      current.current?.abort();
      current.current = null;
      setData(null);
      setAvailability("idle");
      return;
    }
    let disposed = false;
    const controller = new AbortController();
    current.current?.abort();
    current.current = controller;
    setAvailability((value) => (value === "idle" ? "loading" : value));

    void fetchRuntimeOverview(client, controller.signal).then((response) => {
      if (disposed || current.current !== controller || !response) return;
      current.current = null;
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
      setAvailability("available");
    });

    return () => {
      disposed = true;
      controller.abort();
    };
  }, [client, enabled, onUnauthorized, revision]);

  useEffect(() => {
    if (!enabled) return;
    const timer = window.setInterval(refresh, 30_000);
    return () => window.clearInterval(timer);
  }, [enabled, refresh]);

  return { availability, data, refresh };
}
