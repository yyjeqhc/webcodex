import { useCallback, useEffect, useRef, useState } from "react";
import { fetchProjects } from "../api/projects.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, ProjectRow } from "../model/types.js";
import { useVisibleRefresh } from "./useVisibleRefresh.js";

export const PROJECT_PAGE_SIZE = 24;
export type ProjectsState = {
  availability: Availability; projects: ProjectRow[]; total: number; truncated: boolean;
  query: string; runner: string; setQuery: (value: string) => void; setRunner: (value: string) => void;
  refresh: () => void; loadMore: () => void; showLess: () => void; canShowLess: boolean; refreshing: boolean;
};

export function useProjects(client: RuntimeV2Client, enabled: boolean, onUnauthorized: () => void,
  options: { initialLimit?: number; refreshMs?: number } = {}): ProjectsState {
  const initialLimit = Math.max(PROJECT_PAGE_SIZE, Math.min(2_000, options.initialLimit ?? PROJECT_PAGE_SIZE));
  const [availability, setAvailability] = useState<Availability>("idle");
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [total, setTotal] = useState(0);
  const [truncated, setTruncated] = useState(false);
  const [query, setQuery] = useState("");
  const [runner, setRunner] = useState("");
  const [revision, setRevision] = useState(0);
  const [page, setPage] = useState({ key: "", limit: initialLimit });
  const [refreshing, setRefreshing] = useState(false);
  const listRequest = useRef<AbortController | null>(null);
  const selection = useRef({ client, key: "" });
  const refresh = useCallback(() => { if (!listRequest.current) setRevision(value => value + 1); }, []);
  const stableQuery = useDebouncedValue(query, 220);
  const key = JSON.stringify([runner, stableQuery, initialLimit]);
  const limit = page.key === key ? page.limit : initialLimit;

  useEffect(() => {
    if (selection.current.client !== client || selection.current.key !== key) {
      selection.current = { client, key };
      setProjects([]); setTotal(0); setTruncated(false); setAvailability("loading");
    }
    if (!enabled) {
      listRequest.current?.abort(); listRequest.current = null;
      setProjects([]); setTotal(0); setTruncated(false); setRefreshing(false); setAvailability("idle");
      return;
    }
    const controller = new AbortController();
    listRequest.current?.abort(); listRequest.current = controller;
    setRefreshing(true);
    setAvailability(value => value === "idle" ? "loading" : value);
    const failed = () => setAvailability(value => value === "available" || value === "stale" ? "stale" : "error");
    void fetchProjects(client, { runner, query: stableQuery, limit }, controller.signal).then(response => {
      if (controller.signal.aborted || listRequest.current !== controller) return;
      listRequest.current = null; setRefreshing(false);
      if (!response) { failed(); return; }
      if (response.status === 401) { onUnauthorized(); return; }
      if (response.status === 403) {
        setProjects([]); setTotal(0); setTruncated(false); setAvailability("denied"); return;
      }
      if (!response.ok || !response.data) { failed(); return; }
      setProjects(current => JSON.stringify(current) === JSON.stringify(response.data!.projects || []) ? current : response.data!.projects || []);
      setTotal(Math.max(response.data.total || 0, response.data.projects?.length || 0));
      setTruncated(Boolean(response.data.truncated)); setAvailability("available");
    }).catch(() => {
      if (controller.signal.aborted || listRequest.current !== controller) return;
      listRequest.current = null; setRefreshing(false); failed();
    });
    return () => { controller.abort(); if (listRequest.current === controller) listRequest.current = null; };
  }, [client, enabled, onUnauthorized, revision, runner, stableQuery, limit, key]);

  useVisibleRefresh(enabled, refresh, options.refreshMs ?? 30_000);
  return {
    availability, projects, total, truncated, query, runner, setQuery, setRunner, refresh, refreshing,
    loadMore: () => { if (!listRequest.current) setPage({ key, limit: Math.min(2_000, limit + PROJECT_PAGE_SIZE) }); },
    showLess: () => { if (!listRequest.current) setPage({ key, limit: PROJECT_PAGE_SIZE }); },
    canShowLess: limit > PROJECT_PAGE_SIZE,
  };
}

function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), delayMs);
    return () => window.clearTimeout(timer);
  }, [delayMs, value]);
  return debounced;
}
