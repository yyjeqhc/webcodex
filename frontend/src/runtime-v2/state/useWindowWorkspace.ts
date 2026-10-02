import { useCallback, useEffect, useRef, useState } from "react";
import { fetchWindowDetail, fetchWindowPrimaryDetail } from "../api/windows.js";
import { useWindowInventory } from "./useWindowInventory.js";
import { useVisibleRefresh } from "./useVisibleRefresh.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, WindowActivity, WindowDetail, WindowSummary } from "../model/types.js";

const MAX_MERGED_WINDOW_ACTIVITY = 2_000;
const DEFAULT_FULL_DETAIL_REFRESH_MS = 30_000;

function windowActivityIdentity(row: WindowActivity): string {
  if (row.server_trace_id) return "trace:" + row.server_trace_id;
  return [
    row.started_at_ms,
    row.ended_at_ms,
    row.tool_name || row.method,
    row.project || "",
    row.status,
  ].join("|");
}

function mergeWindowActivity(
  newer: WindowActivity[],
  retained: WindowActivity[],
): { activity: WindowActivity[]; truncated: boolean } {
  const seen = new Set<string>();
  const activity: WindowActivity[] = [];
  for (const row of [...newer, ...retained]) {
    const identity = windowActivityIdentity(row);
    if (seen.has(identity)) continue;
    seen.add(identity);
    activity.push(row);
  }
  activity.sort((left, right) =>
    right.started_at_ms - left.started_at_ms ||
    right.ended_at_ms - left.ended_at_ms ||
    windowActivityIdentity(left).localeCompare(windowActivityIdentity(right)));
  const truncated = activity.length > MAX_MERGED_WINDOW_ACTIVITY;
  if (truncated) activity.length = MAX_MERGED_WINDOW_ACTIVITY;
  return { activity, truncated };
}

function mergePrimaryWindowDetail(
  current: WindowDetail | null,
  primary: WindowDetail,
): WindowDetail {
  if (
    !current ||
    current.client_window_key !== primary.client_window_key ||
    current.detail_level !== "full"
  ) {
    return primary;
  }
  const merged = mergeWindowActivity(primary.activity, current.activity);
  return {
    ...current,
    source: primary.source || current.source,
    last_seen_at_ms: Math.max(current.last_seen_at_ms, primary.last_seen_at_ms),
    last_tool_call_at_ms:
      Math.max(current.last_tool_call_at_ms || 0, primary.last_tool_call_at_ms || 0) || undefined,
    last_meaningful_activity_at_ms:
      Math.max(
        current.last_meaningful_activity_at_ms || 0,
        primary.last_meaningful_activity_at_ms || 0,
      ) || undefined,
    active_count: primary.active_count,
    active_requests: primary.active_requests,
    activity: merged.activity,
    activity_returned: merged.activity.length,
    activity_truncated: current.activity_truncated || merged.truncated,
    visibility: primary.visibility,
    detail_level: "full",
  };
}

function mergeFullWindowDetail(
  current: WindowDetail | null,
  full: WindowDetail,
): WindowDetail {
  if (!current || current.client_window_key !== full.client_window_key) {
    return { ...full, detail_level: "full" };
  }
  const merged = mergeWindowActivity(current.activity, full.activity);
  const currentIsNewer = current.last_seen_at_ms > full.last_seen_at_ms;
  return {
    ...full,
    source: currentIsNewer ? current.source || full.source : full.source,
    last_seen_at_ms: Math.max(current.last_seen_at_ms, full.last_seen_at_ms),
    last_tool_call_at_ms:
      Math.max(current.last_tool_call_at_ms || 0, full.last_tool_call_at_ms || 0) || undefined,
    last_meaningful_activity_at_ms:
      Math.max(
        current.last_meaningful_activity_at_ms || 0,
        full.last_meaningful_activity_at_ms || 0,
      ) || undefined,
    active_count: currentIsNewer ? current.active_count : full.active_count,
    active_requests: currentIsNewer ? current.active_requests : full.active_requests,
    activity: merged.activity,
    activity_returned: merged.activity.length,
    activity_truncated:
      full.activity_truncated ||
      (current.detail_level === "full" && current.activity_truncated) ||
      merged.truncated,
    detail_level: "full",
  };
}

