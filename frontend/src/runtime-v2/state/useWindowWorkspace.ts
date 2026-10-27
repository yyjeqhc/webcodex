import { useCallback, useEffect, useRef, useState } from "react";
import { fetchWindowDetail, fetchWindowPrimaryDetail, setWindowLabel } from "../api/windows.js";
import { useWindowInventory } from "./useWindowInventory.js";
import { useVisibleRefresh } from "./useVisibleRefresh.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, WindowDetail, WindowSummary } from "../model/types.js";
import { mergeWindowActivity, shareWindowArray, windowActivityIdentity } from "../model/windowActivity.js";
import { summaryFromWindowDetail } from "../model/windowSummary.js";

const DEFAULT_FULL_DETAIL_REFRESH_MS = 30_000;

function mergePrimaryWindowDetail(
  current: WindowDetail | null,
  primary: WindowDetail,
  preserveCurrentLiveness = false,
): WindowDetail {
  if (!current || current.client_window_key !== primary.client_window_key) return primary;
  const currentIsNewer = current.last_seen_at_ms > primary.last_seen_at_ms ||
    (current.last_seen_at_ms === primary.last_seen_at_ms && preserveCurrentLiveness);
  const merged = mergeWindowActivity(current.activity, primary.activity, !currentIsNewer);
  if (current.detail_level !== "full") return {
    ...(currentIsNewer ? current : primary),
    label: preserveCurrentLiveness || !Object.hasOwn(primary, "label") ? current.label : primary.label,
    active_requests: currentIsNewer ? current.active_requests : shareWindowArray(current.active_requests, primary.active_requests),
    activity: merged.activity, activity_returned: merged.activity.length,
    activity_truncated: current.activity_truncated || primary.activity_truncated || merged.truncated,
  };
  return {
    ...current,
    label: preserveCurrentLiveness || !Object.hasOwn(primary, "label") ? current.label : primary.label,
    source: currentIsNewer ? current.source || primary.source : primary.source || current.source,
    last_seen_at_ms: Math.max(current.last_seen_at_ms, primary.last_seen_at_ms),
    last_tool_call_at_ms:
      Math.max(current.last_tool_call_at_ms || 0, primary.last_tool_call_at_ms || 0) || undefined,
    last_meaningful_activity_at_ms:
      Math.max(
        current.last_meaningful_activity_at_ms || 0,
        primary.last_meaningful_activity_at_ms || 0,
      ) || undefined,
    active_count: currentIsNewer ? current.active_count : primary.active_count,
    active_requests: currentIsNewer ? current.active_requests : shareWindowArray(current.active_requests, primary.active_requests),
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
  preserveCurrentLiveness = false,
): WindowDetail {
  if (!current || current.client_window_key !== full.client_window_key) {
    return { ...full, detail_level: "full" };
  }
  const currentIsNewer = current.last_seen_at_ms > full.last_seen_at_ms ||
    (current.last_seen_at_ms === full.last_seen_at_ms && preserveCurrentLiveness);
  const merged = mergeWindowActivity(current.activity, full.activity, !currentIsNewer);
  return {
    ...full,
    label: preserveCurrentLiveness || !Object.hasOwn(full, "label") ? current.label : full.label,
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
    active_requests: currentIsNewer ? current.active_requests : shareWindowArray(current.active_requests, full.active_requests),
    linked_sessions: shareWindowArray(current.linked_sessions, full.linked_sessions),
    jobs: full.jobs ? shareWindowArray(current.jobs || [], full.jobs) : full.jobs,
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
  updatedAt: number | null;
  labelSaving: boolean;
  labelError: string | null;
  saveLabel: (key: string, label: string | null) => Promise<boolean>;
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
    autoSelect?: boolean;
    initialWindowKey?: string;
    projects?: string[];
  } = {},
): WindowWorkspaceState {
  const refreshMs = options.refreshMs ?? 3_000;
  const [selectedKey, setSelectedKey] = useState(options.initialWindowKey || "");
  const inventory = useWindowInventory(client, enabled, onUnauthorized, { refreshMs, projects: options.projects, detailWindowKey: selectedKey });
  const { availability, windows, total, truncated, scope, removeWindow, updateWindow, beginObservation } = inventory;
  const fullDetailRefreshMs = options.fullDetailRefreshMs ?? DEFAULT_FULL_DETAIL_REFRESH_MS;
  const loadDetail = options.loadDetail ?? true;
  const authorityRevision = client.authorityRevision ?? 0;
  const selectionKey = JSON.stringify(options.projects ? [...new Set(options.projects)].sort() : null);
  const detailOwner = useRef({ client, authorityRevision, selectionKey, selectedKey, enabled, loadDetail });
  const [detailAvailability, setDetailAvailability] = useState<Availability>("idle");
  const [detailHydrating, setDetailHydrating] = useState(false);
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
  const fullDetailAttemptedAt = useRef(Number.NEGATIVE_INFINITY);
  const historyStale = useRef(false);
  const acceptedDetailSequence = useRef(0);

  const [labelMutation, setLabelMutation] = useState<{ key: string; label: string | null } | null>(null);
  const [labelFailure, setLabelFailure] = useState<{ key: string; message: string } | null>(null);
  const labelRequest = useRef<{ controller: AbortController; key: string; previous: string | null } | null>(null);
  const latest = useRef({ windows, detail, selectedKey }); latest.current = { windows, detail, selectedKey };
  const applyLabel = useCallback((key: string, label: string | null) => {
    const sequence = inventory.updateLabel(key, label);
    if (latest.current.selectedKey === key) acceptedDetailSequence.current = Math.max(sequence, acceptedDetailSequence.current);
    setDetail(current => current?.client_window_key === key ? { ...current, label } : current);
  }, [inventory.updateLabel]);
  const saveLabel = useCallback(async (key: string, label: string | null) => {
    const pending = labelRequest.current;
    const previous = pending?.key === key ? pending.previous
      : latest.current.windows.find(row => row.client_window_key === key)?.label ??
        (latest.current.detail?.client_window_key === key ? latest.current.detail.label ?? null : null);
    pending?.controller.abort();
    const request = { controller: new AbortController(), key, previous };
    labelRequest.current = request;
    setLabelFailure(null); setLabelMutation({ key, label });
    try {
      const response = await setWindowLabel(client, key, label, request.controller.signal);
      if (request.controller.signal.aborted || labelRequest.current !== request) return false;
      if (response?.ok && response.data?.client_window_key === key && Object.hasOwn(response.data, "label")) {
        applyLabel(key, response.data.label); setLabelMutation(null); return true;
      }
      applyLabel(key, previous); setLabelMutation(null);
      setLabelFailure({ key, message: response?.status === 403 ? "You do not have permission to rename this Window."
        : response?.status === 404 ? "This Window is no longer visible."
        : response?.status === 409 ? "This Window has no recorded activity yet. Retry after its next call." : "Could not save the name. Please retry." });
      if (response?.status === 401) onUnauthorized();
      return false;
    } catch {
      if (request.controller.signal.aborted || labelRequest.current !== request) return false;
      applyLabel(key, previous); setLabelMutation(null);
      setLabelFailure({ key, message: "Could not save the name. Please retry." }); return false;
    } finally { if (labelRequest.current === request) labelRequest.current = null; }
  }, [client, authorityRevision, applyLabel, onUnauthorized]);
  useEffect(() => {
    setLabelMutation(null); setLabelFailure(null);
    return () => { labelRequest.current?.controller.abort(); labelRequest.current = null; };
  }, [client, authorityRevision]);

  const fenceDetailLabel = inventory.fenceLabel;

  const refresh = useCallback(() => {
    inventory.refresh();
    if (!detailRequest.current) setDetailRevision(value => value + 1);
  }, [inventory.refresh]);

  useEffect(() => {
    if (options.autoSelect !== false) setSelectedKey(current => current || String(windows[0]?.client_window_key || ""));
  }, [windows, options.autoSelect]);

  useEffect(() => {
    if (availability === "denied") {
      detailRequest.current?.abort(); detailRequest.current = null;
      fullDetailRequest.current?.controller.abort(); fullDetailRequest.current = null;
      setDetailHydrating(false);
      setSelectedKey(""); setDetail(null); setDetailAvailability("denied");
    }
  }, [availability]);

  const hydrateFullDetail = useCallback((key: string) => {
    if (!enabled || !loadDetail || !key || fullDetailRequest.current || document.visibilityState === "hidden") return;
    // Failed/catch-up reads share the full-history cadence; a 503 cannot turn
    // each cheap primary tick into another 2,000-row history request.
    if (Date.now() - fullDetailAttemptedAt.current < fullDetailRefreshMs) return;
    fullDetailAttemptedAt.current = Date.now();
    const sequence = beginObservation();
    const controller = new AbortController();
    fullDetailRequest.current = { key, controller };
    fullDetailNeedsCatchup.current = false;
    setDetailHydrating(true);
    const failed = () => {
      fullDetailNeedsCatchup.current = true; historyStale.current = true;
      setDetailAvailability(current => current === "available" || current === "stale" ? "stale" : "error");
    };
    void fetchWindowDetail(client, key, controller.signal).then((response) => {
      if (fullDetailRequest.current?.controller !== controller || controller.signal.aborted) return;
      fullDetailRequest.current = null;
      setDetailHydrating(false);
      if (!response) { failed(); return; }
      if (response.status === 401 || response.status === 403 || response.status === 404) {
        detailRequest.current?.abort(); detailRequest.current = null;
        setDetail(null); setDetailAvailability("denied");
        if (response.status === 401) onUnauthorized();
        if (response.status === 404) removeWindow(key);
        return;
      }
      if (!response.ok || !response.data || response.data.client_window_key !== key) { failed(); return; }
      const preserveCurrentLiveness = sequence < acceptedDetailSequence.current;
      acceptedDetailSequence.current = Math.max(sequence, acceptedDetailSequence.current);
      fullDetailLoadedKey.current = key;
      fullDetailLoadedAt.current = Date.now();
      historyStale.current = false;
      const incoming = fenceDetailLabel(response.data, sequence);
      setDetail((current) => mergeFullWindowDetail(current, incoming, preserveCurrentLiveness));
      updateWindow(summaryFromWindowDetail({ ...incoming, detail_level: "full" }), sequence);
      setDetailAvailability("available");
    }).catch(() => {
      if (fullDetailRequest.current?.controller !== controller || controller.signal.aborted) return;
      fullDetailRequest.current = null; setDetailHydrating(false); failed();
    });
  }, [client, authorityRevision, selectionKey, enabled, loadDetail, onUnauthorized, removeWindow, updateWindow, beginObservation, fullDetailRefreshMs, fenceDetailLabel]);

  useEffect(() => {
    detailOwner.current = { client, authorityRevision, selectionKey, selectedKey, enabled, loadDetail };
    setDetail(null);
    setDetailAvailability(enabled && loadDetail && selectedKey ? "loading" : "idle");
    fullDetailRequest.current?.controller.abort();
    fullDetailRequest.current = null;
    fullDetailLoadedKey.current = "";
    fullDetailLoadedAt.current = 0;
    lastPrimaryActivity.current = null;
    fullDetailNeedsCatchup.current = false;
    fullDetailAttemptedAt.current = Number.NEGATIVE_INFINITY;
    historyStale.current = false; acceptedDetailSequence.current = 0;
    setDetailHydrating(false);
  }, [client, authorityRevision, selectionKey, enabled, loadDetail, selectedKey]);

  useEffect(() => {
    detailRequest.current?.abort();
    detailRequest.current = null;
    if (!enabled || !loadDetail || !selectedKey) {
      setDetail(null);
      setDetailAvailability("idle");
      return;
    }
    if (document.visibilityState === "hidden") return;
    const sequence = beginObservation();
    const controller = new AbortController();
    detailRequest.current = controller;
    setDetailAvailability((value) => (value === "idle" ? "loading" : value));
    void fetchWindowPrimaryDetail(client, selectedKey, controller.signal).then((response) => {
      if (detailRequest.current !== controller || controller.signal.aborted) return;
      detailRequest.current = null;
      if (!response) {
        setDetailAvailability(current => current === "available" || current === "stale" ? "stale" : "error");
        return;
      }
      if (response.status === 401) {
        fullDetailRequest.current?.controller.abort(); fullDetailRequest.current = null;
        setDetailHydrating(false); setDetail(null); setDetailAvailability("denied");
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
      const preserveCurrentLiveness = sequence < acceptedDetailSequence.current;
      acceptedDetailSequence.current = Math.max(sequence, acceptedDetailSequence.current);
      const incoming = fenceDetailLabel(response.data, sequence);
      setDetail((current) => mergePrimaryWindowDetail(current, incoming, preserveCurrentLiveness));
      updateWindow(summaryFromWindowDetail(incoming), sequence);
      setDetailAvailability(historyStale.current ? "stale" : "available");
      const fullDetailDue =
        fullDetailNeedsCatchup.current || fullDetailLoadedKey.current !== selectedKey ||
        Date.now() - fullDetailLoadedAt.current >= fullDetailRefreshMs;
      if (fullDetailDue) hydrateFullDetail(selectedKey);
    }).catch(() => {
      if (detailRequest.current !== controller || controller.signal.aborted) return;
      detailRequest.current = null;
      setDetailAvailability(current => current === "available" || current === "stale" ? "stale" : "error");
    });
    return () => {
      controller.abort();
      if (detailRequest.current === controller) detailRequest.current = null;
    };
  }, [
    client,
    authorityRevision,
    selectionKey,
    detailRevision,
    enabled,
    fullDetailRefreshMs,
    hydrateFullDetail,
    loadDetail,
    onUnauthorized,
    removeWindow,
    updateWindow,
    beginObservation,
    fenceDetailLabel,
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

  const owner = detailOwner.current;
  const currentDetail = enabled && loadDetail && availability !== "denied" && owner.client === client &&
    owner.authorityRevision === authorityRevision && owner.selectionKey === selectionKey &&
    owner.selectedKey === selectedKey && owner.enabled === enabled && owner.loadDetail === loadDetail;

  return {
    updatedAt: inventory.updatedAt,
    availability,
    detailAvailability: availability === "denied" ? "denied" : currentDetail ? detailAvailability : enabled && loadDetail ? "loading" : "idle",
    detailHydrating: currentDetail && detailHydrating,
    windows: labelMutation ? windows.map(row => row.client_window_key === labelMutation.key ? { ...row, label: labelMutation.label } : row) : windows,
    labelSaving: labelMutation?.key === selectedKey,
    labelError: labelFailure?.key === selectedKey ? labelFailure.message : null,
    saveLabel,
    historicalWindows: inventory.historicalWindows,
    loadMore: inventory.loadMore,
    loadingMore: inventory.loadingMore,
    total,
    truncated,
    scope,
    selectedKey,
    detail: currentDetail && detail?.client_window_key === selectedKey ? labelMutation?.key === selectedKey ? { ...detail, label: labelMutation.label } : detail : null,
    select,
    refresh,
  };
}
