import { useCallback, useEffect, useRef, useState } from "react";
import { fetchWindowDetail, fetchWindows } from "../api/windows.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, WindowDetail, WindowSummary } from "../model/types.js";

export type WindowWorkspaceState = {
  availability: Availability;
  detailAvailability: Availability;
  windows: WindowSummary[];
  total: number;
  truncated: boolean;
  scope: "global" | "principal";
  selectedKey: string;
  detail: WindowDetail | null;
  select: (key: string) => void;
  refresh: () => void;
};

export function useWindowWorkspace(
  client: RuntimeV2Client,
  enabled: boolean,
  onUnauthorized: () => void,
  options: { refreshMs?: number; loadDetail?: boolean } = {},
): WindowWorkspaceState {
  const refreshMs = options.refreshMs ?? 3_000;
  const loadDetail = options.loadDetail ?? true;
  const [availability, setAvailability] = useState<Availability>("idle");
  const [detailAvailability, setDetailAvailability] = useState<Availability>("idle");
  const [windows, setWindows] = useState<WindowSummary[]>([]);
  const [total, setTotal] = useState(0);
  const [truncated, setTruncated] = useState(false);
  const [scope, setScope] = useState<"global" | "principal">("principal");
  const [selectedKey, setSelectedKey] = useState("");
  const [detail, setDetail] = useState<WindowDetail | null>(null);
  const [revision, setRevision] = useState(0);
  const listRequest = useRef<AbortController | null>(null);
  const detailRequest = useRef<AbortController | null>(null);
  const refresh = useCallback(() => setRevision((value) => value + 1), []);

  useEffect(() => {
    listRequest.current?.abort();
    if (!enabled) {
      setAvailability("idle");
      return;
    }
    const controller = new AbortController();
    listRequest.current = controller;
    setAvailability((value) => (value === "idle" ? "loading" : value));
    void fetchWindows(client, undefined, controller.signal).then((response) => {
      if (listRequest.current !== controller || !response) return;
      listRequest.current = null;
      if (response.status === 401) {
        onUnauthorized();
        return;
      }
      if (response.status === 403) {
        setWindows([]);
        setTotal(0);
        setTruncated(false);
        setDetail(null);
        setSelectedKey("");
        setAvailability("denied");
        setDetailAvailability("denied");
        return;
      }
      if (!response.ok || !response.data) {
        setAvailability((current) => current === "available" || current === "stale" ? "stale" : "error");
        return;
      }
      const rows = response.data.windows || [];
      setWindows(rows);
      setTotal(Math.max(response.data.total || 0, rows.length));
      setTruncated(Boolean(response.data.truncated));
      setScope(response.data.visibility?.scope === "global" ? "global" : "principal");
      setAvailability("available");
      setSelectedKey((current) => rows.some((row) => row.client_window_key === current)
        ? current
        : String(rows[0]?.client_window_key || ""));
    });
    return () => controller.abort();
  }, [client, enabled, onUnauthorized, revision]);

  useEffect(() => {
    detailRequest.current?.abort();
    if (!enabled || !loadDetail || !selectedKey) {
      setDetail(null);
      setDetailAvailability(selectedKey ? "idle" : "idle");
      return;
    }
    const controller = new AbortController();
    detailRequest.current = controller;
    setDetailAvailability((value) => (value === "idle" ? "loading" : value));
    void fetchWindowDetail(client, selectedKey, controller.signal).then((response) => {
      if (detailRequest.current !== controller || !response) return;
      detailRequest.current = null;
      if (response.status === 401) {
        onUnauthorized();
        return;
      }
      if (response.status === 403) {
        setDetail(null);
        setDetailAvailability("denied");
        return;
      }
      if (response.status === 404) {
        setDetail(null);
        setDetailAvailability("denied");
        setWindows((current) => current.filter((row) => row.client_window_key !== selectedKey));
        setSelectedKey("");
        return;
      }
      if (!response.ok || !response.data || response.data.client_window_key !== selectedKey) {
        setDetailAvailability((current) => current === "available" || current === "stale" ? "stale" : "error");
        return;
      }
      setDetail(response.data);
      setDetailAvailability("available");
    });
    return () => controller.abort();
  }, [client, enabled, loadDetail, onUnauthorized, revision, selectedKey]);

  useEffect(() => {
    if (!enabled) return;
    const timer = window.setInterval(refresh, refreshMs);
    return () => window.clearInterval(timer);
  }, [enabled, refresh, refreshMs]);

  return {
    availability,
    detailAvailability,
    windows,
    total,
    truncated,
    scope,
    selectedKey,
    detail,
    select: setSelectedKey,
    refresh,
  };
}