export type WindowWorkspaceState = {
  availability: Availability;
  detailAvailability: Availability;
  detailHydrating: boolean;
  windows: WindowSummary[];
  historicalWindows: WindowSummary[];
  loadMore: () => void;
  loadingMore: boolean;
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
  options: {
    refreshMs?: number;
    backgroundRefreshMs?: number;
    fullDetailRefreshMs?: number;
    loadDetail?: boolean;
    initialWindowKey?: string;
    projects?: string[];
  } = {},
): WindowWorkspaceState {
  const refreshMs = options.refreshMs ?? 3_000;
  const inventory = useWindowInventory(client, enabled, onUnauthorized, { refreshMs, projects: options.projects });
  const { availability, windows, total, truncated, scope, removeWindow } = inventory;
  const fullDetailRefreshMs = options.fullDetailRefreshMs ?? DEFAULT_FULL_DETAIL_REFRESH_MS;
  const loadDetail = options.loadDetail ?? true;
  const [detailAvailability, setDetailAvailability] = useState<Availability>("idle");
  const [detailHydrating, setDetailHydrating] = useState(false);
  const [selectedKey, setSelectedKey] = useState(options.initialWindowKey || "");
  const select = useCallback((key: string) => {
    setSelectedKey(key);
  }, []);
  const [detail, setDetail] = useState<WindowDetail | null>(null);
  const [detailRevision, setDetailRevision] = useState(0);
  const detailRequest = useRef<AbortController | null>(null);
  const fullDetailRequest = useRef<{ key: string; controller: AbortController } | null>(null);
  const fullDetailLoadedKey = useRef("");
  const fullDetailLoadedAt = useRef(0);
  const lastPrimaryActivity = useRef<string | null>(null);
  const fullDetailNeedsCatchup = useRef(false);

  const refresh = useCallback(() => {
    inventory.refresh();
    if (!detailRequest.current) setDetailRevision(value => value + 1);
  }, [inventory.refresh]);

  useEffect(() => {
    setSelectedKey(current => current || String(windows[0]?.client_window_key || ""));
  }, [windows]);

  useEffect(() => {
    if (availability === "denied") {
      detailRequest.current?.abort(); detailRequest.current = null;
      fullDetailRequest.current?.controller.abort(); fullDetailRequest.current = null;
      setDetailHydrating(false);
      setSelectedKey(""); setDetail(null); setDetailAvailability("denied");
    }
  }, [availability]);

  const hydrateFullDetail = useCallback((key: string) => {
    if (!enabled || !loadDetail || !key || fullDetailRequest.current) return;
    const controller = new AbortController();
    fullDetailRequest.current = { key, controller };
    fullDetailNeedsCatchup.current = false;
    setDetailHydrating(true);
    void fetchWindowDetail(client, key, controller.signal).then((response) => {
      if (
        fullDetailRequest.current?.controller !== controller ||
        controller.signal.aborted ||
        !response
      ) return;
      fullDetailRequest.current = null;
      setDetailHydrating(false);
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
        removeWindow(key);
        return;
      }
      if (!response.ok || !response.data || response.data.client_window_key !== key) {
        // Primary data remains usable. The next primary refresh can retry history.
        fullDetailNeedsCatchup.current = true;
        return;
      }
      fullDetailLoadedKey.current = key;
      fullDetailLoadedAt.current = Date.now();
      setDetail((current) => mergeFullWindowDetail(current, response.data!));
      setDetailAvailability("available");
    });
  }, [client, enabled, loadDetail, onUnauthorized, removeWindow]);

  useEffect(() => {
    fullDetailRequest.current?.controller.abort();
    fullDetailRequest.current = null;
    fullDetailLoadedKey.current = "";
    fullDetailLoadedAt.current = 0;
    lastPrimaryActivity.current = null;
    fullDetailNeedsCatchup.current = false;
    setDetailHydrating(false);
  }, [client, enabled, loadDetail, selectedKey]);

  useEffect(() => {
    detailRequest.current?.abort();
    detailRequest.current = null;
    if (!enabled || !loadDetail || !selectedKey) {
      setDetail(null);
      setDetailAvailability("idle");
      return;
    }
    const controller = new AbortController();
    detailRequest.current = controller;
    setDetailAvailability((value) => (value === "idle" ? "loading" : value));
    void fetchWindowPrimaryDetail(client, selectedKey, controller.signal).then((response) => {
      if (detailRequest.current !== controller || controller.signal.aborted || !response) return;
      detailRequest.current = null;
      if (response.status === 401) {
        onUnauthorized();
        return;
      }
      if (response.status === 403) {
        fullDetailRequest.current?.controller.abort();
        fullDetailRequest.current = null;
        setDetailHydrating(false);
        setDetail(null);
        setDetailAvailability("denied");
        return;
      }
      if (response.status === 404) {
        fullDetailRequest.current?.controller.abort();
        fullDetailRequest.current = null;
        setDetailHydrating(false);
        setDetail(null);
        setDetailAvailability("denied");
        removeWindow(selectedKey);
        return;
      }
      if (!response.ok || !response.data || response.data.client_window_key !== selectedKey) {
        setDetailAvailability((current) =>
          current === "available" || current === "stale" ? "stale" : "error");
        return;
      }
      const recent = response.data.activity;
      if (lastPrimaryActivity.current !== null && response.data.activity_truncated &&
          !recent.some((row) => windowActivityIdentity(row) === lastPrimaryActivity.current)) {
        fullDetailNeedsCatchup.current = true;
      }
      lastPrimaryActivity.current = recent[0] ? windowActivityIdentity(recent[0]) : "";
      setDetail((current) => mergePrimaryWindowDetail(current, response.data!));
      setDetailAvailability("available");
      const fullDetailDue =
        fullDetailNeedsCatchup.current || fullDetailLoadedKey.current !== selectedKey ||
        Date.now() - fullDetailLoadedAt.current >= fullDetailRefreshMs;
      if (fullDetailDue) hydrateFullDetail(selectedKey);
    });
    return () => {
      controller.abort();
      if (detailRequest.current === controller) detailRequest.current = null;
    };
  }, [
    client,
    detailRevision,
    enabled,
    fullDetailRefreshMs,
    hydrateFullDetail,
    loadDetail,
    onUnauthorized,
    removeWindow,
    selectedKey,
  ]);

  const refreshDetail = useCallback(() => {
    if (!detailRequest.current) setDetailRevision(value => value + 1);
  }, []);
  const pauseDetail = useCallback(() => {
    detailRequest.current?.abort(); detailRequest.current = null;
    fullDetailRequest.current?.controller.abort(); fullDetailRequest.current = null;
    setDetailHydrating(false);
  }, []);
  useVisibleRefresh(enabled && loadDetail, refreshDetail, refreshMs, pauseDetail);
  useEffect(() => () => {
    fullDetailRequest.current?.controller.abort(); fullDetailRequest.current = null;
  }, []);

  return {
    availability,
    detailAvailability,
    detailHydrating,
    windows,
    historicalWindows: inventory.historicalWindows,
    loadMore: inventory.loadMore,
    loadingMore: inventory.loadingMore,
    total,
    truncated,
    scope,
    selectedKey,
    detail: enabled && loadDetail && detail?.client_window_key === selectedKey ? detail : null,
    select,
    refresh,
  };
}
