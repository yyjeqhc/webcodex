import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { RuntimeV2Client } from "../api/client.js";
import { fetchWindowLiveness, fetchWindows, WINDOW_PAGE_SIZE, type WindowSelection } from "../api/windows.js";
import type { Availability, WindowSummary } from "../model/types.js";
import { useVisibleRefresh } from "./useVisibleRefresh.js";

const sameRows = (a: WindowSummary[], b: WindowSummary[]) => JSON.stringify(a) === JSON.stringify(b);
const unique = (rows: WindowSummary[]) => [...new Map(rows.map(row => [row.client_window_key, row])).values()];

/** Low-frequency paged evidence and high-frequency live state have separate
 * requests, ownership fences and state. A live tick never reloads history. */
export function useWindowInventory(client: RuntimeV2Client, enabled: boolean, onUnauthorized: () => void,
  options: WindowSelection & { refreshMs?: number; inventoryRefreshMs?: number } = {}) {
  const key = JSON.stringify([options.projects ? [...new Set(options.projects)].sort() : null, options.query || "", options.client_window_key || null]);
  const selection = useMemo<WindowSelection>(() => {
    const [projects, query, windowKey] = JSON.parse(key) as [string[] | null, string, string | null];
    return { ...(projects ? { projects } : {}), ...(query ? { query } : {}), ...(windowKey ? { client_window_key: windowKey } : {}) };
  }, [key]);
  const owner = useRef({ client, key });
  const [history, setHistory] = useState<WindowSummary[]>([]);
  const [recent, setRecent] = useState<WindowSummary[]>([]);
  const [live, setLive] = useState<WindowSummary[] | null>(null);
  const [total, setTotal] = useState(0);
  const [next, setNext] = useState<number>();
  const [scope, setScope] = useState<"global" | "principal">("principal");
  const [availability, setAvailability] = useState<Availability>("idle");
  const [loadingMore, setLoadingMore] = useState(false);
  const inventoryRequest = useRef<AbortController | null>(null);
  const liveRequest = useRef<AbortController | null>(null);
  const snapshot = useRef({ history, next }); snapshot.current = { history, next };
  const clear = useCallback(() => {
    setHistory([]); setRecent([]); setLive(null); setTotal(0); setNext(undefined);
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

  const load = useCallback((append: boolean) => {
    if (!enabled || inventoryRequest.current || document.visibilityState === "hidden") return;
    const offset = append ? snapshot.current.next : 0;
    if (offset === undefined) return;
    const controller = new AbortController(); inventoryRequest.current = controller;
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
        const nextRows = append ? unique([...current, ...rows]) : rows;
        return sameRows(current, nextRows) ? current : nextRows;
      });
      if (!append) setRecent([]);
      setTotal(data.total || 0);
      setNext(data.next_offset ?? (data.truncated && rows.length ? offset + rows.length : undefined));
      setScope(data.visibility?.scope === "global" ? "global" : "principal");
      setAvailability("available");
    }).catch(() => {
      if (controller.signal.aborted || inventoryRequest.current !== controller) return;
      inventoryRequest.current = null; setLoadingMore(false); failed();
    });
  }, [client, enabled, selection, denied, failed]);
  const refresh = useCallback(() => load(false), [load]);
  const loadMore = useCallback(() => load(true), [load]);
  const pauseInventory = useCallback(() => { inventoryRequest.current?.abort(); inventoryRequest.current = null; setLoadingMore(false); }, []);
  const pauseLive = useCallback(() => { liveRequest.current?.abort(); liveRequest.current = null; }, []);
  const refreshLive = useCallback(() => {
    if (!enabled || liveRequest.current || document.visibilityState === "hidden") return;
    const controller = new AbortController(); liveRequest.current = controller;
    void fetchWindowLiveness(client, selection, controller.signal).then(response => {
      if (controller.signal.aborted || liveRequest.current !== controller) return;
      liveRequest.current = null;
      if (!response) return;
      if (denied(response.status)) return;
      if (!response.ok || !response.data) return;
      const rows = response.data.windows || [];
      setLive(current => current && sameRows(current, rows) ? current : rows);
      const known = new Set(snapshot.current.history.map(row => row.client_window_key));
      const added = rows.filter(row => !known.has(row.client_window_key));
      // New live Windows become visible immediately and survive completion until
      // the next authorized inventory snapshot, without refreshing all history.
      if (added.length) setRecent(current => {
        const nextRows = unique([...current, ...added]).slice(-128);
        return sameRows(current, nextRows) ? current : nextRows;
      });
    }).catch(() => { if (liveRequest.current === controller) liveRequest.current = null; });
  }, [client, enabled, selection, denied]);

  useEffect(() => {
    owner.current = { client, key }; pauseInventory(); pauseLive(); clear();
    setAvailability(enabled ? "loading" : "idle");
    snapshot.current = { history: [], next: undefined };
    if (enabled) load(false);
    return () => { pauseInventory(); pauseLive(); };
  }, [client, key, enabled, load, clear, pauseInventory, pauseLive]);
  useVisibleRefresh(enabled, refresh, options.inventoryRefreshMs ?? 30_000, pauseInventory);
  useVisibleRefresh(enabled, refreshLive, options.refreshMs ?? 3_000, pauseLive);

  const historicalWindows = useMemo(() => unique([...history, ...recent]), [history, recent]);
  const liveByKey = useMemo(() => new Map((live || []).map(row => [row.client_window_key, row])), [live]);
  const windows = useMemo(() => historicalWindows.map(row => {
    const active = liveByKey.get(row.client_window_key);
    if (live === null) return row;
    return { ...row, active_count: active?.active_count || 0,
      last_seen_at_ms: Math.max(row.last_seen_at_ms, active?.last_seen_at_ms || 0),
      last_project: active?.last_project || row.last_project,
      last_activity_name: active?.last_activity_name || row.last_activity_name,
      last_activity_status: active ? "running" : row.last_activity_status === "running" ? undefined : row.last_activity_status,
    };
  }), [historicalWindows, liveByKey, live]);
  const removeWindow = useCallback((key: string) => {
    setHistory(rows => rows.filter(row => row.client_window_key !== key));
    setRecent(rows => rows.filter(row => row.client_window_key !== key));
    setLive(rows => rows?.filter(row => row.client_window_key !== key) ?? null);
  }, []);
  const current = enabled && owner.current.client === client && owner.current.key === key;
  return { availability: current ? availability : enabled ? "loading" as const : "idle" as const, windows: current ? windows : [], historicalWindows: current ? historicalWindows : [],
    liveByKey, total, truncated: next !== undefined, scope, refresh, loadMore, loadingMore, removeWindow };
}
