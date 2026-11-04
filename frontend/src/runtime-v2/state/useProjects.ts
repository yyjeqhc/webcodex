import { useEffect, useRef, useState } from "react";
import { fetchProjects } from "../api/projects.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, ProjectRow } from "../model/types.js";
import { useInventoryRefresh } from "./useInventoryRefresh.js";

export const PROJECT_PAGE_SIZE = 24;
export type ProjectsState = {
  availability: Availability; projects: ProjectRow[]; total: number; truncated: boolean;
  query: string; runner: string; setQuery: (value: string) => void; setRunner: (value: string) => void;
  refresh: () => void; loadMore: () => void; showLess: () => void; canShowLess: boolean; refreshing: boolean;
};

export function useProjects(client: RuntimeV2Client, enabled: boolean, onUnauthorized: () => void,
  options: { initialLimit?: number; refreshMs?: number; inventoryKey?: string } = {}): ProjectsState {
  const initialLimit = Math.max(PROJECT_PAGE_SIZE, Math.min(2_000, options.initialLimit ?? PROJECT_PAGE_SIZE));
  const [availability, setAvailability] = useState<Availability>("idle");
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [total, setTotal] = useState(0);
  const [truncated, setTruncated] = useState(false);
  const [query, setQuery] = useState("");
  const [runner, setRunner] = useState("");
  const [page, setPage] = useState({ key: "", limit: initialLimit });
  const [refreshing, setRefreshing] = useState(false);
  const authorityRevision = client.authorityRevision ?? 0;
  const selection = useRef({ client, key: "" });
  const { request, revision, refresh, invalidate, reset, failed: deferRetry, stopAutomatic } = useInventoryRefresh(
    enabled, options.refreshMs ?? 30_000, availability !== "denied",
  );
  const stableQuery = useDebouncedValue(query, 220);
  const key = JSON.stringify([authorityRevision, runner, stableQuery, initialLimit]);
  const limit = page.key === key ? page.limit : initialLimit;

  useEffect(() => {
    request.cancel();
    if (selection.current.client !== client || selection.current.key !== key) {
      selection.current = { client, key }; reset();
      setProjects([]); setTotal(0); setTruncated(false); setAvailability("loading");
    }
    if (!enabled) {
      reset();
      setProjects([]); setTotal(0); setTruncated(false); setRefreshing(false); setAvailability("idle");
      return;
    }
    setRefreshing(true);
    setAvailability(value => value === "idle" ? "loading" : value);
    const failed = () => {
      deferRetry();
      setAvailability(value => value === "available" || value === "stale" ? "stale" : "error");
    };
    void request.run(signal => fetchProjects(client, { runner, query: stableQuery, limit }, signal), response => {
      setRefreshing(false);
      if (!response) { failed(); return; }
      if (response.status === 401) { stopAutomatic(); if ((client.authorityRevision ?? 0) === authorityRevision) onUnauthorized(); return; }
      if (response.status === 403) {
        stopAutomatic();
        setProjects([]); setTotal(0); setTruncated(false); setAvailability("denied"); return;
      }
      if (!response.ok || !response.data) { failed(); return; }
      reset();
      setProjects(current => JSON.stringify(current) === JSON.stringify(response.data!.projects || []) ? current : response.data!.projects || []);
      setTotal(Math.max(response.data.total || 0, response.data.projects?.length || 0));
      setTruncated(Boolean(response.data.truncated)); setAvailability("available");
    });
    return () => request.cancel();
  }, [client, authorityRevision, enabled, onUnauthorized, revision, runner, stableQuery, limit, key, request, reset, deferRetry, stopAutomatic]);

  const inventory = useRef({ client, key, enabled, value: options.inventoryKey });
  useEffect(() => {
    const previous = inventory.current;
    inventory.current = { client, key, enabled, value: options.inventoryKey };
    if (enabled && previous.enabled && previous.client === client && previous.key === key && previous.value !== undefined
      && options.inventoryKey !== undefined && previous.value !== options.inventoryKey) invalidate();
  }, [client, enabled, key, options.inventoryKey, invalidate]);
  const visible = enabled && selection.current.client === client && selection.current.key === key;
  return {
    availability: visible ? availability : enabled ? "loading" : "idle",
    projects: visible ? projects : [], total: visible ? total : 0, truncated: visible && truncated,
    query, runner, setQuery, setRunner, refresh, refreshing: enabled && (visible ? refreshing : true),
    loadMore: () => { if (!request.pending) setPage({ key, limit: Math.min(2_000, limit + PROJECT_PAGE_SIZE) }); },
    showLess: () => { if (!request.pending) setPage({ key, limit: PROJECT_PAGE_SIZE }); },
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