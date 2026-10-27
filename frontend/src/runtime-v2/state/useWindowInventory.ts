import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { RuntimeV2Client } from "../api/client.js";
import { fetchWindowLiveness, fetchWindows, WINDOW_PAGE_SIZE, type WindowSelection } from "../api/windows.js";
import type { Availability, WindowSummary } from "../model/types.js";
import { mergeWindowSummary, type WindowObservation } from "../model/windowSummary.js";
import { useVisibleRefresh } from "./useVisibleRefresh.js";

const sameRows = (a: WindowSummary[], b: WindowSummary[]) => JSON.stringify(a) === JSON.stringify(b);
const unique = (rows: WindowSummary[]) => [...new Map(rows.map(row => [row.client_window_key, row])).values()];

/** Low-frequency paged evidence and high-frequency live state have separate
 * requests, ownership fences and state. A live tick never reloads history. */
export function useWindowInventory(client: RuntimeV2Client, enabled: boolean, onUnauthorized: () => void,
  options: WindowSelection & { refreshMs?: number; inventoryRefreshMs?: number; detailWindowKey?: string } = {}) {
  const key = JSON.stringify([options.projects ? [...new Set(options.projects)].sort() : null, options.query || "", options.client_window_key || null]);
  const selection = useMemo<WindowSelection>(() => {
    const [projects, query, windowKey] = JSON.parse(key) as [string[] | null, string, string | null];
    return { ...(projects ? { projects } : {}), ...(query ? { query } : {}), ...(windowKey ? { client_window_key: windowKey } : {}) };
  }, [key]);
  const authorityRevision = client.authorityRevision ?? 0;
  const owner = useRef({ client, key, authorityRevision });
  const [updatedAt, setUpdatedAt] = useState<number | null>(null);
  const [history, setHistory] = useState<WindowSummary[]>([]);
  const [recent, setRecent] = useState<WindowSummary[]>([]);
  const [live, setLive] = useState<WindowSummary[] | null>(null);
  const [liveVersion, setLiveVersion] = useState(0);
  const [total, setTotal] = useState(0);
  const [next, setNext] = useState<number>();
  const [scope, setScope] = useState<"global" | "principal">("principal");
  const [availability, setAvailability] = useState<Availability>("idle");
  const [loadingMore, setLoadingMore] = useState(false);
  const [liveUnavailable, setLiveUnavailable] = useState(false);
  const observationSequence = useRef(0);
  const observedByKey = useRef(new Map<string, number>());
  const lastLive = useRef<WindowSummary[]>([]);
  const liveSequence = useRef(0);
  const beginObservation = useCallback(() => ++observationSequence.current, []);
  const completionRequest = useRef<AbortController | null>(null);
  const inventoryRequest = useRef<AbortController | null>(null);
  const liveRequest = useRef<AbortController | null>(null);
  const snapshot = useRef({ history, next }); snapshot.current = { history, next };
  const clear = useCallback(() => {
    setUpdatedAt(null); setHistory([]); setRecent([]); setLive(null); setTotal(0); setNext(undefined); setLiveUnavailable(false);
    lastLive.current = []; liveSequence.current = 0; observedByKey.current.clear();
    completionRequest.current?.abort(); completionRequest.current = null;
  }, []);
  const denied = useCallback((status: number) => {
    if (status !== 401 && status !== 403 && status !== 404) return false;
    inventoryRequest.current?.abort(); inventoryRequest.current = null;
    liveRequest.current?.abort(); liveRequest.current = null;
    clear(); setAvailability("denied");
    if (status === 401) onUnauthorized();
    return true;
  }, [clear, onUnauthorized]);
  const failed = useCallback(() => setAvailability(value => value === "available" || value === "stale" ? "stale" : "error"), []);

  const updateWindow = useCallback((incoming: WindowObservation, sequence = ++observationSequence.current) => {
    const preferIncoming = sequence >= (observedByKey.current.get(incoming.client_window_key) || 0);
    observedByKey.current.set(incoming.client_window_key, Math.max(sequence, observedByKey.current.get(incoming.client_window_key) || 0));
    const merge = (current: WindowSummary[]) => {
      const rows = current.map(row => row.client_window_key === incoming.client_window_key ? mergeWindowSummary(row, incoming, preferIncoming) : row);
      return rows.every((row, index) => row === current[index]) ? current : rows;
    };
    // Exact detail refreshes update existing inventory rows; they never insert
    // an omitted/unauthorized Window into a filtered inventory.
    setHistory(merge); setRecent(merge);
  }, []);

  const load = useCallback((append: boolean) => {
    if (!enabled || inventoryRequest.current || document.visibilityState === "hidden") return;
    const offset = append ? snapshot.current.next : 0;
    if (offset === undefined) return;
    const controller = new AbortController(); inventoryRequest.current = controller;
    const sequence = ++observationSequence.current;
    setLoadingMore(append); setAvailability(value => value === "idle" ? "loading" : value);
    // Explicit load-more fetches one new page. Periodic refresh revalidates only
    // the prefix the user has actually opened, not a fixed 2,000-row inventory.
    const limit = append ? WINDOW_PAGE_SIZE : Math.max(WINDOW_PAGE_SIZE, snapshot.current.history.length);
    const readPrefix = async () => {
      let response = await fetchWindows(client, { ...selection, offset, limit: Math.min(2_000, limit) }, controller.signal);
      if (append || limit <= 2_000 || !response?.ok || !response.data) return response;
      let rows = response.data.windows || [];
      let cursor = response.data.next_offset ?? (response.data.truncated && rows.length ? rows.length : undefined);
      // Refresh only pages the user opened, including a prefix exceeding one
      // server response. Never silently discard loaded pages at the 2,000 cap.
      while (rows.length < limit && cursor !== undefined && !controller.signal.aborted) {
        const page = await fetchWindows(client, { ...selection, offset: cursor, limit: Math.min(2_000, limit - rows.length) }, controller.signal);
        if (!page?.ok || !page.data) return page;
        const addition = page.data.windows || [];
        const nextCursor: number | undefined = page.data.next_offset ?? (page.data.truncated && addition.length ? cursor + addition.length : undefined);
        if (!addition.length || (nextCursor !== undefined && nextCursor <= cursor)) break;
        rows = [...rows, ...addition]; cursor = nextCursor;
        response = { ...page, data: { ...page.data, windows: rows, returned: rows.length, next_offset: cursor } };
      }
      return response;
    };
    void readPrefix().then(response => {
      if (controller.signal.aborted || inventoryRequest.current !== controller) return;
      inventoryRequest.current = null; setLoadingMore(false);
      if (!response) { failed(); return; }
      if (denied(response.status)) return;
      if (!response.ok || !response.data) { failed(); return; }
      const data = response.data;
      const rows = data.windows || [];
      setHistory(current => {
        const byKey = new Map(current.map(row => [row.client_window_key, row]));
        const merged = rows.map(row => {
          const previous = byKey.get(row.client_window_key);
          const preferIncoming = sequence >= (observedByKey.current.get(row.client_window_key) || 0);
          observedByKey.current.set(row.client_window_key, Math.max(sequence, observedByKey.current.get(row.client_window_key) || 0));
          return previous ? mergeWindowSummary(previous, row, preferIncoming) : row;
        });
        const nextRows = append ? unique([...current, ...merged]) : merged;
        return sameRows(current, nextRows) ? current : nextRows;
      });
      if (!append) {
        setRecent([]);
        const retained = new Set(rows.map(row => row.client_window_key));
        for (const key of observedByKey.current.keys()) if (!retained.has(key)) observedByKey.current.delete(key);
      }
      setTotal(data.total || 0);
      setNext(data.next_offset ?? (data.truncated && rows.length ? offset + rows.length : undefined));
      setScope(data.visibility?.scope === "global" ? "global" : "principal");
      setUpdatedAt(Date.now());
      setAvailability("available");
    }).catch(() => {
      if (controller.signal.aborted || inventoryRequest.current !== controller) return;
      inventoryRequest.current = null; setLoadingMore(false); failed();
    });
  }, [client, authorityRevision, enabled, selection, denied, failed]);
  const refresh = useCallback(() => load(false), [load]);
  const loadMore = useCallback(() => load(true), [load]);
  const pauseInventory = useCallback(() => { inventoryRequest.current?.abort(); inventoryRequest.current = null; setLoadingMore(false); }, []);
  const pauseLive = useCallback(() => {
    liveRequest.current?.abort(); liveRequest.current = null;
    completionRequest.current?.abort(); completionRequest.current = null;
  }, []);
  const refreshCompleted = useCallback((keys: string[]) => {
    if (!keys.length || completionRequest.current || inventoryRequest.current) return;
    const controller = new AbortController(); completionRequest.current = controller;
    // One bounded serial catch-up, only for observed active -> idle transitions.
    // Larger bursts are covered by the ordinary 30-second inventory refresh.
    void (async () => {
      for (const key of keys.slice(0, 4)) {
        const sequence = ++observationSequence.current;
        const response = await fetchWindows(client, { ...selection, client_window_key: key, offset: 0, limit: 1 }, controller.signal);
        if (controller.signal.aborted || completionRequest.current !== controller) return;
        if (response && denied(response.status)) return;
        if (!response?.ok || !response.data) { failed(); break; }
        const row = response.data.windows?.find(row => row.client_window_key === key);
        if (row) updateWindow(row, sequence);
        else {
          setHistory(rows => rows.filter(row => row.client_window_key !== key));
          setRecent(rows => rows.filter(row => row.client_window_key !== key));
        }
      }
    })().catch(() => { if (!controller.signal.aborted) failed(); }).finally(() => {
      if (completionRequest.current === controller) completionRequest.current = null;
    });
  }, [client, authorityRevision, selection, denied, failed, updateWindow]);
  const refreshLive = useCallback(() => {
    if (!enabled || liveRequest.current || document.visibilityState === "hidden") return;
    const controller = new AbortController(); liveRequest.current = controller;
    const sequence = ++observationSequence.current;
    void fetchWindowLiveness(client, selection, controller.signal).then(response => {
      if (controller.signal.aborted || liveRequest.current !== controller) return;
      liveRequest.current = null;
      if (!response) { setLiveUnavailable(true); return; }
      if (denied(response.status)) return;
      if (!response.ok || !response.data) { setLiveUnavailable(true); return; }
      setLiveUnavailable(false); setUpdatedAt(Date.now());
      const rows = response.data.windows || [];
      const activeKeys = new Set(rows.filter(row => row.active_count > 0).map(row => row.client_window_key));
      const completed = lastLive.current.filter(row => row.active_count > 0 && !activeKeys.has(row.client_window_key) && row.client_window_key !== options.detailWindowKey).map(row => row.client_window_key);
      lastLive.current = rows; liveSequence.current = sequence; setLiveVersion(sequence);
      setLive(current => current && sameRows(current, rows) ? current : rows);
      // Retain Server request times after a live row disappears. Status/counters
      // remain owned by the live snapshot until exact/periodic history catches up.
      setHistory(current => {
        const byKey = new Map(rows.map(row => [row.client_window_key, row]));
        const retained = current.map(row => {
          const active = byKey.get(row.client_window_key);
          return active && active.last_seen_at_ms > row.last_seen_at_ms
            ? mergeWindowSummary(row, { ...active, active_count: row.active_count, last_activity_status: row.last_activity_status }) : row;
        });
        return retained.every((row, index) => row === current[index]) ? current : retained;
      });
      refreshCompleted(completed);
      const known = new Set(snapshot.current.history.map(row => row.client_window_key));
      const added = rows.filter(row => !known.has(row.client_window_key));
      // New live Windows become visible immediately and survive completion until
      // the next authorized inventory snapshot, without refreshing all history.
      if (added.length) setRecent(current => {
        const byKey = new Map(current.map(row => [row.client_window_key, row]));
        const merged = added.map(row => {
          const previous = byKey.get(row.client_window_key);
          const preferIncoming = sequence >= (observedByKey.current.get(row.client_window_key) || 0);
          return previous ? mergeWindowSummary(previous, row, preferIncoming) : row;
        });
        const nextRows = unique([...current, ...merged]).slice(-128);
        return sameRows(current, nextRows) ? current : nextRows;
      });
    }).catch(() => {
      if (liveRequest.current === controller) { liveRequest.current = null; if (!controller.signal.aborted) setLiveUnavailable(true); }
    });
  }, [client, authorityRevision, enabled, selection, denied, options.detailWindowKey, refreshCompleted]);

  useEffect(() => {
    owner.current = { client, key, authorityRevision }; pauseInventory(); pauseLive(); clear();
    setAvailability(enabled ? "loading" : "idle");
    snapshot.current = { history: [], next: undefined };
    if (enabled) load(false);
    return () => { pauseInventory(); pauseLive(); };
  }, [client, key, authorityRevision, enabled, load, clear, pauseInventory, pauseLive]);
  useVisibleRefresh(enabled, refresh, options.inventoryRefreshMs ?? 30_000, pauseInventory);
  useVisibleRefresh(enabled, refreshLive, options.refreshMs ?? 3_000, pauseLive);

  const historicalWindows = useMemo(() => unique([...history, ...recent]), [history, recent]);
  const liveByKey = useMemo(() => new Map((live || []).map(row => [row.client_window_key, row])), [live]);
  const windows = useMemo(() => historicalWindows.map(row => {
    const active = liveByKey.get(row.client_window_key);
    if (live === null || (observedByKey.current.get(row.client_window_key) || 0) > liveSequence.current) return row;
    return active ? mergeWindowSummary(row, { ...active, last_activity_status: "running" })
      : row.active_count || row.last_activity_status === "running"
        ? { ...row, active_count: 0, last_activity_status: row.last_activity_status === "running" ? undefined : row.last_activity_status }
        : row;
  }), [historicalWindows, liveByKey, live, liveVersion]);
  const removeWindow = useCallback((key: string) => {
    setHistory(rows => rows.filter(row => row.client_window_key !== key));
    setRecent(rows => rows.filter(row => row.client_window_key !== key));
    setLive(rows => rows?.filter(row => row.client_window_key !== key) ?? null);
  }, []);
  const current = enabled && owner.current.client === client && owner.current.key === key && owner.current.authorityRevision === authorityRevision;
  return { availability: current ? (liveUnavailable && availability === "available" ? "stale" as const : availability) : enabled ? "loading" as const : "idle" as const, windows: current ? windows : [], historicalWindows: current ? historicalWindows : [],
    liveByKey: current ? liveByKey : new Map<string, WindowSummary>(), total: current ? total : 0, truncated: current && next !== undefined,
    updatedAt: current ? updatedAt : null,
    scope: current ? scope : "principal" as const, refresh, loadMore, loadingMore: current && loadingMore, removeWindow, updateWindow, beginObservation };
}
